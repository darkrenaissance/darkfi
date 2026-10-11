/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

#![cfg(not(target_os = "android"))]

use super::*;
use crate::{
    app::node::{create_multiline_edit, create_singleline_edit},
    expr,
    gfx::{AnimId, EpochIndex, GraphicsMethod},
    prop::{Property, PropertyPermission, PropertySubType, PropertyType},
    scene::{SceneNode, SceneNodeType},
};

const COLORS: &[&str] = &[
    "text_color",
    "text_hi_color",
    "placeholder_color",
    "cursor_color",
    "hi_bg_color",
    "action_fg_color",
    "action_bg_color",
];
const SPACING: &[(&str, &[f32])] = &[
    ("font_size", &[24.]),
    ("lineheight", &[1.5]),
    ("padding", &[3., 11., 5., 17.]),
    ("action_padding", &[6.]),
    ("action_spacing", &[13.]),
];

struct Fixture {
    edit: BaseEditPtr,
    node: SceneNodePtr,
    sources: Vec<(&'static str, PropertyPtr)>,
    ex: ExecutorPtr,
    redraw_rx: async_channel::Receiver<()>,
    /// Must outlive the scene, animation, and queued frame GPU handles.
    method_rx: async_channel::Receiver<(EpochIndex, GraphicsMethod)>,
}

impl Fixture {
    async fn new(multiline: bool) -> Self {
        let (redraw, redraw_rx) = RedrawTrigger::new();
        let (method_tx, method_rx) = async_channel::unbounded();
        let renderer = Renderer::new(method_tx);
        let ex: ExecutorPtr = Arc::new(smol::Executor::new());
        let node = if multiline {
            create_multiline_edit("draw_test")
        } else {
            create_singleline_edit("draw_test")
        };
        {
            let atom = &mut PropertyAtomicGuard::none();
            node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
            node.set_property_bool(atom, Role::App, "is_focused", true).unwrap();
            for (i, value) in [0., 0., 180., 120.].into_iter().enumerate() {
                node.get_property("rect").unwrap().set_f32(atom, Role::App, i, value).unwrap();
            }
            if multiline {
                let range = node.get_property("height_range").unwrap();
                range.set_f32(atom, Role::App, 0, 20.).unwrap();
                range.set_f32(atom, Role::App, 1, 400.).unwrap();
            }
            node.set_property_f32(atom, Role::App, "font_size", 18.).unwrap();
            node.set_property_f32(atom, Role::App, "baseline", 14.).unwrap();
            node.set_property_f32(atom, Role::App, "cursor_descent", 6.).unwrap();
            node.set_property_str(atom, Role::App, "text", "alpha beta gamma delta epsilon zeta")
                .unwrap();
        }
        let mut scratch = SceneNode::new("scratch", SceneNodeType::Layer);
        let mut scale = Property::new(
            "scale",
            PropertyType::Float32,
            PropertySubType::Null,
            PropertyPermission::default(),
        );
        scale.set_defaults_f32(vec![1.]).unwrap();
        scratch.add_property(scale).unwrap();
        let scale = PropertyFloat32::wrap(&scratch, Role::App, "scale", 0).unwrap();
        let edit_type = if multiline { BaseEditType::MultiLine } else { BaseEditType::SingleLine };
        let node = node
            .setup(|me| BaseEdit::new(me, scale, renderer, redraw, edit_type, ex.clone()))
            .await;
        let Pimpl::Edit(edit) = node.pimpl() else { panic!() };
        let edit = edit.clone();
        edit.on_text_prop_changed();

        let mut sources = vec![];
        for (name, values) in COLORS
            .iter()
            .map(|name| (*name, &[0.2, 0.4, 0.6, 1.][..]))
            .chain(SPACING.iter().copied())
        {
            let mut source = Property::new(
                name,
                PropertyType::Float32,
                PropertySubType::Null,
                PropertyPermission::default(),
            );
            source.set_array_len(values.len());
            source.set_defaults_f32(values.to_vec()).unwrap();
            let source = Arc::new(source);
            let prop = node.get_property(name).unwrap();
            let atom = &mut PropertyAtomicGuard::none();
            for i in 0..values.len() {
                let local = format!("value_{i}");
                prop.add_depend(Role::App, &source, i, &local);
                prop.set_expr(atom, Role::App, i, expr::load_var(&local)).unwrap();
            }
            sources.push((name, source));
        }
        Self { edit, node, sources, ex, redraw_rx, method_rx }
    }

    fn source(&self, name: &str) -> PropertyPtr {
        self.sources.iter().find(|(key, _)| *key == name).unwrap().1.clone()
    }

    fn settle(&self) {
        for _ in 0..1024 {
            if !self.ex.try_tick() {
                return
            }
        }
        panic!("edit tasks did not become idle");
    }

    async fn draw(&self) -> DrawUpdate {
        let atom = &mut PropertyAtomicGuard::none();
        self.edit.draw(Rectangle::new(0., 0., 640., 480.), atom).await.unwrap()
    }

    fn assert_cursor(
        &self,
        rect: Rectangle,
        color: Color,
        blink_time: u32,
        created: bool,
    ) -> AnimId {
        let cursor = self.edit.cursor_render.lock();
        let cursor = cursor.as_ref().unwrap();
        assert_eq!(cursor.rect, rect);
        assert_eq!(cursor.color, color);
        assert_eq!(cursor.blink_time, blink_time);
        let mut frames = vec![];
        let mut meshes = 0;
        let mut animations = 0;
        while let Ok((_, method)) = self.method_rx.try_recv() {
            match method {
                GraphicsMethod::NewSeqAnim { id, frames_len, oneshot, .. } => {
                    assert_eq!(id, cursor.anim.id);
                    assert_eq!(frames_len, 2);
                    assert!(!oneshot);
                    animations += 1;
                }
                GraphicsMethod::UpdateSeqAnim { id, frame_idx, frame, .. } => {
                    assert_eq!(id, cursor.anim.id);
                    // Frame fields are private; Debug exposes both duration and draw contents.
                    let frame = format!("{frame:?}");
                    assert!(frame.starts_with(&format!("Frame {{ duration: {blink_time},")));
                    if frame_idx == 0 {
                        assert!(frame.contains("instrs: [Draw("));
                    } else {
                        assert!(frame.contains("instrs: []"));
                    }
                    frames.push(frame_idx);
                }
                GraphicsMethod::NewVertexBuffer((verts, _, tag))
                    if tag == gfxtag!("chatedit_cursor") =>
                {
                    assert_eq!(verts.len(), 4);
                    assert!(verts.iter().all(|v| v.color == color));
                    assert_eq!(verts[0].pos, [rect.x, rect.y]);
                    assert_eq!(verts[1].pos, [rect.rhs(), rect.y]);
                    assert_eq!(verts[2].pos, [rect.x, rect.y + rect.h]);
                    assert_eq!(verts[3].pos, [rect.rhs(), rect.y + rect.h]);
                    meshes += 1;
                }
                GraphicsMethod::PauseSeqAnim { .. } | GraphicsMethod::DeleteSeqAnim(_) => {
                    panic!("draw must not reset the interaction pause or replace the animation");
                }
                _ => {}
            }
        }
        assert_eq!(frames, vec![0, 1]);
        assert_eq!(meshes, 1);
        assert_eq!(animations, usize::from(created));
        cursor.anim.id
    }

    fn assert_no_animation_changes(&self) {
        while let Ok((_, method)) = self.method_rx.try_recv() {
            assert!(
                !matches!(
                    method,
                    GraphicsMethod::NewSeqAnim { .. } |
                        GraphicsMethod::UpdateSeqAnim { .. } |
                        GraphicsMethod::PauseSeqAnim { .. } |
                        GraphicsMethod::DeleteSeqAnim(_)
                ),
                "unexpected animation change: {method:?}"
            );
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.edit.stop();
        // Drop queued frames while the receiver can still accept their buffer deletions.
        while self.method_rx.try_recv().is_ok() {}
    }
}

#[test]
fn pointer_hit_testing_matches_rendered_text_at_window_scales() {
    smol::block_on(async {
        for scale in [0.8, 1., 1.5] {
            for (multiline, scrolled) in [(false, false), (true, false), (true, true)] {
                let f = Fixture::new(multiline).await;
                {
                    let atom = &mut PropertyAtomicGuard::none();
                    f.edit.window_scale.prop().set_f32(atom, Role::App, 0, scale).unwrap();
                    for (i, value) in [35., 27., 450., 80.].into_iter().enumerate() {
                        f.edit.rect.prop().set_f32(atom, Role::App, i, value).unwrap();
                    }
                    if multiline {
                        f.node
                            .get_property("height_range")
                            .unwrap()
                            .set_f32(atom, Role::App, 1, 80.)
                            .unwrap();
                    }
                    let line = "abcdefghijklmnopqrstuvwxyz";
                    let text = if multiline {
                        format!("{line}\n{line}\n{line}\n{line}")
                    } else {
                        line.to_owned()
                    };
                    f.node.set_property_str(atom, Role::App, "text", &text).unwrap();
                }
                f.edit.on_text_prop_changed();
                f.draw().await;
                if scrolled {
                    assert!(f.edit.behave.max_scroll() >= 24.);
                    f.edit.scroll.store(24., Ordering::Relaxed);
                }
                let line_start = if scrolled { 27 } else { 0 };
                let indices = [line_start + 1, line_start + 12, line_start + 24];
                let points = indices.map(|index| {
                    let editor = f.edit.editor.lock();
                    let layout = editor.layout();
                    let cursor = parley::Cursor::from_byte_index(
                        layout,
                        index,
                        parley::Affinity::Downstream,
                    );
                    let rect = Rectangle::from(cursor.geometry(layout, 0.)) / scale;
                    rect.center() +
                        f.edit.rect.get().pos() +
                        f.edit.behave.inner_pos() +
                        f.edit.behave.scroll()
                });
                for (&index, &point) in indices.iter().zip(&points) {
                    assert!(f.edit.handle_mouse_btn_down(MouseButton::Left, point).await);
                    let editor = f.edit.editor.lock();
                    let selection = editor.selection(1);
                    assert!(selection.is_collapsed());
                    assert_eq!(
                        selection.focus().index(),
                        index,
                        "click: scale={scale}, multiline={multiline}, scrolled={scrolled}"
                    );
                }
                for (anchor, focus) in [(0, 2), (2, 0)] {
                    assert!(f.edit.handle_mouse_btn_down(MouseButton::Left, points[anchor]).await);
                    f.edit.handle_select(points[focus], None);
                    let editor = f.edit.editor.lock();
                    let selection = editor.selection(1);
                    assert_eq!(selection.anchor().index(), indices[anchor]);
                    assert_eq!(
                        selection.focus().index(),
                        indices[focus],
                        "drag: scale={scale}, multiline={multiline}, scrolled={scrolled}"
                    );
                }
            }
        }
    });
}

#[test]
fn first_draw_and_dependency_updates_style_layout_in_one_pass() {
    smol::block_on(async {
        for multiline in [false, true] {
            let f = Fixture::new(multiline).await;
            let mut menu = action::Menu::new();
            menu.add("Copy", ACTION_COPY);
            menu.add("Paste", ACTION_PASTE);
            menu.anchor = Point::new(0., 100.);
            f.edit.action_mode.set(menu);
            let mut previous_height = None;
            for updated in [false, true] {
                if updated {
                    let atom = &mut PropertyAtomicGuard::none();
                    for name in COLORS {
                        for (i, value) in [0.8, 0.6, 0.4, 0.9].into_iter().enumerate() {
                            f.source(name).set_f32(atom, Role::App, i, value).unwrap();
                        }
                    }
                    for (name, values) in [
                        ("font_size", &[32.][..]),
                        ("lineheight", &[2.][..]),
                        ("padding", &[9., 23., 15., 31.][..]),
                        ("action_padding", &[10.][..]),
                        ("action_spacing", &[21.][..]),
                    ] {
                        for (i, value) in values.iter().enumerate() {
                            f.source(name).set_f32(atom, Role::App, i, *value).unwrap();
                        }
                    }
                }
                let draw = f.draw().await;
                for (name, source) in &f.sources {
                    let prop = f.node.get_property(name).unwrap();
                    for i in 0..source.get_len() {
                        assert_eq!(
                            prop.get_f32(i).unwrap(),
                            source.get_f32(i).unwrap(),
                            "{name}[{i}]"
                        );
                    }
                }
                let font = if updated { 32. } else { 24. };
                let lineheight = if updated { 2. } else { 1.5 };
                let color = if updated { [0.8, 0.6, 0.4, 0.9] } else { [0.2, 0.4, 0.6, 1.] };
                let padding = if updated { [9., 23., 15., 31.] } else { [3., 11., 5., 17.] };
                let expected = text::make_layout2(
                    &f.edit.text.get(),
                    color,
                    font,
                    lineheight,
                    1.,
                    multiline.then_some(180. - padding[1] - padding[3]),
                    &[],
                    &[],
                    parley::Alignment::Start,
                    parley::OverflowWrap::Anywhere,
                );
                let editor = f.edit.editor.lock();
                assert!((editor.height() - expected.height()).abs() < 0.01);
                assert_eq!(editor.layout().lines().count(), expected.lines().count());
                let mut runs = 0;
                for line in editor.layout().lines() {
                    for item in line.items() {
                        if let parley::PositionedLayoutItem::GlyphRun(run) = item {
                            assert_eq!(run.style().brush, color);
                            assert_eq!(run.run().font_size(), font);
                            runs += 1;
                        }
                    }
                }
                assert!(runs > 0);
                drop(editor);
                assert!(expected.height() > 0.);
                if let Some(height) = previous_height {
                    assert_ne!(expected.height(), height);
                }
                previous_height = Some(expected.height());
                let expected_height = if multiline {
                    (expected.height() + padding[0] + padding[2]).clamp(20., 400.)
                } else {
                    120.
                };
                assert!((f.edit.rect.get_height() - expected_height).abs() < 0.01);
                let expected_pos = Point::new(
                    padding[3],
                    if multiline { padding[0] } else { (120. - expected.height()) / 2. },
                );
                assert!(f.edit.behave.inner_pos().dist(expected_pos) < 0.01);
                let text_dc =
                    &draw.draw_calls.iter().find(|(key, _)| *key == f.edit.text_dc_key).unwrap().1;
                assert!(matches!(
                    text_dc.instrs[0],
                    DrawInstruction::Move(pos) if pos.dist(expected_pos) < 0.01
                ));

                let action_padding = if updated { 10. } else { 6. };
                let action_spacing = if updated { 21. } else { 13. };
                let copy = text::make_layout("Copy", color, font, 0., 1., None, &[]);
                let copy_width = copy.width() + 2. * action_padding;
                let menu_y = 100. - (font + 2. * action_padding) / 2.;
                assert!(f.edit.action_mode.hit(Point::new(copy_width / 2., menu_y)));
                assert!(!f
                    .edit
                    .action_mode
                    .hit(Point::new(copy_width + action_spacing / 2., menu_y)));
                assert!(f
                    .edit
                    .action_mode
                    .hit(Point::new(copy_width + action_spacing + 1., menu_y)));
            }
        }
    });
}

#[test]
fn first_visible_cursor_is_opaque_with_or_without_start() {
    smol::block_on(async {
        for multiline in [false, true] {
            for started in [false, true] {
                let f = Fixture::new(multiline).await;
                assert_eq!(f.edit.cursor_color.get(), [0.; 4]);
                f.edit.pause_blinking();
                if started {
                    // A hidden ancestor starts its children but does not draw them yet.
                    f.edit.clone().start(f.ex.clone()).await;
                    f.settle();
                }
                assert!(f.edit.cursor_render.lock().is_none());
                f.assert_no_animation_changes();
                let draw = f.draw().await;
                let id = f.assert_cursor(
                    Rectangle::new(0., 4., 2., 16.),
                    [0.2, 0.4, 0.6, 1.],
                    500,
                    true,
                );
                let dc = &draw
                    .draw_calls
                    .iter()
                    .find(|(key, _)| *key == f.edit.cursor_dc_key)
                    .unwrap()
                    .1;
                assert!(matches!(
                    dc.instrs.as_slice(),
                    [DrawInstruction::Move(_), DrawInstruction::Animation(anim)] if anim.id == id
                ));
                f.settle();
                assert!(
                    f.redraw_rx.is_empty(),
                    "draw-time cache writes must not request another pass"
                );
                if !started {
                    f.edit.clone().start(f.ex.clone()).await;
                    f.settle();
                    f.draw().await;
                    f.assert_no_animation_changes();
                    assert_eq!(f.edit.cursor_render.lock().as_ref().unwrap().anim.id, id);
                }
            }
        }
    });
}

#[test]
fn cursor_input_changes_update_frames_without_replacing_or_rearming_animation() {
    smol::block_on(async {
        for multiline in [false, true] {
            let f = Fixture::new(multiline).await;
            f.draw().await;
            let mut color = [0.2, 0.4, 0.6, 1.];
            let mut blink = 500;
            let id = f.assert_cursor(Rectangle::new(0., 4., 2., 16.), color, blink, true);
            assert!(f.edit.handle_key_down(KeyCode::Right, KeyMods::default(), false).await);
            let (_, method) = f.method_rx.try_recv().unwrap();
            assert!(matches!(
                method,
                GraphicsMethod::PauseSeqAnim {
                    id: paused, frame_idx: 0, duration_ms: 150, ..
                } if paused == id
            ));
            for (name, value, rect) in [
                ("baseline", 20., Rectangle::new(0., 10., 2., 16.)),
                ("cursor_width", 5., Rectangle::new(0., 10., 5., 16.)),
                ("cursor_ascent", 13., Rectangle::new(0., 7., 5., 19.)),
                ("cursor_descent", 9., Rectangle::new(0., 7., 5., 22.)),
                ("cursor_color", 0.8, Rectangle::new(0., 7., 5., 22.)),
                ("cursor_blink_time", 275., Rectangle::new(0., 7., 5., 22.)),
            ] {
                {
                    let atom = &mut PropertyAtomicGuard::none();
                    if name == "cursor_color" {
                        color[0] = value;
                        f.source(name).set_f32(atom, Role::App, 0, value).unwrap();
                    } else if name == "cursor_blink_time" {
                        blink = value as u32;
                        f.node.set_property_u32(atom, Role::App, name, blink).unwrap();
                    } else {
                        f.node.set_property_f32(atom, Role::App, name, value).unwrap();
                    }
                }
                f.draw().await;
                assert_eq!(f.assert_cursor(rect, color, blink, false), id, "{name}");
                f.draw().await;
                f.assert_no_animation_changes();
            }
        }
    });
}

#[test]
fn style_subscriptions_redraw_for_external_changes_but_not_internal_echoes() {
    smol::block_on(async {
        for multiline in [false, true] {
            let f = Fixture::new(multiline).await;
            f.edit.clone().start(f.ex.clone()).await;
            f.settle();
            assert!(f.redraw_rx.is_empty());
            f.draw().await;
            f.settle();
            assert!(f.redraw_rx.is_empty());
            for (name, source) in &f.sources {
                {
                    let atom = &mut PropertyAtomicGuard::none();
                    source.set_f32(atom, Role::App, 0, source.get_f32(0).unwrap() + 0.1).unwrap();
                }
                f.settle();
                assert!(f.redraw_rx.try_recv().is_ok(), "missing dependency subscription: {name}");
                f.draw().await;
                f.settle();
                assert!(f.redraw_rx.is_empty(), "cache feedback: {name}");
                {
                    let atom = &mut PropertyAtomicGuard::none();
                    let prop = f.node.get_property(name).unwrap();
                    prop.set_cache_f32(atom, Role::Internal, 0, 0.).unwrap();
                    source.set_cache_f32(atom, Role::Internal, 0, 0.).unwrap();
                }
                f.settle();
                assert!(f.redraw_rx.is_empty(), "internal dependency/cache echo: {name}");
                {
                    let atom = &mut PropertyAtomicGuard::none();
                    f.node
                        .set_property_f32(atom, Role::App, name, source.get_f32(0).unwrap())
                        .unwrap();
                }
                f.settle();
                assert!(f.redraw_rx.try_recv().is_ok(), "missing direct subscription: {name}");
            }
            for (name, index, value) in [
                ("baseline", 0, 20.),
                ("cursor_width", 0, 3.),
                ("cursor_ascent", 0, 12.),
                ("cursor_descent", 0, 7.),
                ("select_ascent", 0, 12.),
                ("select_descent", 0, 7.),
                ("handle_descent", 0, 9.),
                ("height_range", 0, 30.),
                ("height_range", 1, 300.),
            ] {
                let Some(prop) = f.node.get_property(name) else {
                    assert_eq!(name, "height_range");
                    continue
                };
                {
                    let atom = &mut PropertyAtomicGuard::none();
                    prop.set_f32(atom, Role::App, index, value).unwrap();
                }
                f.settle();
                assert!(f.redraw_rx.try_recv().is_ok(), "missing subscription: {name}[{index}]");
            }
            {
                let atom = &mut PropertyAtomicGuard::none();
                f.node.set_property_u32(atom, Role::App, "cursor_blink_time", 275).unwrap();
            }
            f.settle();
            assert!(f.redraw_rx.try_recv().is_ok(), "missing blink-time subscription");
            f.draw().await;
            f.settle();
            assert!(f.redraw_rx.is_empty());
        }
    });
}
