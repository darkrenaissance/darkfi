/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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

use async_lock::Mutex as AsyncMutex;
use async_trait::async_trait;
use atomic_float::AtomicF32;
use darkfi::system::msleep;
use miniquad::{window, KeyCode, KeyMods, MouseButton, TouchPhase};
use rand::{rngs::OsRng, Rng};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex as SyncMutex, OnceLock, Weak,
    },
    time::Instant,
};

use crate::{
    error::Result,
    gfx::{
        GfxDrawCall, GfxDrawInstruction, GfxDrawMesh, GfxTextureId, GraphicsEventPublisherPtr,
        Point, Rectangle, RenderApi, Vertex,
    },
    mesh::{MeshBuilder, MeshInfo, COLOR_BLUE, COLOR_WHITE},
    prop::{
        PropertyAtomicGuard, PropertyBool, PropertyColor, PropertyFloat32, PropertyPtr,
        PropertyRect, PropertyStr, PropertyUint32, Role,
    },
    pubsub::Subscription,
    scene::{Pimpl, SceneNodePtr, SceneNodeWeak},
    text::{self, Glyph, GlyphPositionIter, TextShaperPtr},
    util::{enumerate_ref, is_whitespace, unixtime, zip3},
    ExecutorPtr,
};

use super::{DrawUpdate, OnModify, UIObject};

pub mod editable;
use editable::{Editable, Selection, TextIdx, TextPos};

pub mod repeat;
use repeat::{PressedKey, PressedKeysSmoothRepeat};

macro_rules! d { ($($arg:tt)*) => { debug!(target: "ui::editbox", $($arg)*); } }
macro_rules! t { ($($arg:tt)*) => { trace!(target: "ui::editbox", $($arg)*); } }

// EOL whitespace is given a nudge since it has a width of 0 after text shaping
const CURSOR_EOL_WS_NUDGE: f32 = 0.8;
// EOL chars are more aesthetic when given a smallish nudge
const CURSOR_EOL_NUDGE: f32 = 0.2;

pub fn eol_nudge(font_size: f32, glyphs: &Vec<Glyph>) -> f32 {
    if is_whitespace(&glyphs.last().unwrap().substr) {
        (font_size * CURSOR_EOL_WS_NUDGE).round()
    } else {
        (font_size * CURSOR_EOL_NUDGE).round()
    }
}

#[derive(Clone)]
struct TouchStartInfo {
    pos: Point,
    instant: std::time::Instant,
}

impl TouchStartInfo {
    fn new(pos: Point) -> Self {
        Self { pos, instant: std::time::Instant::now() }
    }
}

#[derive(Clone)]
enum TouchStateAction {
    Inactive,
    Started { pos: Point, instant: std::time::Instant },
    StartSelect,
    Select,
    DragSelectHandle { side: isize },
    ScrollHoriz { start_pos: Point, scroll_start: f32 },
}

struct TouchInfo {
    state: TouchStateAction,
    start: Option<TouchStartInfo>,

    scroll: PropertyFloat32,
}

impl TouchInfo {
    fn new(scroll: PropertyFloat32) -> Self {
        Self { state: TouchStateAction::Inactive, start: None, scroll }
    }

    fn start(&mut self, pos: Point) {
        d!("TouchStateAction::Started");
        self.state = TouchStateAction::Started { pos, instant: std::time::Instant::now() };
    }

    fn stop(&mut self) -> TouchStateAction {
        std::mem::replace(&mut self.state, TouchStateAction::Inactive)
    }

    fn update(&mut self, pos: &Point) {
        match &self.state {
            TouchStateAction::Started { pos: start_pos, instant } => {
                let travel_dist = pos.dist_sq(&start_pos);
                let x_dist = pos.x - start_pos.x;
                let elapsed = instant.elapsed().as_millis();

                if travel_dist < 5. {
                    if elapsed > 1000 {
                        d!("TouchStateAction::StartSelect");
                        self.state = TouchStateAction::StartSelect;
                    }
                } else if x_dist.abs() > 5. {
                    d!("TouchStateAction::ScrollHoriz");
                    let scroll_start = self.scroll.get();
                    self.state =
                        TouchStateAction::ScrollHoriz { start_pos: *start_pos, scroll_start };
                }
            }
            _ => {}
        }
    }
}

pub type EditBoxPtr = Arc<EditBox>;

pub struct EditBox {
    node: SceneNodeWeak,
    tasks: OnceLock<Vec<smol::Task<()>>>,
    render_api: RenderApi,
    text_shaper: TextShaperPtr,
    key_repeat: SyncMutex<PressedKeysSmoothRepeat>,

    glyphs: SyncMutex<Vec<Glyph>>,
    /// DC key for the text
    text_dc_key: u64,
    cursor_mesh: SyncMutex<Option<GfxDrawMesh>>,
    /// DC key for the cursor. Allows updating cursor independently.
    cursor_dc_key: u64,

    is_active: PropertyBool,
    is_focused: PropertyBool,
    rect: PropertyRect,
    baseline: PropertyFloat32,
    scroll: PropertyFloat32,
    scroll_speed: PropertyFloat32,
    cursor_pos: PropertyUint32,
    font_size: PropertyFloat32,
    text: PropertyStr,
    text_color: PropertyColor,
    text_hi_color: PropertyColor,
    cursor_color: PropertyColor,
    cursor_width: PropertyFloat32,
    cursor_ascent: PropertyFloat32,
    cursor_descent: PropertyFloat32,
    cursor_blink_time: PropertyUint32,
    cursor_idle_time: PropertyUint32,
    hi_bg_color: PropertyColor,
    select_ascent: PropertyFloat32,
    select_descent: PropertyFloat32,
    selected: PropertyPtr,
    z_index: PropertyUint32,
    priority: PropertyUint32,
    debug: PropertyBool,

    editable: SyncMutex<Editable>,
    select: SyncMutex<Vec<Selection>>,

    mouse_btn_held: AtomicBool,
    cursor_is_visible: AtomicBool,
    blink_is_paused: AtomicBool,
    /// Used to explicitly hide the cursor. Must be manually re-enabled.
    hide_cursor: AtomicBool,

    touch_info: SyncMutex<TouchInfo>,
    is_phone_select: AtomicBool,

    old_window_scale: AtomicF32,
    window_scale: PropertyFloat32,
    parent_rect: SyncMutex<Option<Rectangle>>,
    is_mouse_hover: AtomicBool,
}

impl EditBox {
    pub async fn new(
        node: SceneNodeWeak,
        window_scale: PropertyFloat32,
        render_api: RenderApi,
        text_shaper: TextShaperPtr,
        ex: ExecutorPtr,
    ) -> Pimpl {
        t!("EditBox::new()");

        let node_ref = &node.upgrade().unwrap();
        let is_active = PropertyBool::wrap(node_ref, Role::Internal, "is_active", 0).unwrap();
        let is_focused = PropertyBool::wrap(node_ref, Role::Internal, "is_focused", 0).unwrap();
        let rect = PropertyRect::wrap(node_ref, Role::Internal, "rect").unwrap();
        let baseline = PropertyFloat32::wrap(node_ref, Role::Internal, "baseline", 0).unwrap();
        let scroll = PropertyFloat32::wrap(node_ref, Role::Internal, "scroll", 0).unwrap();
        let scroll_speed =
            PropertyFloat32::wrap(node_ref, Role::Internal, "scroll_speed", 0).unwrap();
        let cursor_pos = PropertyUint32::wrap(node_ref, Role::Internal, "cursor_pos", 0).unwrap();
        let font_size = PropertyFloat32::wrap(node_ref, Role::Internal, "font_size", 0).unwrap();
        let text = PropertyStr::wrap(node_ref, Role::Internal, "text", 0).unwrap();
        let text_color = PropertyColor::wrap(node_ref, Role::Internal, "text_color").unwrap();
        let text_hi_color = PropertyColor::wrap(node_ref, Role::Internal, "text_hi_color").unwrap();
        let cursor_color = PropertyColor::wrap(node_ref, Role::Internal, "cursor_color").unwrap();
        let cursor_width =
            PropertyFloat32::wrap(node_ref, Role::Internal, "cursor_width", 0).unwrap();
        let cursor_ascent =
            PropertyFloat32::wrap(node_ref, Role::Internal, "cursor_ascent", 0).unwrap();
        let cursor_descent =
            PropertyFloat32::wrap(node_ref, Role::Internal, "cursor_descent", 0).unwrap();
        let hi_bg_color = PropertyColor::wrap(node_ref, Role::Internal, "hi_bg_color").unwrap();
        let select_ascent =
            PropertyFloat32::wrap(node_ref, Role::Internal, "select_ascent", 0).unwrap();
        let select_descent =
            PropertyFloat32::wrap(node_ref, Role::Internal, "select_descent", 0).unwrap();
        let selected = node_ref.get_property("selected").unwrap();
        let cursor_blink_time =
            PropertyUint32::wrap(node_ref, Role::Internal, "cursor_blink_time", 0).unwrap();
        let cursor_idle_time =
            PropertyUint32::wrap(node_ref, Role::Internal, "cursor_idle_time", 0).unwrap();
        let z_index = PropertyUint32::wrap(node_ref, Role::Internal, "z_index", 0).unwrap();
        let priority = PropertyUint32::wrap(node_ref, Role::Internal, "priority", 0).unwrap();
        let debug = PropertyBool::wrap(node_ref, Role::Internal, "debug", 0).unwrap();

        let node_name = node_ref.name.clone();
        let node_id = node_ref.id;

        // Must do this whenever the text changes
        let glyphs = text_shaper.shape(text.get(), font_size.get(), window_scale.get());

        let self_ = Arc::new(Self {
            node,
            tasks: OnceLock::new(),
            render_api,
            text_shaper: text_shaper.clone(),
            key_repeat: SyncMutex::new(PressedKeysSmoothRepeat::new(400, 50)),

            glyphs: SyncMutex::new(glyphs),
            text_dc_key: OsRng.gen(),
            cursor_mesh: SyncMutex::new(None),
            cursor_dc_key: OsRng.gen(),

            is_active,
            is_focused,
            rect,
            baseline: baseline.clone(),
            scroll: scroll.clone(),
            scroll_speed,
            cursor_pos,
            font_size: font_size.clone(),
            text,
            text_color,
            text_hi_color,
            cursor_color,
            cursor_width,
            cursor_ascent,
            cursor_descent,
            cursor_blink_time,
            cursor_idle_time,
            hi_bg_color,
            select_ascent,
            select_descent,
            selected,
            z_index,
            priority,
            debug,

            editable: SyncMutex::new(Editable::new(
                text_shaper,
                font_size,
                window_scale.clone(),
                baseline,
            )),
            select: SyncMutex::new(vec![]),

            mouse_btn_held: AtomicBool::new(false),
            cursor_is_visible: AtomicBool::new(true),
            blink_is_paused: AtomicBool::new(false),
            hide_cursor: AtomicBool::new(false),

            touch_info: SyncMutex::new(TouchInfo::new(scroll)),
            is_phone_select: AtomicBool::new(false),

            old_window_scale: AtomicF32::new(window_scale.get()),
            window_scale,
            parent_rect: SyncMutex::new(None),
            is_mouse_hover: AtomicBool::new(false),
        });

        Pimpl::EditBox(self_)
    }

    fn node(&self) -> SceneNodePtr {
        self.node.upgrade().unwrap()
    }

    /// Called whenever the text or any text property changes.
    fn regen_text_mesh(&self, mut clip: Rectangle) -> GfxDrawMesh {
        clip.x = 0.;
        clip.y = 0.;

        let is_focused = self.is_focused.get();
        let text = self.text.get();
        let font_size = self.font_size.get();
        let window_scale = self.window_scale.get();
        let text_color = self.text_color.get();
        let text_hi_color = self.text_hi_color.get();
        let baseline = self.baseline.get();
        let scroll = self.scroll.get();
        let cursor_pos = self.cursor_pos.get() as usize;
        let cursor_color = self.cursor_color.get();
        let debug = self.debug.get();
        //debug!(target: "ui::editbox", "Rendering text '{text}' clip={clip:?}");
        //debug!(target: "ui::editbox", "    cursor_pos={cursor_pos}, is_focused={is_focused}");

        let rendered = self.editable.lock().unwrap().render();
        let atlas = text::make_texture_atlas(&self.render_api, &rendered.glyphs);

        let mut mesh = MeshBuilder::with_clip(clip.clone());

        let selections = self.select.lock().unwrap().clone();
        // Just an assert
        if self.is_phone_select.load(Ordering::Relaxed) {
            assert!(selections.len() <= 1);
        }
        for select in &selections {
            self.draw_selected(&mut mesh, select, &rendered.glyphs, clip.h);
        }
        let select_marks = self.mark_selected_glyphs(&rendered.glyphs, selections);

        if rendered.has_underline() {
            self.draw_underline(
                &mut mesh,
                &rendered.glyphs,
                clip.h,
                rendered.under_start,
                rendered.under_end,
            );
        }

        let glyph_pos_iter =
            GlyphPositionIter::new(font_size, window_scale, &rendered.glyphs, baseline);

        for (glyph_idx, mut glyph_rect, glyph, is_selected) in
            zip3(glyph_pos_iter, rendered.glyphs.iter(), select_marks.iter())
        {
            let uv_rect = atlas.fetch_uv(glyph.glyph_id).expect("missing glyph UV rect");

            glyph_rect.x -= scroll;

            //mesh.draw_outline(&glyph_rect, COLOR_BLUE, 2.);
            let mut color = text_color.clone();
            if *is_selected {
                color = text_hi_color.clone();
            }
            if glyph.sprite.has_color {
                color = COLOR_WHITE;
            }
            mesh.draw_box(&glyph_rect, color, uv_rect);
        }

        if debug {
            mesh.draw_outline(&clip, COLOR_BLUE, 1.);
        }

        mesh.alloc(&self.render_api).draw_with_texture(atlas.texture)
    }

    fn regen_cursor_mesh(&self) -> GfxDrawMesh {
        let cursor_width = self.cursor_width.get();
        let cursor_ascent = self.cursor_ascent.get();
        let cursor_descent = self.cursor_descent.get();
        let baseline = self.baseline.get();

        let cursor_rect = Rectangle {
            x: 0.,
            y: baseline - cursor_ascent,
            w: cursor_width,
            h: cursor_ascent + cursor_descent,
        };
        let cursor_color = self.cursor_color.get();

        let mut mesh = MeshBuilder::new();
        mesh.draw_filled_box(&cursor_rect, cursor_color);
        mesh.alloc(&self.render_api).draw_untextured()
    }

    fn cursor_px_offset(&self) -> f32 {
        assert!(self.is_focused.get());

        let (cursor_pos, glyphs) = {
            let editable = self.editable.lock().unwrap();
            let rendered = editable.render();
            let cursor_pos = editable.get_cursor_pos(&rendered);
            (cursor_pos, rendered.glyphs)
        };

        let font_size = self.font_size.get();
        let window_scale = self.window_scale.get();
        let baseline = self.baseline.get();
        let scroll = self.scroll.get();
        // Add composer glyphs too
        let glyph_pos_iter = GlyphPositionIter::new(font_size, window_scale, &glyphs, baseline);
        // Used for drawing the cursor when it's at the end of the line.
        let mut rhs = 0.;

        if cursor_pos == 0 {
            return 0.;
        }

        for (glyph_idx, (mut glyph_rect, glyph)) in glyph_pos_iter.zip(glyphs.iter()).enumerate() {
            glyph_rect.x -= scroll;

            if cursor_pos == glyph_idx {
                return glyph_rect.x;
            }

            rhs = glyph_rect.rhs();
        }

        assert!(cursor_pos == glyphs.len());

        rhs += eol_nudge(font_size, &glyphs);
        rhs
    }

    fn mark_selected_glyphs(&self, glyphs: &Vec<Glyph>, selections: Vec<Selection>) -> Vec<bool> {
        let mut marks: Vec<_> = glyphs.iter().map(|_| false).collect();
        for select in selections {
            let start = std::cmp::min(select.start, select.end);
            let end = std::cmp::max(select.start, select.end);

            for i in start..end {
                marks[i] = true;
            }
        }
        marks
    }

    fn draw_selected(
        &self,
        mesh: &mut MeshBuilder,
        select: &Selection,
        glyphs: &Vec<Glyph>,
        clip_h: f32,
    ) {
        if select.start == select.end {
            return
        }
        let start = std::cmp::min(select.start, select.end);
        let end = std::cmp::max(select.start, select.end);

        let font_size = self.font_size.get();
        let window_scale = self.window_scale.get();
        let baseline = self.baseline.get();
        let scroll = self.scroll.get();
        let hi_bg_color = self.hi_bg_color.get();
        let glyph_pos_iter = GlyphPositionIter::new(font_size, window_scale, &glyphs, baseline);

        let is_phone_select = self.is_phone_select.load(Ordering::Relaxed);

        let mut start_x = 0.;
        let mut end_x = 0.;
        // When cursor lands at the end of the line
        let mut rhs = 0.;

        for (glyph_idx, mut glyph_rect) in glyph_pos_iter.enumerate() {
            glyph_rect.x -= scroll;

            if glyph_idx == start {
                start_x = glyph_rect.x;
            }
            if glyph_idx == end {
                end_x = glyph_rect.x;
            }

            rhs = glyph_rect.rhs();
        }

        if start == 0 {
            start_x = scroll;
        }

        if end == glyphs.len() {
            rhs += eol_nudge(font_size, &glyphs);
            end_x = rhs;
        }

        let select_ascent = self.select_ascent.get();
        let select_descent = self.select_descent.get();

        let select_rect = Rectangle {
            x: start_x,
            y: baseline - select_ascent,
            w: end_x - start_x,
            h: select_ascent + select_descent,
        };
        mesh.draw_box(&select_rect, hi_bg_color, &Rectangle::zero());

        if is_phone_select {
            self.draw_phone_select_handle(mesh, start_x, -1.);
            self.draw_phone_select_handle(mesh, end_x, 1.);
        }
    }

    fn draw_phone_select_handle(&self, mesh: &mut MeshBuilder, x: f32, side: f32) {
        t!("draw_phone_select_handle(..., {x}, {side})");

        let baseline = self.baseline.get();
        let select_ascent = self.select_ascent.get();
        let select_descent = self.select_descent.get();
        let color = self.text_hi_color.get();
        // Transparent for fade
        let mut color_trans = color.clone();
        color_trans[3] = 0.;

        // Vertical line downwards. We use this instead of draw_box() so we have a fade.
        let verts = vec![
            Vertex {
                pos: [x - side * 1., baseline - select_ascent],
                color: color_trans,
                uv: [0., 0.],
            },
            Vertex {
                pos: [x + side * 4., baseline - select_ascent],
                color: color_trans,
                uv: [0., 0.],
            },
            Vertex { pos: [x - side * 1., baseline + select_descent + 5.], color, uv: [0., 0.] },
            Vertex { pos: [x + side * 4., baseline + select_descent + 5.], color, uv: [0., 0.] },
        ];
        let indices = vec![0, 2, 1, 1, 2, 3];
        mesh.append(verts, indices);

        let y = baseline + select_descent;

        // The arrow itself.
        // Go anti-clockwise
        let verts = vec![
            Vertex { pos: [x, y], color, uv: [0., 0.] },
            Vertex { pos: [x, y + 50.], color, uv: [0., 0.] },
            Vertex { pos: [x + side * 30., y + 50.], color, uv: [0., 0.] },
            Vertex { pos: [x + side * 50., y + 25.], color, uv: [0., 0.] },
        ];
        let indices = vec![0, 1, 2, 0, 2, 3];
        mesh.append(verts, indices);
    }

    fn draw_underline(
        &self,
        mesh: &mut MeshBuilder,
        glyphs: &Vec<Glyph>,
        clip_h: f32,
        under_start: usize,
        under_end: usize,
    ) {
        assert!(under_start < under_end);

        let font_size = self.font_size.get();
        let window_scale = self.window_scale.get();
        let baseline = self.baseline.get();
        let scroll = self.scroll.get();
        let text_color = self.text_color.get();
        let glyph_pos_iter = GlyphPositionIter::new(font_size, window_scale, &glyphs, baseline);

        let mut start_x = 0.;
        let mut end_x = 0.;
        // When cursor lands at the end of the line
        let mut rhs = 0.;

        for (glyph_idx, mut glyph_rect) in glyph_pos_iter.enumerate() {
            glyph_rect.x -= scroll;

            if glyph_idx == under_start {
                start_x = glyph_rect.x;
            }
            if glyph_idx == under_end {
                end_x = glyph_rect.x;
            }

            rhs = glyph_rect.rhs();
        }

        if under_start == 0 {
            start_x = scroll;
        }

        if under_end == glyphs.len() {
            end_x = rhs;
        }

        // We don't need to do manual clipping since MeshBuilder should do that
        let underline_rect = Rectangle { x: start_x, y: baseline + 6., w: end_x - start_x, h: 4. };
        mesh.draw_box(&underline_rect, text_color, &Rectangle::zero());
    }

    async fn change_focus(self: Arc<Self>) {
        if !self.is_active.get() {
            return
        }
        d!("Focus changed");

        // Cursor visibility will change so just redraw everything lol
        self.redraw().await;
    }

    async fn insert_char(&self, key: char) {
        {
            let mut editable = self.editable.lock().unwrap();
            let mut tmp = [0; 4];
            let key_str = key.encode_utf8(&mut tmp);
            editable.compose(key_str, true);
        }

        self.pause_blinking();
        self.redraw().await;
    }

    async fn handle_shortcut(&self, key: char, mods: &KeyMods, atom: &mut PropertyAtomicGuard) {
        t!("handle_shortcut({:?}, {:?})", key, mods);

        match key {
            'c' => {
                if mods.ctrl {
                    self.copy_highlighted().unwrap();
                }
            }
            'v' => {
                if mods.ctrl {
                    if let Some(text) = window::clipboard_get() {
                        self.paste_text(text, atom).await;
                    }
                }
            }
            _ => {}
        }
    }

    async fn handle_key(&self, key: &KeyCode, mods: &KeyMods, atom: &mut PropertyAtomicGuard) {
        t!("handle_key({:?}, {:?})", key, mods);
        match key {
            KeyCode::Left => {
                self.adjust_cursor(mods.shift, |editable| editable.move_cursor(-1));
                self.pause_blinking();
                //self.apply_cursor_scrolling();
                self.redraw().await;
            }
            KeyCode::Right => {
                self.adjust_cursor(mods.shift, |editable| editable.move_cursor(1));
                self.pause_blinking();
                //self.apply_cursor_scrolling();
                self.redraw().await;
            }
            KeyCode::Kp0 => self.insert_char('0').await,
            KeyCode::Kp1 => self.insert_char('1').await,
            KeyCode::Kp2 => self.insert_char('2').await,
            KeyCode::Kp3 => self.insert_char('3').await,
            KeyCode::Kp4 => self.insert_char('4').await,
            KeyCode::Kp5 => self.insert_char('5').await,
            KeyCode::Kp6 => self.insert_char('6').await,
            KeyCode::Kp7 => self.insert_char('7').await,
            KeyCode::Kp8 => self.insert_char('8').await,
            KeyCode::Kp9 => self.insert_char('9').await,
            KeyCode::KpDecimal => self.insert_char('.').await,
            KeyCode::Enter | KeyCode::KpEnter => {
                let node = self.node.upgrade().unwrap();
                node.trigger("enter_pressed", vec![]).await.unwrap();
            }
            KeyCode::Delete => {
                self.delete(0, 1);
                self.clamp_scroll(atom);
                self.pause_blinking();
                self.redraw().await;
            }
            KeyCode::Backspace => {
                self.delete(1, 0);
                self.clamp_scroll(atom);
                self.pause_blinking();
                self.redraw().await;
            }
            KeyCode::Home => {
                self.adjust_cursor(mods.shift, |editable| editable.move_start());
                self.pause_blinking();
                //self.apply_cursor_scrolling();
                self.redraw().await;
            }
            KeyCode::End => {
                self.adjust_cursor(mods.shift, |editable| editable.move_end());
                self.pause_blinking();
                //self.apply_cursor_scrolling();
                self.redraw().await;
            }
            _ => {}
        }
    }

    fn delete(&self, before: usize, after: usize) {
        let mut editable = self.editable.lock().unwrap();
        let selection = std::mem::take(&mut *self.select.lock().unwrap());

        if selection.is_empty() {
            editable.delete(before, after);
            return
        }

        let sel = selection.first().unwrap();
        let cursor_pos = std::cmp::min(sel.start, sel.end);

        let render = editable.render();
        // Same code in chatedit's TextWrap::delete_selected()
        let mut before_text = String::new();
        let mut after_text = String::new();
        'next: for (pos, glyph) in render.glyphs.iter().enumerate() {
            for select in &selection {
                let start = std::cmp::min(select.start, select.end);
                let end = std::cmp::max(select.start, select.end);

                if start <= pos && pos < end {
                    continue 'next
                }
            }
            if pos <= cursor_pos {
                before_text += &glyph.substr;
            } else {
                after_text += &glyph.substr;
            }
        }

        editable.set_text(before_text, after_text);

        self.is_phone_select.store(false, Ordering::Relaxed);
        // Reshow cursor (if hidden)
        self.hide_cursor.store(false, Ordering::Relaxed);
    }

    fn adjust_cursor(&self, has_shift: bool, move_cursor: impl Fn(&mut Editable)) {
        let mut editable = self.editable.lock().unwrap();
        let rendered = editable.render();
        let prev_cursor_pos = editable.get_cursor_pos(&rendered);
        move_cursor(&mut editable);
        let cursor_pos = editable.get_cursor_pos(&rendered);
        drop(editable);

        let mut select = self.select.lock().unwrap();

        // Start selection if shift is held
        if has_shift {
            // Create a new selection
            if select.is_empty() {
                select.push(Selection::new(prev_cursor_pos, cursor_pos));
            }

            // Update the selection
            select.last_mut().unwrap().end = cursor_pos;
        } else {
            select.clear();
        }
    }

    /// This will select the entire word rather than move the cursor to that location
    fn start_touch_select(&self, x: f32) {
        let rect = self.rect.get();

        let font_size = self.font_size.get();
        let window_scale = self.window_scale.get();
        let baseline = self.baseline.get();

        {
            let mut editable = self.editable.lock().unwrap();
            editable.end_compose();

            let rendered = editable.render();
            let x = x - rect.x + self.scroll.get();

            let cpos = rendered.x_to_pos(x, font_size, window_scale, baseline);

            // Find word start
            let mut cpos_start = cpos;
            while cpos_start > 0 {
                // Is the glyph before this pos just whitespace?
                let glyph_str = &rendered.glyphs[cpos_start - 1].substr;
                if is_whitespace(glyph_str) {
                    break
                }
                cpos_start -= 1;
            }
            // Find word end
            let mut cpos_end = cpos;
            while cpos_end < rendered.glyphs.len() {
                cpos_end += 1;
                let glyph_str = &rendered.glyphs[cpos_end].substr;
                if is_whitespace(glyph_str) {
                    break
                }
            }

            let cidx_start = rendered.pos_to_idx(cpos_start);
            let cidx_end = rendered.pos_to_idx(cpos_end);

            // begin selection
            let mut select = self.select.lock().unwrap();
            select.clear();
            select.push(Selection::new(cpos_start, cpos_end));
        }

        self.is_phone_select.store(true, Ordering::Relaxed);
        // redraw() will now hide the cursor
        self.hide_cursor.store(true, Ordering::Relaxed);
    }

    fn glyphs_to_string(glyphs: &Vec<Glyph>) -> String {
        let mut text = String::new();
        for (i, glyph) in glyphs.iter().enumerate() {
            text.push_str(&glyph.substr);
        }
        text
    }

    fn delete_highlighted(&self, atom: &mut PropertyAtomicGuard) {
        assert!(!self.selected.is_null(0).unwrap());
        assert!(!self.selected.is_null(1).unwrap());

        let start = self.selected.get_u32(0).unwrap() as usize;
        let end = self.selected.get_u32(1).unwrap() as usize;

        let sel_start = std::cmp::min(start, end);
        let sel_end = std::cmp::max(start, end);

        let mut glyphs = self.glyphs.lock().unwrap().clone();
        glyphs.drain(sel_start..sel_end);

        let text = Self::glyphs_to_string(&glyphs);
        t!("delete_highlighted() text='{text}', cursor_pos={sel_start}");
        self.text.set(atom, text);

        self.selected.clone().set_null(atom, Role::Internal, 0).unwrap();
        self.selected.clone().set_null(atom, Role::Internal, 1).unwrap();
        self.cursor_pos.set(atom, sel_start as u32);
    }

    fn copy_highlighted(&self) -> Result<()> {
        let start = self.selected.get_u32(0)? as usize;
        let end = self.selected.get_u32(1)? as usize;

        let sel_start = std::cmp::min(start, end);
        let sel_end = std::cmp::max(start, end);

        let mut text = String::new();

        let glyphs = self.glyphs.lock().unwrap().clone();
        for (glyph_idx, glyph) in glyphs.iter().enumerate() {
            if sel_start <= glyph_idx && glyph_idx < sel_end {
                text.push_str(&glyph.substr);
            }
        }

        info!(target: "ui::editbox", "Copied '{}'", text);
        window::clipboard_set(&text);
        Ok(())
    }

    async fn paste_text(&self, key: String, atom: &mut PropertyAtomicGuard) {
        let mut text = String::new();

        let cursor_pos = self.cursor_pos.get();

        if cursor_pos == 0 {
            text = key.clone();
        }

        let glyphs = self.glyphs.lock().unwrap().clone();
        for (glyph_idx, glyph) in glyphs.iter().enumerate() {
            text.push_str(&glyph.substr);
            if cursor_pos == glyph_idx as u32 + 1 {
                text.push_str(&key);
            }
        }

        self.text.set(atom, text);
        // Not always true lol
        self.cursor_pos.set(atom, cursor_pos + 1);

        self.apply_cursor_scrolling(atom);
        self.redraw().await;
    }

    async fn handle_touch_start(&self, pos: Point) -> bool {
        t!("handle_touch_start({pos:?})");
        let mut touch_info = self.touch_info.lock().unwrap();

        if self.try_handle_drag(&mut touch_info, pos) {
            return true
        }

        let rect = self.rect.get();
        if !rect.contains(pos) {}

        touch_info.start(pos);
        true
    }
    fn try_handle_drag(&self, touch_info: &mut TouchInfo, pos: Point) -> bool {
        let selections = self.select.lock().unwrap().clone();

        if self.is_phone_select.load(Ordering::Relaxed) && selections.len() == 1 {
            let select = selections.first().unwrap();

            let rendered = {
                let editable = self.editable.lock().unwrap();
                editable.render()
            };

            let font_size = self.font_size.get();
            let window_scale = self.window_scale.get();
            let baseline = self.baseline.get();

            // Get left handle centerpoint
            let x1 = rendered.pos_to_xw(select.start, font_size, window_scale, baseline).0;
            // Get right handle centerpoint
            let x2 = {
                let (x, w) = rendered.pos_to_xw(select.end, font_size, window_scale, baseline);
                x + w
            };

            // Are we within range of either one?
            let select_descent = self.select_descent.get();
            let scroll = self.scroll.get();
            let y = baseline + select_descent + 25.;

            let p1 = Point::new(x1 - scroll, y);
            let p2 = Point::new(x2 - scroll, y);
            t!("handle center points = ({p1:?}, {p2:?})");

            const TOUCH_RADIUS_SQ: f32 = 10_000.;
            // Make pos relative to the rect
            let pos_rel = pos - self.rect.get().pos();

            if p1.dist_sq(&pos_rel) <= TOUCH_RADIUS_SQ {
                d!("TouchStateAction::DragSelectHandle [side=-1]");
                // Set touch_state status to enable begin dragging them
                touch_info.state = TouchStateAction::DragSelectHandle { side: -1 };
                return true;
            }
            if p2.dist_sq(&pos_rel) <= TOUCH_RADIUS_SQ {
                d!("TouchStateAction::DragSelectHandle [side=1]");
                // Set touch_state status to enable begin dragging them
                touch_info.state = TouchStateAction::DragSelectHandle { side: 1 };
                return true;
            }
        }

        false
    }

    async fn handle_touch_move(&self, pos: Point) -> bool {
        t!("handle_touch_move({pos:?})");
        let atom = &mut PropertyAtomicGuard::new();
        let touch_state = {
            let mut touch_info = self.touch_info.lock().unwrap();
            touch_info.update(&pos);
            touch_info.state.clone()
        };
        match &touch_state {
            TouchStateAction::Inactive => return false,
            TouchStateAction::StartSelect => {
                let x = pos.x;
                self.start_touch_select(x);
                self.redraw().await;
                d!("TouchStateAction::Select");
                self.touch_info.lock().unwrap().state = TouchStateAction::Select;
            }
            TouchStateAction::DragSelectHandle { side } => {
                {
                    assert!(*side == -1 || *side == 1);
                    assert!(self.is_phone_select.load(Ordering::Relaxed));
                    let mut selections = self.select.lock().unwrap();
                    assert_eq!(selections.len(), 1);
                    let select = selections.first_mut().unwrap();

                    let rendered = {
                        let editable = self.editable.lock().unwrap();
                        editable.render()
                    };

                    let font_size = self.font_size.get();
                    let window_scale = self.window_scale.get();
                    let baseline = self.baseline.get();

                    let pos_x = pos.x + self.scroll.get();

                    let mut pos = rendered.x_to_pos(pos_x, font_size, window_scale, baseline);
                    if *side == -1 {
                        let select_other_pos = &mut select.end;
                        if pos >= *select_other_pos {
                            pos = *select_other_pos - 1;
                        }
                        select.start = pos;
                    } else {
                        let select_other_pos = &mut select.start;
                        if pos <= *select_other_pos {
                            pos = *select_other_pos + 1;
                        }
                        select.end = pos;
                    }
                }
                self.redraw().await;
            }
            TouchStateAction::ScrollHoriz { start_pos, scroll_start } => {
                let x_dist = start_pos.x - pos.x;
                let mut scroll = scroll_start + x_dist;
                scroll = scroll.clamp(0., self.max_cursor_scroll());
                self.scroll.set(atom, scroll);
                self.redraw().await;
            }
            _ => {}
        }
        true
    }
    async fn handle_touch_end(&self, pos: Point) -> bool {
        t!("handle_touch_end({pos:?})");
        let state = self.touch_info.lock().unwrap().stop();
        match state {
            TouchStateAction::Inactive => return false,
            TouchStateAction::Started { pos: _, instant: _ } => {
                let rect = self.rect.get();
                let font_size = self.font_size.get();
                let window_scale = self.window_scale.get();
                let baseline = self.baseline.get();

                {
                    let mut editable = self.editable.lock().unwrap();
                    let rendered = editable.render();
                    // Adjust with scroll here too
                    let x = pos.x - rect.x;

                    let cpos = rendered.x_to_pos(x, font_size, window_scale, baseline);
                    let cidx = rendered.pos_to_idx(cpos);
                    editable.set_cursor_idx(cidx);

                    // begin selection
                    let mut select = self.select.lock().unwrap();
                    select.clear();
                }

                self.is_phone_select.store(false, Ordering::Relaxed);
                // Reshow cursor (if hidden)
                self.hide_cursor.store(false, Ordering::Relaxed);

                self.redraw().await;
            }
            _ => {}
        }
        window::show_keyboard(true);
        true
    }

    /// Whenever the cursor property is modified this MUST be called
    /// to recalculate the scroll x property.
    fn apply_cursor_scrolling(&self, atom: &mut PropertyAtomicGuard) {
        let rect = self.rect.get();

        let cursor_pos = self.cursor_pos.get() as usize;
        let cursor_width = self.cursor_width.get();
        let mut scroll = self.scroll.get();

        let cursor_x = {
            let font_size = self.font_size.get();
            let window_scale = self.window_scale.get();
            let baseline = self.baseline.get();
            let glyphs = self.glyphs.lock().unwrap().clone();

            let mut glyph_pos_iter =
                GlyphPositionIter::new(font_size, window_scale, &glyphs, baseline);

            if cursor_pos == 0 {
                0.
            } else if cursor_pos == glyphs.len() {
                let glyph_pos = glyph_pos_iter.last().unwrap();

                let rhs = glyph_pos.rhs() + eol_nudge(font_size, &glyphs);
                rhs
            } else {
                assert!(cursor_pos < glyphs.len());
                let glyph_pos = glyph_pos_iter.nth(cursor_pos).expect("glyph pos mismatch glyphs");
                glyph_pos.x
            }
        };

        // The LHS and RHS of the cursor box
        let cursor_lhs = cursor_x - scroll;
        let cursor_rhs = cursor_lhs + cursor_width;

        // RHS is outside
        if cursor_rhs > rect.w {
            // We want a scroll so RHS = w
            // cursor_x - scroll + CURSOR_WIDTH = rect.w
            scroll = cursor_x + cursor_width - rect.w;
        // LHS is negative
        } else if cursor_lhs < 0. {
            // We want scroll so LHS = 0
            // cursor_x - scroll = 0
            scroll = cursor_x;
        }

        self.scroll.set(atom, scroll);
    }

    fn max_cursor_scroll(&self) -> f32 {
        let font_size = self.font_size.get();
        let window_scale = self.window_scale.get();
        let baseline = self.baseline.get();
        let rhs = {
            let mut editable = self.editable.lock().unwrap();
            let rendered = editable.render();

            if rendered.glyphs.is_empty() {
                return 0.
            }
            let glyph_pos_iter =
                GlyphPositionIter::new(font_size, window_scale, &rendered.glyphs, baseline);
            let last_rect = glyph_pos_iter.last().unwrap();
            let mut rhs = last_rect.x + last_rect.w;
            rhs += eol_nudge(font_size, &rendered.glyphs);
            rhs
        };

        let rect_w = self.rect.get().w;
        if rhs <= rect_w {
            return 0.;
        }
        let max_scroll = rhs - rect_w;
        max_scroll
    }

    fn clamp_scroll(&self, atom: &mut PropertyAtomicGuard) {
        let mut scroll = self.scroll.get();
        scroll = scroll.clamp(0., self.max_cursor_scroll());
        self.scroll.set(atom, scroll);
    }

    fn pause_blinking(&self) {
        self.blink_is_paused.store(true, Ordering::Relaxed);
        self.cursor_is_visible.store(true, Ordering::Relaxed);
    }

    async fn redraw(&self) {
        let timest = unixtime();
        t!("redraw()");

        let parent_rect = self.parent_rect.lock().unwrap().unwrap().clone();
        self.rect.eval(&parent_rect).expect("unable to eval rect");

        let Some(draw_update) = self.make_draw_calls() else {
            error!(target: "ui::editbox", "Text failed to draw");
            return;
        };

        self.render_api.replace_draw_calls(timest, draw_update.draw_calls);
    }

    fn redraw_cursor(&self) {
        let timest = unixtime();
        let cursor_instrs = self.get_cursor_instrs();

        let draw_calls = vec![(
            self.cursor_dc_key,
            GfxDrawCall { instrs: cursor_instrs, dcs: vec![], z_index: self.z_index.get() },
        )];

        self.render_api.replace_draw_calls(timest, draw_calls);
    }

    fn get_cursor_instrs(&self) -> Vec<GfxDrawInstruction> {
        if !self.is_focused.get() ||
            !self.cursor_is_visible.load(Ordering::Relaxed) ||
            self.hide_cursor.load(Ordering::Relaxed)
        {
            return vec![]
        }

        let mut cursor_instrs = vec![];

        let rect_w = self.rect.get().w;

        let mut cursor_pos = Point::zero();
        cursor_pos.x += self.cursor_px_offset();
        if cursor_pos.x > rect_w {
            return vec![]
        }
        cursor_instrs.push(GfxDrawInstruction::Move(cursor_pos));

        let cursor_mesh = {
            let mut cursor_mesh = self.cursor_mesh.lock().unwrap();
            if cursor_mesh.is_none() {
                *cursor_mesh = Some(self.regen_cursor_mesh());
            }
            cursor_mesh.clone().unwrap()
        };

        cursor_instrs.push(GfxDrawInstruction::Draw(cursor_mesh));

        cursor_instrs
    }

    fn make_draw_calls(&self) -> Option<DrawUpdate> {
        let rect = self.rect.get();

        let text_mesh = self.regen_text_mesh(rect.clone());
        let cursor_instrs = self.get_cursor_instrs();

        Some(DrawUpdate {
            key: self.text_dc_key,
            draw_calls: vec![
                (
                    self.text_dc_key,
                    GfxDrawCall {
                        instrs: vec![
                            GfxDrawInstruction::Move(rect.pos()),
                            GfxDrawInstruction::Draw(text_mesh),
                        ],
                        dcs: vec![self.cursor_dc_key],
                        z_index: self.z_index.get(),
                    },
                ),
                (
                    self.cursor_dc_key,
                    GfxDrawCall { instrs: cursor_instrs, dcs: vec![], z_index: self.z_index.get() },
                ),
            ],
        })
    }
}

impl Drop for EditBox {
    fn drop(&mut self) {
        self.render_api
            .replace_draw_calls(unixtime(), vec![(self.text_dc_key, Default::default())]);
    }
}

#[async_trait]
impl UIObject for EditBox {
    fn priority(&self) -> u32 {
        self.priority.get()
    }

    async fn start(self: Arc<Self>, ex: ExecutorPtr) {
        let me = Arc::downgrade(&self);

        let mut on_modify = OnModify::new(ex.clone(), self.node.clone(), me.clone());
        on_modify.when_change(self.is_focused.prop(), Self::change_focus);

        // When text has been changed.
        // Cursor and selection might be invalidated.
        async fn reset(self_: Arc<EditBox>) {
            let atom = &mut PropertyAtomicGuard::new();
            self_.cursor_pos.set(atom, 0);
            self_.selected.clone().set_null(atom, Role::Internal, 0).unwrap();
            self_.selected.clone().set_null(atom, Role::Internal, 1).unwrap();
            self_.scroll.set(atom, 0.);
            self_.redraw();
        }
        async fn redraw(self_: Arc<EditBox>) {
            self_.redraw().await;
        }
        on_modify.when_change(self.rect.prop(), redraw);
        on_modify.when_change(self.baseline.prop(), redraw);
        // The commented properties are modified on input events
        // So then redraw() will get repeatedly triggered when these properties
        // are changed. We should find a solution. For now the hooks are disabled.
        //on_modify.when_change(scroll.prop(), redraw);
        //on_modify.when_change(cursor_pos.prop(), redraw);
        on_modify.when_change(self.font_size.prop(), redraw);
        // We must also reshape text
        on_modify.when_change(self.text.prop(), reset);
        on_modify.when_change(self.text_color.prop(), redraw);
        on_modify.when_change(self.hi_bg_color.prop(), redraw);
        //on_modify.when_change(selected.clone(), redraw);
        on_modify.when_change(self.z_index.prop(), redraw);
        on_modify.when_change(self.debug.prop(), redraw);

        async fn regen_cursor(self_: Arc<EditBox>) {
            let mesh = std::mem::take(&mut *self_.cursor_mesh.lock().unwrap());
        }
        on_modify.when_change(self.cursor_color.prop(), regen_cursor);
        on_modify.when_change(self.cursor_ascent.prop(), regen_cursor);
        on_modify.when_change(self.cursor_descent.prop(), regen_cursor);
        on_modify.when_change(self.cursor_width.prop(), regen_cursor);

        let me2 = me.clone();
        let cursor_blink_time = self.cursor_blink_time.clone();
        let cursor_idle_time = self.cursor_idle_time.clone();
        let blinking_cursor_task = ex.spawn(async move {
            loop {
                msleep(cursor_blink_time.get() as u64).await;

                let self_ = me2.upgrade().unwrap();

                if self_.blink_is_paused.swap(false, Ordering::Relaxed) {
                    msleep(cursor_idle_time.get() as u64).await;
                    continue
                }

                // Invert the bool
                self_.cursor_is_visible.fetch_not(Ordering::Relaxed);
                self_.redraw_cursor();
            }
        });

        let mut tasks = on_modify.tasks;
        tasks.push(blinking_cursor_task);
        self.tasks.set(tasks);
    }

    async fn draw(
        &self,
        parent_rect: Rectangle,
        trace_id: u32,
        atom: &mut PropertyAtomicGuard,
    ) -> Option<DrawUpdate> {
        t!("EditBox::draw() [trace_id={trace_id}]");

        *self.parent_rect.lock().unwrap() = Some(parent_rect);
        self.rect.eval(&parent_rect).ok()?;

        self.clamp_scroll(atom);
        self.make_draw_calls()
    }

    async fn handle_char(&self, key: char, mods: KeyMods, repeat: bool) -> bool {
        // First filter for only single digit keys
        if DISALLOWED_CHARS.contains(&key) {
            return false
        }

        if !self.is_focused.get() {
            return false
        }

        let atom = &mut PropertyAtomicGuard::new();

        if mods.ctrl || mods.alt {
            if repeat {
                return false
            }
            self.handle_shortcut(key, &mods, atom).await;
            return true
        }

        let actions = {
            let mut repeater = self.key_repeat.lock().unwrap();
            repeater.key_down(PressedKey::Char(key), repeat)
        };
        t!("Key {key:?} has {actions} actions");
        for _ in 0..actions {
            self.insert_char(key).await;
        }
        true
    }

    async fn handle_key_down(&self, key: KeyCode, mods: KeyMods, repeat: bool) -> bool {
        // First filter for only single digit keys
        // Avoid processing events handled by insert_char()
        if !ALLOWED_KEYCODES.contains(&key) {
            return false
        }

        if !self.is_focused.get() {
            return false
        }

        let actions = {
            let mut repeater = self.key_repeat.lock().unwrap();
            repeater.key_down(PressedKey::Key(key), repeat)
        };

        let atom = &mut PropertyAtomicGuard::new();

        // Suppress noisy message
        if actions > 0 {
            t!("Key {key:?} has {actions} actions");
        }
        for _ in 0..actions {
            self.handle_key(&key, &mods, atom).await;
        }
        true
    }

    async fn handle_mouse_btn_down(&self, btn: MouseButton, mouse_pos: Point) -> bool {
        if !self.is_active.get() {
            return false
        }

        if btn != MouseButton::Left {
            return false
        }

        let rect = self.rect.get();
        let atom = &mut PropertyAtomicGuard::new();

        // clicking inside box will:
        // 1. make it active
        // 2. begin selection
        if !rect.contains(mouse_pos) {
            if self.is_focused.get() {
                d!("EditBox unfocused");
                self.is_focused.set(atom, false);
                self.select.lock().unwrap().clear();

                self.redraw().await;
            }
            return false
        }

        if self.is_focused.get() {
            d!("EditBox clicked");
        } else {
            d!("EditBox focused");
            self.is_focused.set(atom, true);
        }

        let font_size = self.font_size.get();
        let window_scale = self.window_scale.get();
        let baseline = self.baseline.get();

        {
            let mut editable = self.editable.lock().unwrap();
            let rendered = editable.render();
            let x = mouse_pos.x - rect.x + self.scroll.get();

            let cpos = rendered.x_to_pos(x, font_size, window_scale, baseline);
            let cidx = rendered.pos_to_idx(cpos);
            editable.set_cursor_idx(cidx);

            // begin selection
            let mut select = self.select.lock().unwrap();
            select.clear();
            select.push(Selection::new(cpos, cpos));

            self.mouse_btn_held.store(true, Ordering::Relaxed);
        }

        self.pause_blinking();
        self.redraw().await;
        true
    }

    async fn handle_mouse_btn_up(&self, btn: MouseButton, mouse_pos: Point) -> bool {
        if !self.is_active.get() {
            return false
        }

        // releasing mouse button will end selection
        self.mouse_btn_held.store(false, Ordering::Relaxed);
        false
    }

    async fn handle_mouse_move(&self, mouse_pos: Point) -> bool {
        if !self.is_active.get() {
            return false
        }

        let rect = self.rect.get();
        self.is_mouse_hover.store(rect.contains(mouse_pos), Ordering::Relaxed);

        if !self.mouse_btn_held.load(Ordering::Relaxed) {
            return false;
        }

        // if active and selection_active, then use x to modify the selection.
        // also implement scrolling when cursor is to the left or right
        // just scroll to the end
        // also set cursor_pos too

        let font_size = self.font_size.get();
        let window_scale = self.window_scale.get();
        let baseline = self.baseline.get();

        {
            let mut editable = self.editable.lock().unwrap();
            let rendered = editable.render();
            let x = mouse_pos.x - rect.x + self.scroll.get();

            let cpos = rendered.x_to_pos(x, font_size, window_scale, baseline);
            let cidx = rendered.pos_to_idx(cpos);
            editable.set_cursor_idx(cidx);

            // begin selection
            let mut select = self.select.lock().unwrap();
            select.last_mut().unwrap().end = cpos;
        }

        self.pause_blinking();
        //self.apply_cursor_scrolling();
        self.redraw().await;
        true
    }

    async fn handle_mouse_wheel(&self, wheel_pos: Point) -> bool {
        if !self.is_mouse_hover.load(Ordering::Relaxed) {
            return false
        }

        let atom = &mut PropertyAtomicGuard::new();

        let mut scroll = self.scroll.get() + wheel_pos.y * self.scroll_speed.get();
        scroll = scroll.clamp(0., self.max_cursor_scroll());
        t!("handle_mouse_wheel({wheel_pos:?}) [scroll={scroll}]");
        self.scroll.set(atom, scroll);
        self.redraw().await;

        true
    }

    async fn handle_touch(&self, phase: TouchPhase, id: u64, touch_pos: Point) -> bool {
        if !self.is_active.get() {
            return false
        }

        // Ignore multi-touch
        if id != 0 {
            return false
        }

        match phase {
            TouchPhase::Started => self.handle_touch_start(touch_pos).await,
            TouchPhase::Moved => self.handle_touch_move(touch_pos).await,
            TouchPhase::Ended => self.handle_touch_end(touch_pos).await,
            TouchPhase::Cancelled => false,
        }
    }

    async fn handle_compose_text(&self, suggest_text: &str, is_commit: bool) -> bool {
        t!("handle_compose_text({suggest_text}, {is_commit})");

        if !self.is_active.get() {
            return false
        }

        {
            let mut editable = self.editable.lock().unwrap();
            editable.compose(suggest_text, is_commit);
        }

        let atom = &mut PropertyAtomicGuard::new();
        //self.apply_cursor_scrolling();
        self.clamp_scroll(atom);
        self.redraw().await;

        true
    }
    async fn handle_set_compose_region(&self, start: usize, end: usize) -> bool {
        t!("handle_set_compose_region({start}, {end})");

        if !self.is_active.get() {
            return false
        }

        {
            let mut editable = self.editable.lock().unwrap();
            editable.set_compose_region(start, end);
        }

        self.redraw().await;

        true
    }
}

/// Filter these char events from being handled since we handle them
/// using the key_up/key_down events.
/// Avoids duplicate processing of keyboard input events.
pub static DISALLOWED_CHARS: &'static [char] = &['\r', '\u{8}', '\u{7f}', '\t', '\n'];

/// These keycodes are handled via normal key_up/key_down events.
/// Anything in this list must be disallowed char events.
pub static ALLOWED_KEYCODES: &'static [KeyCode] = &[
    KeyCode::Left,
    KeyCode::Right,
    KeyCode::Up,
    KeyCode::Down,
    KeyCode::Enter,
    KeyCode::Kp0,
    KeyCode::Kp1,
    KeyCode::Kp2,
    KeyCode::Kp3,
    KeyCode::Kp4,
    KeyCode::Kp5,
    KeyCode::Kp6,
    KeyCode::Kp7,
    KeyCode::Kp8,
    KeyCode::Kp9,
    KeyCode::KpDecimal,
    KeyCode::KpEnter,
    KeyCode::Delete,
    KeyCode::Backspace,
    KeyCode::Home,
    KeyCode::End,
];
