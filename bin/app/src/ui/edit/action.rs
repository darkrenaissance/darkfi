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

use parking_lot::Mutex as SyncMutex;
use rand::{rngs::OsRng, Rng};

use crate::{
    gfx::{gfxtag, DrawInstruction, Point, Rectangle, Renderer},
    mesh::{Color, MeshBuilder},
    text,
};

macro_rules! d { ($($arg:tt)*) => { debug!(target: "ui::edit::action", $($arg)*); } }

struct MenuItem {
    layout: text::TextLayout,
    action_id: u32,
    rect: Rectangle,
}

#[derive(Copy, Clone, PartialEq)]
pub(super) struct Style {
    pub font_size: f32,
    pub fg_color: Color,
    pub bg_color: Color,
    pub padding: f32,
    pub spacing: f32,
    pub window_scale: f32,
}

pub struct Menu {
    /// Requested widget-local position, before draw-time clamping.
    pub anchor: Point,
    items: Vec<(String, u32)>,
    style: Option<Style>,
    /// Position and items used by the last draw, not pending logical changes.
    pos: Point,
    rendered_items: Vec<MenuItem>,
}

impl Menu {
    pub fn new() -> Self {
        Self {
            anchor: Point::zero(),
            items: vec![],
            style: None,
            pos: Point::zero(),
            rendered_items: vec![],
        }
    }

    pub fn add(&mut self, label: &str, action: u32) {
        self.items.push((label.to_owned(), action));
        self.style = None;
    }
}

pub struct ActionMode {
    pub dc_key: u64,

    menu: SyncMutex<Option<Menu>>,
    renderer: Renderer,
}

impl ActionMode {
    pub fn new(renderer: Renderer) -> Self {
        Self { dc_key: OsRng.gen(), menu: SyncMutex::new(None), renderer }
    }

    pub fn set(&self, menu: Menu) {
        *self.menu.lock() = Some(menu);
    }

    /// Dismiss the overlay. Call whenever the text/selection it refers to
    /// is invalidated (text edit, cursor tap) so a stale menu can't linger.
    pub fn clear(&self) {
        *self.menu.lock() = None;
    }

    /// Returns `Some(n)` if item n is selected.
    pub fn interact(&self, pos: Point) -> Option<u32> {
        let menu = std::mem::take(&mut *self.menu.lock())?;

        let local_pos = pos - menu.pos;

        for item in &menu.rendered_items {
            if item.rect.contains(local_pos) {
                d!("Action clicked: {}", item.action_id);
                return Some(item.action_id);
            }
        }

        d!("Nothing clicked");
        None
    }

    /// Non-consuming hit test: whether `pos` (widget-local) lands on a
    /// menu item. Used for gesture hit-testing so the menu overlay can
    /// be grabbed without consuming it.
    pub fn hit(&self, pos: Point) -> bool {
        let menu = self.menu.lock();
        let Some(menu) = &*menu else { return false };

        let local_pos = pos - menu.pos;
        menu.rendered_items.iter().any(|item| item.rect.contains(local_pos))
    }

    /// Called by the parent layout
    pub(super) fn get_instrs(&self, style: Style, width: f32) -> Vec<DrawInstruction> {
        let mut menu = self.menu.lock();
        let Some(menu) = &mut *menu else { return vec![] };

        if menu.style != Some(style) {
            let mut x_offset = 0.;
            menu.rendered_items.clear();
            for (label, action_id) in &menu.items {
                let layout = text::make_layout(
                    label,
                    style.fg_color,
                    style.font_size,
                    0.,
                    style.window_scale,
                    None,
                    &[],
                );
                let item_height = style.font_size + 2. * style.padding;
                let rect = Rectangle::new(
                    x_offset,
                    -item_height,
                    layout.width() + 2. * style.padding,
                    item_height,
                );
                x_offset = rect.rhs() + style.spacing;
                menu.rendered_items.push(MenuItem { layout, action_id: *action_id, rect });
            }
            menu.style = Some(style);
        }

        let mut total_width = 0.;
        if let Some(item) = menu.rendered_items.last() {
            total_width = item.rect.rhs();
        }
        menu.pos = menu.anchor;
        let max_x = width - total_width;
        menu.pos.x = menu.pos.x.clamp(0., max_x.max(0.));

        let mut instrs = vec![DrawInstruction::Move(menu.pos)];

        for item in &menu.rendered_items {
            // Used to reset the pos again
            let mut off_pos = Point::zero();

            // Draw background with border
            let mut mesh = MeshBuilder::new(gfxtag!("action_bg"));
            let bg_rect = item.rect.with_zero_pos();
            mesh.draw_filled_box(&bg_rect, style.bg_color);
            mesh.draw_outline(&bg_rect, style.fg_color, 1.);

            off_pos -= item.rect.pos();
            instrs.push(DrawInstruction::Move(item.rect.pos()));
            instrs.push(DrawInstruction::Draw(mesh.alloc(&self.renderer).draw_untextured()));

            // Draw text label
            let layout_height = item.layout.height();
            // Center text vertically
            let text_y = (item.rect.h - layout_height) / 2.;
            let text_pos = Point::new(style.padding, text_y);
            let mut txt_instrs =
                text::render_layout(&item.layout, &self.renderer, gfxtag!("action_txt"));
            off_pos -= text_pos;
            instrs.push(DrawInstruction::Move(text_pos));
            instrs.append(&mut txt_instrs);

            // Reset cursor
            instrs.push(DrawInstruction::Move(off_pos));
        }

        vec![DrawInstruction::Overlay(instrs)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gfx::GraphicsMethod;

    fn style() -> Style {
        Style {
            font_size: 18.,
            fg_color: [0.9, 0.8, 0.7, 1.],
            bg_color: [0.1, 0.2, 0.3, 1.],
            padding: 4.,
            spacing: 8.,
            window_scale: 1.,
        }
    }

    fn menu() -> Menu {
        let mut menu = Menu::new();
        menu.add("Copy", 10);
        menu.add("Paste", 20);
        menu.anchor = Point::new(80., 100.);
        menu
    }

    #[test]
    fn first_draw_builds_menu_and_enables_matching_hits() {
        let (tx, rx) = async_channel::unbounded();
        let mode = ActionMode::new(Renderer::new(tx));
        assert!(mode.get_instrs(style(), 1000.).is_empty());
        mode.set(menu());
        assert!(rx.is_empty());
        assert!(!mode.hit(Point::zero()));
        assert!(!mode.hit(Point::new(81., 99.)));
        {
            let menu = mode.menu.lock();
            let menu = menu.as_ref().unwrap();
            assert!(menu.style.is_none());
            assert!(menu.rendered_items.is_empty());
        }

        let instrs = mode.get_instrs(style(), 1000.);
        let [DrawInstruction::Overlay(overlay)] = instrs.as_slice() else { panic!() };
        assert!(matches!(overlay[0], DrawInstruction::Move(pos) if pos == Point::new(80., 100.)));
        let centers = {
            let menu = mode.menu.lock();
            let menu = menu.as_ref().unwrap();
            assert_eq!(menu.pos, menu.anchor);
            menu.rendered_items.iter().map(|item| menu.pos + item.rect.center()).collect::<Vec<_>>()
        };
        assert!(centers.iter().all(|pos| mode.hit(*pos)));
        assert_eq!(mode.interact(centers[1]), Some(20));
        assert!(!mode.hit(centers[0]));
        assert!(mode.get_instrs(style(), 1000.).is_empty());
    }

    #[test]
    fn every_style_field_refreshes_layout_meshes_and_hit_geometry() {
        let (tx, rx) = async_channel::unbounded();
        let mode = ActionMode::new(Renderer::new(tx));
        mode.set(menu());
        let base = style();
        for current in [
            base,
            Style { font_size: 30., ..base },
            Style { fg_color: [0.3, 0.7, 0.2, 0.8], ..base },
            Style { bg_color: [0.8, 0.1, 0.5, 0.6], ..base },
            Style { padding: 12., ..base },
            Style { spacing: 24., ..base },
            Style { window_scale: 2., ..base },
        ] {
            // Start each case from the same rendered style to exercise each cache key alone.
            mode.get_instrs(base, 240.);
            while rx.try_recv().is_ok() {}
            let instrs = mode.get_instrs(current, 240.);
            let [DrawInstruction::Overlay(overlay)] = instrs.as_slice() else { panic!() };
            let (pos, rects) = {
                let menu = mode.menu.lock();
                let menu = menu.as_ref().unwrap();
                assert!(menu.style == Some(current));
                assert_eq!(menu.items, vec![("Copy".to_owned(), 10), ("Paste".to_owned(), 20)]);
                let mut x = 0.;
                for (item, (label, action_id)) in menu.rendered_items.iter().zip(&menu.items) {
                    let expected = text::make_layout(
                        label,
                        current.fg_color,
                        current.font_size,
                        0.,
                        current.window_scale,
                        None,
                        &[],
                    );
                    let height = current.font_size + 2. * current.padding;
                    assert_eq!(
                        item.rect,
                        Rectangle::new(x, -height, expected.width() + 2. * current.padding, height)
                    );
                    assert_eq!(item.action_id, *action_id);
                    assert_eq!(item.layout.scale(), current.window_scale);
                    for line in item.layout.lines() {
                        for run in line.items() {
                            if let parley::PositionedLayoutItem::GlyphRun(run) = run {
                                assert_eq!(run.style().brush, current.fg_color);
                                assert_eq!(
                                    run.run().font_size(),
                                    current.font_size * current.window_scale
                                );
                            }
                        }
                    }
                    x = item.rect.rhs() + current.spacing;
                }
                let total_width = menu.rendered_items.last().unwrap().rect.rhs();
                assert_eq!(
                    menu.pos,
                    Point::new(80_f32.clamp(0., (240. - total_width).max(0.)), 100.)
                );
                (menu.pos, menu.rendered_items.iter().map(|item| item.rect).collect::<Vec<_>>())
            };
            let mut cursor = Point::zero();
            let mut backgrounds = 0;
            for instr in overlay {
                match instr {
                    DrawInstruction::Move(offset) => cursor += *offset,
                    DrawInstruction::Draw(mesh) if mesh.textures.is_none() => {
                        assert!(cursor.dist(pos + rects[backgrounds].pos()) < 0.001);
                        backgrounds += 1;
                    }
                    _ => {}
                }
            }
            assert_eq!(backgrounds, 2);
            assert!(rects.iter().all(|rect| mode.hit(pos + rect.center())));
            assert!(!mode.hit(pos + Point::new(rects[0].rhs() + current.spacing / 2., -1.)));

            let mut backgrounds = 0;
            let mut text_meshes = 0;
            while let Ok((_, method)) = rx.try_recv() {
                if let GraphicsMethod::NewVertexBuffer((verts, _, tag)) = method {
                    if tag == gfxtag!("action_bg") {
                        assert!(verts[..4].iter().all(|v| v.color == current.bg_color));
                        assert!(verts[4..].iter().all(|v| v.color == current.fg_color));
                        assert_eq!(verts[3].pos, [rects[backgrounds].w, rects[backgrounds].h]);
                        backgrounds += 1;
                    } else if tag == gfxtag!("action_txt") {
                        assert!(!verts.is_empty());
                        assert!(verts.iter().all(|v| v.color == current.fg_color));
                        text_meshes += 1;
                    }
                }
            }
            assert_eq!(backgrounds, 2);
            assert!(text_meshes >= 2);
        }
    }

    #[test]
    fn restyling_replaces_stale_hit_rects_and_preserves_actions() {
        let (tx, _rx) = async_channel::unbounded();
        let mode = ActionMode::new(Renderer::new(tx));
        mode.set(menu());
        mode.get_instrs(Style { font_size: 36., padding: 20., ..style() }, 2000.);
        let (old_top, old_second) = {
            let menu = mode.menu.lock();
            let menu = menu.as_ref().unwrap();
            (
                menu.pos + menu.rendered_items[0].rect.pos() + Point::new(1., 1.),
                menu.pos + Point::new(menu.rendered_items[1].rect.center().x, -1.),
            )
        };
        assert!(mode.hit(old_top));
        assert!(mode.hit(old_second));
        let current = Style { spacing: 500., ..style() };
        mode.get_instrs(current, 2000.);
        assert!(!mode.hit(old_top));
        assert!(!mode.hit(old_second));
        let centers = {
            let menu = mode.menu.lock();
            let menu = menu.as_ref().unwrap();
            menu.rendered_items.iter().map(|item| menu.pos + item.rect.center()).collect::<Vec<_>>()
        };
        for (pos, action_id) in centers.into_iter().zip([10, 20]) {
            assert!(mode.hit(pos));
            assert_eq!(mode.interact(pos), Some(action_id));
            mode.set(menu());
            mode.get_instrs(current, 2000.);
        }
        assert_eq!(mode.interact(old_second), None);
        assert!(mode.get_instrs(current, 2000.).is_empty());
    }

    #[test]
    fn width_changes_reclamp_and_restore_anchor_without_rebuilding_layout() {
        let (tx, _rx) = async_channel::unbounded();
        let mode = ActionMode::new(Renderer::new(tx));
        mode.set(menu());
        mode.get_instrs(style(), 1000.);
        let (total_width, cached_items) = {
            let menu = mode.menu.lock();
            let menu = menu.as_ref().unwrap();
            (menu.rendered_items.last().unwrap().rect.rhs(), menu.rendered_items.as_ptr())
        };
        let narrow_width = total_width + 20.;
        for (width, x) in [
            (narrow_width, narrow_width - total_width),
            (10., 0.),
            (0., 0.),
            (1000., 80.),
            (1000., 80.),
        ] {
            let instrs = mode.get_instrs(style(), width);
            let [DrawInstruction::Overlay(overlay)] = instrs.as_slice() else { panic!() };
            assert!(matches!(overlay[0], DrawInstruction::Move(pos) if pos == Point::new(x, 100.)));
            let center = {
                let menu = mode.menu.lock();
                let menu = menu.as_ref().unwrap();
                assert_eq!(menu.anchor, Point::new(80., 100.));
                assert_eq!(menu.pos, Point::new(x, 100.));
                assert_eq!(menu.rendered_items.as_ptr(), cached_items);
                menu.pos + menu.rendered_items[0].rect.center()
            };
            assert!(mode.hit(center));
            assert!(!mode.hit(Point::new(x - 1., 99.)));
        }
        mode.menu.lock().as_mut().unwrap().anchor = Point::new(-30., -10.);
        mode.get_instrs(style(), 1000.);
        let menu = mode.menu.lock();
        let menu = menu.as_ref().unwrap();
        assert_eq!(menu.anchor, Point::new(-30., -10.));
        assert_eq!(menu.pos, Point::new(0., -10.));
        assert_eq!(menu.rendered_items.as_ptr(), cached_items);
    }

    #[test]
    fn pending_menu_changes_do_not_expose_unrendered_hit_targets() {
        let (tx, _rx) = async_channel::unbounded();
        let mode = ActionMode::new(Renderer::new(tx));
        mode.set(menu());
        assert_eq!(mode.interact(Point::new(81., 99.)), None);
        mode.set(menu());
        mode.get_instrs(style(), 2000.);
        let old_hit = Point::new(81., 99.);
        {
            let mut menu = mode.menu.lock();
            let menu = menu.as_mut().unwrap();
            menu.anchor = Point::new(1000., 200.);
            menu.add("Select All", 30);
        }
        assert!(mode.hit(old_hit));
        assert!(!mode.hit(Point::new(1001., 199.)));
        mode.get_instrs(style(), 2000.);
        assert!(!mode.hit(old_hit));
        let new_hit = {
            let menu = mode.menu.lock();
            let menu = menu.as_ref().unwrap();
            menu.pos + menu.rendered_items[2].rect.center()
        };
        assert!(mode.hit(new_hit));
        assert_eq!(mode.interact(new_hit), Some(30));

        mode.set(menu());
        mode.get_instrs(style(), 2000.);
        mode.set(menu());
        assert!(!mode.hit(old_hit));
        assert_eq!(mode.interact(old_hit), None);
        mode.set(menu());
        mode.get_instrs(style(), 2000.);
        mode.clear();
        assert!(!mode.hit(old_hit));
        assert_eq!(mode.interact(old_hit), None);
    }
}
