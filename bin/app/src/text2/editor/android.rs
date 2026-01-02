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
    android::{
        self,
        textinput::{AndroidTextInput, AndroidTextInputState},
    },
    gfx::Point,
    mesh::Color,
    prop::{PropertyAtomicGuard, PropertyColor, PropertyFloat32, PropertyStr},
    text2::{TextContext, TEXT_CTX},
};
use std::cmp::{max, min};

macro_rules! t { ($($arg:tt)*) => { trace!(target: "text::editor::android", $($arg)*); } }

pub struct Editor {
    input: AndroidTextInput,
    pub state: AndroidTextInputState,
    pub recvr: async_channel::Receiver<AndroidTextInputState>,

    layout: parley::Layout<Color>,
    width: Option<f32>,

    text: PropertyStr,
    font_size: PropertyFloat32,
    text_color: PropertyColor,
    window_scale: PropertyFloat32,
    lineheight: PropertyFloat32,
}

impl Editor {
    pub fn new(
        text: PropertyStr,
        font_size: PropertyFloat32,
        text_color: PropertyColor,
        window_scale: PropertyFloat32,
        lineheight: PropertyFloat32,
    ) -> Self {
        let (sender, recvr) = async_channel::unbounded();
        let input = AndroidTextInput::new(sender);
        Self {
            input,
            state: Default::default(),
            recvr,

            layout: Default::default(),
            width: None,

            text,
            font_size,
            text_color,
            window_scale,
            lineheight,
        }
    }

    pub async fn on_text_prop_changed(&mut self) {
        // Update GameTextInput state
        self.state.text = self.text.get();
        self.state.select = (0, 0);
        self.state.compose = None;
        self.input.set_state(self.state.clone());
        // Refresh our layout
        self.refresh().await;
    }
    pub async fn on_buffer_changed(&mut self, atom: &mut PropertyAtomicGuard) {
        // Refresh the layout using the Android buffer
        self.refresh().await;

        // Update the text attribute
        self.text.set(atom, &self.state.text);
    }

    pub fn focus(&mut self) {
        self.input.show();
    }
    pub fn unfocus(&mut self) {
        self.input.hide();
    }

    pub async fn refresh(&mut self) {
        let font_size = self.font_size.get();
        let text_color = self.text_color.get();
        let window_scale = self.window_scale.get();
        let lineheight = self.lineheight.get();

        let mut underlines = vec![];
        if let Some((compose_start, compose_end)) = self.state.compose {
            underlines.push(compose_start..compose_end);
        }

        let mut txt_ctx = TEXT_CTX.get().await;
        self.layout = txt_ctx.make_layout(
            &self.state.text,
            text_color,
            font_size,
            lineheight,
            window_scale,
            self.width,
            &underlines,
        );
    }

    pub fn layout(&self) -> &parley::Layout<Color> {
        &self.layout
    }

    pub fn move_to_pos(&mut self, pos: Point) {
        let cursor = parley::Cursor::from_point(&self.layout, pos.x, pos.y);
        let cursor_idx = cursor.index();
        t!("  move_to_pos: {cursor_idx}");
        self.state.text = self.text.get();
        self.state.select = (cursor_idx, cursor_idx);
        self.state.compose = None;
        self.input.set_state(self.state.clone());
    }

    pub async fn select_word_at_point(&mut self, pos: Point) {
        let select = parley::Selection::word_from_point(&self.layout, pos.x, pos.y);
        assert!(!select.is_collapsed());
        let select = select.text_range();
        self.set_selection(select.start, select.end).await;
    }

    pub fn get_cursor_pos(&self) -> Point {
        let lineheight = self.lineheight.get();
        let cursor_idx = self.state.select.0;

        let cursor = if cursor_idx >= self.state.text.len() {
            parley::Cursor::from_byte_index(
                &self.layout,
                self.state.text.len(),
                parley::Affinity::Upstream,
            )
        } else {
            parley::Cursor::from_byte_index(&self.layout, cursor_idx, parley::Affinity::Downstream)
        };
        let cursor_rect = cursor.geometry(&self.layout, lineheight);
        Point::new(cursor_rect.x0 as f32, cursor_rect.y0 as f32)
    }

    pub async fn insert(&mut self, txt: &str, atom: &mut PropertyAtomicGuard) {
        // TODO: need to verify this is correct
        // Insert text by updating the state
        self.state.text.push_str(txt);
        let cursor_idx = self.state.text.len();
        self.state.select = (cursor_idx, cursor_idx);
        self.state.compose = None;
        self.input.set_state(self.state.clone());
        self.on_buffer_changed(atom).await;
    }

    pub fn driver<'a>(
        &'a mut self,
        _txt_ctx: &'a mut TextContext,
    ) -> Option<parley::PlainEditorDriver<'a, Color>> {
        None
    }

    pub fn set_width(&mut self, w: f32) {
        self.width = Some(w);
    }
    pub fn width(&self) -> f32 {
        self.layout().full_width()
    }
    pub fn height(&self) -> f32 {
        self.layout().height()
    }

    pub fn selected_text(&self) -> Option<String> {
        let (start, end) = (self.state.select.0, self.state.select.1);
        if start == end {
            return None
        }
        let (start, end) = (min(start, end), max(start, end));
        Some(self.state.text[start..end].to_string())
    }
    pub fn selection(&self, side: isize) -> parley::Selection {
        assert!(side.abs() == 1);
        t!("selection({side}) [state={:?}]", self.state);

        let (start, end) = (self.state.select.0, self.state.select.1);
        let (anchor, focus) = match side {
            -1 => (end, start),
            1 => (start, end),
            _ => panic!(),
        };

        let anchor =
            parley::Cursor::from_byte_index(&self.layout, anchor, parley::Affinity::Downstream);
        let focus =
            parley::Cursor::from_byte_index(&self.layout, focus, parley::Affinity::Downstream);

        parley::Selection::new(anchor, focus)
    }
    pub async fn set_selection(&mut self, select_start: usize, select_end: usize) {
        self.state.text = self.text.get();
        self.state.select = (select_start, select_end);
        self.state.compose = None;
        self.input.set_state(self.state.clone());
    }

    #[allow(dead_code)]
    pub fn buffer(&self) -> String {
        self.state.text.clone()
    }
}
