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
    gfx::{gfxtag, DrawCall, DrawInstruction, Point, Rectangle, RenderApi, Renderer},
    mesh::{Color, MeshBuilder},
    prop::BatchGuardId,
    text,
};

macro_rules! d { ($($arg:tt)*) => { debug!(target: "ui::edit::action", $($arg)*); } }

struct MenuItem {
    layout: parley::Layout<Color>,
    action_id: u32,
    rect: Rectangle,
}

pub struct Menu {
    font_size: f32,
    fg_color: Color,
    bg_color: Color,
    padding: f32,
    spacing: f32,
    window_scale: f32,
    pub pos: Point,
    items: Vec<MenuItem>,
}

impl Menu {
    pub fn new(
        font_size: f32,
        fg_color: Color,
        bg_color: Color,
        padding: f32,
        spacing: f32,
        window_scale: f32,
    ) -> Self {
        Self {
            font_size,
            fg_color,
            bg_color,
            padding,
            spacing,
            window_scale,
            pos: Point::zero(),
            items: vec![],
        }
    }

    pub fn add(&mut self, label: &str, action: u32) {
        let layout = text::make_layout(
            label,
            self.fg_color,
            self.font_size,
            0.,
            self.window_scale,
            None,
            &vec![],
        );

        let item_width = layout.width();
        let item_height = self.font_size + 2. * self.padding;

        let x_offset = match self.items.last() {
            Some(item) => item.rect.rhs() + self.spacing,
            None => 0.,
        };

        let rect =
            Rectangle::new(x_offset, -item_height, item_width + 2. * self.padding, item_height);

        self.items.push(MenuItem { layout, action_id: action, rect });
    }

    pub fn total_width(&self) -> f32 {
        match self.items.last() {
            Some(item) => item.rect.rhs(),
            None => 0.,
        }
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

    /// Returns `Some(n)` if item n is selected.
    pub fn interact(&self, pos: Point) -> Option<u32> {
        let menu = std::mem::take(&mut *self.menu.lock())?;

        let local_pos = pos - menu.pos;
        //d!("interact: pos={:?}, menu.pos={:?}, local_pos={:?}", pos, menu.pos, local_pos);

        for item in &menu.items {
            if item.rect.contains(local_pos) {
                d!("Action clicked: {}", item.action_id);
                return Some(item.action_id);
            }
        }

        d!("Nothing clicked");
        None
    }

    /// Called by the parent layout
    pub fn get_instrs(&self) -> Vec<DrawInstruction> {
        let Some(menu) = &*self.menu.lock() else { return vec![] };

        let mut instrs = vec![DrawInstruction::Move(menu.pos)];

        for item in &menu.items {
            // Used to reset the pos again
            let mut off_pos = Point::zero();

            // Draw background with border
            let mut mesh = MeshBuilder::new(gfxtag!("action_bg"));
            let bg_rect = item.rect.with_zero_pos();
            mesh.draw_filled_box(&bg_rect, menu.bg_color);
            mesh.draw_outline(&bg_rect, menu.fg_color, 1.);

            off_pos -= item.rect.pos();
            instrs.push(DrawInstruction::Move(item.rect.pos()));
            instrs.push(DrawInstruction::Draw(mesh.alloc(&self.renderer).draw_untextured()));

            // Draw text label
            let layout_height = item.layout.height();
            // Center text vertically
            let text_y = (item.rect.h - layout_height) / 2.;
            let text_pos = Point::new(menu.padding, text_y);
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

    /// When theres a state change, call this to update the draw cmds.
    pub fn redraw(&self, batch_id: BatchGuardId) {
        let dcs =
            vec![(self.dc_key, DrawCall::new(self.get_instrs(), vec![], 1, "chatedit_action"))];
        self.renderer.replace_draw_calls(Some(batch_id), dcs);
    }
}
