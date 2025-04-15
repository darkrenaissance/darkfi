/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
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

use crate::{
    android,
    gfx::Point,
    mesh::Color,
    prop::{PropertyColor, PropertyFloat32},
    text2::get_ctx,
};

macro_rules! t { ($($arg:tt)*) => { trace!(target: "text::editor::android", $($arg)*); } }

// You must be careful working with string indexes in Java. They are UTF16 string indexs, not UTF8
fn char16_to_byte_index(s: &str, char_idx: usize) -> Option<usize> {
    let utf16_data: Vec<_> = s.encode_utf16().take(char_idx).collect();
    let prestr = String::from_utf16(&utf16_data).ok()?;
    Some(prestr.len())
}
fn byte_to_char16_index(s: &str, byte_idx: usize) -> Option<usize> {
    if byte_idx > s.len() || !s.is_char_boundary(byte_idx) {
        return None;
    }
    Some(s[..byte_idx].encode_utf16().count())
}

pub struct Editor {
    pub composer_id: usize,
    is_init: bool,

    layout: parley::Layout<Color>,

    font_size: PropertyFloat32,
    text_color: PropertyColor,
    window_scale: PropertyFloat32,
    lineheight: PropertyFloat32,
}

impl Editor {
    pub async fn new(
        font_size: PropertyFloat32,
        text_color: PropertyColor,
        window_scale: PropertyFloat32,
        lineheight: PropertyFloat32,
    ) -> Self {
        Self {
            composer_id: usize::MAX,
            is_init: false,

            layout: Default::default(),
            font_size,
            text_color,
            window_scale,
            lineheight,
        }
    }

    pub fn init(&mut self) {
        android::focus(self.composer_id).unwrap();
    }
    pub fn setup(&mut self) {
        assert!(self.composer_id != usize::MAX);
        t!("Initialized composer [{}]", self.composer_id);
        let atxt = "A berry is small 😊 and pulpy.";
        android::set_text(self.composer_id, atxt).unwrap();
        self.is_init = true;
    }

    pub async fn refresh(&mut self) {
        let font_size = self.font_size.get();
        let text_color = self.text_color.get();
        let window_scale = self.window_scale.get();
        let lineheight = self.lineheight.get();

        let edit = android::get_editable(self.composer_id).unwrap();
        t!("refesh buffer = {}", edit.buffer);

        let mut txt_ctx = get_ctx().await;
        self.layout = txt_ctx.make_layout(
            &edit.buffer,
            text_color,
            font_size,
            lineheight,
            window_scale,
            None,
        );
    }

    pub fn layout(&self) -> &parley::Layout<Color> {
        &self.layout
    }

    pub fn move_to_pos(&self, pos: Point) {
        let cursor = parley::Cursor::from_point(&self.layout, pos.x, pos.y);

        let edit = android::get_editable(self.composer_id).unwrap();
        let cursor_idx = cursor.index();
        let pos = byte_to_char16_index(&edit.buffer, cursor_idx).unwrap();
        android::set_selection(self.composer_id, pos, pos);
        t!("  {cursor_idx} => {pos}");
    }

    pub fn get_cursor_pos(&self) -> Option<Point> {
        if !self.is_init {
            return None
        }

        let lineheight = self.lineheight.get();
        let edit = android::get_editable(self.composer_id).unwrap();

        //let buffer = android::get_raw_text(self.composer_id).unwrap();
        //let sel_start = android::get_selection_start(self.composer_id).unwrap();
        //let sel_end = android::get_selection_end(self.composer_id).unwrap();
        //if sel_start != sel_end || sel_start < 0 {
        //    return None
        //}
        let cursor_byte_idx = char16_to_byte_index(&edit.buffer, edit.select_start).unwrap();

        let cursor = if cursor_byte_idx >= edit.buffer.len() {
            parley::Cursor::from_byte_index(
                &self.layout,
                edit.buffer.len(),
                parley::Affinity::Upstream,
            )
        } else {
            parley::Cursor::from_byte_index(
                &self.layout,
                cursor_byte_idx,
                parley::Affinity::Downstream,
            )
        };
        let cursor_rect = cursor.geometry(&self.layout, lineheight);

        let cursor_pos = Point::new(cursor_rect.x0 as f32, cursor_rect.y0 as f32);
        Some(cursor_pos)
    }
}
