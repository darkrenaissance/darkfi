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
    prop::{PropertyAtomicGuard, PropertyColor, PropertyFloat32, PropertyStr},
    text2::{TextContext, TEXT_CTX},
    AndroidSuggestEvent,
};
use std::{
    cmp::{max, min},
    sync::atomic::{AtomicBool, Ordering},
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
        return None
    }
    Some(s[..byte_idx].encode_utf16().count())
}

pub struct Editor {
    pub composer_id: usize,
    pub recvr: async_channel::Receiver<AndroidSuggestEvent>,
    is_init: bool,
    is_setup: bool,
    /// We cannot receive focus until `AndroidSuggestEvent::Init` has finished.
    /// We use this flag to delay calling `android::focus()` until the init has completed.
    is_focus_req: AtomicBool,

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
        let composer_id = android::create_composer(sender);
        t!("Created composer [{composer_id}]");

        Self {
            composer_id,
            recvr,
            is_init: false,
            is_setup: false,
            is_focus_req: AtomicBool::new(false),

            layout: Default::default(),
            width: None,

            text,
            font_size,
            text_color,
            window_scale,
            lineheight,
        }
    }

    /// Called on `AndroidSuggestEvent::Init` after the View has been added to the main hierarchy
    /// and is ready to receive commands such as focus.
    pub fn init(&mut self) {
        self.is_init = true;

        // Perform any focus requests.
        let is_focus_req = self.is_focus_req.swap(false, Ordering::SeqCst);
        if is_focus_req {
            android::focus(self.composer_id).unwrap();
        }

        //android::focus(self.composer_id).unwrap();
        //let atxt = "A berry is small juicy 😊 pulpy and edible.";
        //let atxt = "A berry is a small, pulpy, and often edible fruit. Typically, berries are juicy, rounded, brightly colored, sweet, sour or tart, and do not have a stone or pit, although many pips or seeds may be present. Common examples of berries in the culinary sense are strawberries, raspberries, blueberries, blackberries, white currants, blackcurrants, and redcurrants. In Britain, soft fruit is a horticultural term for such fruits. The common usage of the term berry is different from the scientific or botanical definition of a berry, which refers to a fruit produced from the ovary of a single flower where the outer layer of the ovary wall develops into an edible fleshy portion (pericarp). The botanical definition includes many fruits that are not commonly known or referred to as berries, such as grapes, tomatoes, cucumbers, eggplants, bananas, and chili peppers.";
        //let atxt = "small berry terry";
        //android::set_text(self.composer_id, atxt);
        //self.set_selection(2, 7);
        // Call this after:
        //self.on_buffer_changed(&mut PropertyAtomicGuard::none()).await;
    }
    /// Called on `AndroidSuggestEvent::CreateInputConnect`, which only happens after the View
    /// is focused for the first time.
    pub fn setup(&mut self) {
        assert!(self.is_init);
        self.is_setup = true;

        assert!(self.composer_id != usize::MAX);
        t!("Initialized composer [{}]", self.composer_id);
    }

    pub async fn on_text_prop_changed(&mut self) {
        // Get modified text property
        let txt = self.text.get();
        // Update Android text buffer
        android::set_text(self.composer_id, &txt);
        assert_eq!(android::get_editable(self.composer_id).unwrap().buffer, txt);
        // Refresh our layout
        self.refresh().await;
    }
    pub async fn on_buffer_changed(&mut self, atom: &mut PropertyAtomicGuard) {
        // Refresh the layout using the Android buffer
        self.refresh().await;

        // Update the text attribute
        let edit = android::get_editable(self.composer_id).unwrap();
        self.text.set(atom, &edit.buffer);
    }

    /// Can only be called after AndroidSuggestEvent::Init.
    pub fn focus(&self) {
        // We're not yet ready to receive focus
        if !self.is_init {
            self.is_focus_req.store(true, Ordering::SeqCst);
            return
        }
        android::focus(self.composer_id).unwrap();
    }
    pub fn unfocus(&self) {
        android::unfocus(self.composer_id).unwrap();
    }

    pub async fn refresh(&mut self) {
        let font_size = self.font_size.get();
        let text_color = self.text_color.get();
        let window_scale = self.window_scale.get();
        let lineheight = self.lineheight.get();

        let edit = android::get_editable(self.composer_id).unwrap();

        let mut underlines = vec![];
        if let Some(compose_start) = edit.compose_start {
            let compose_end = edit.compose_end.unwrap();

            let compose_start = char16_to_byte_index(&edit.buffer, compose_start).unwrap();
            let compose_end = char16_to_byte_index(&edit.buffer, compose_end).unwrap();
            underlines.push(compose_start..compose_end);
        }

        let mut txt_ctx = TEXT_CTX.get().await;
        self.layout = txt_ctx.make_layout(
            &edit.buffer,
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

    pub fn move_to_pos(&self, pos: Point) {
        let cursor = parley::Cursor::from_point(&self.layout, pos.x, pos.y);

        let edit = android::get_editable(self.composer_id).unwrap();
        let cursor_idx = cursor.index();
        let pos = byte_to_char16_index(&edit.buffer, cursor_idx).unwrap();
        t!("  {cursor_idx} => {pos}");
        android::set_selection(self.composer_id, pos, pos);
    }

    pub async fn select_word_at_point(&mut self, pos: Point) {
        let select = parley::Selection::word_from_point(&self.layout, pos.x, pos.y);
        assert!(!select.is_collapsed());
        let select = select.text_range();
        self.set_selection(select.start, select.end).await;
    }

    pub fn get_cursor_pos(&self) -> Point {
        let lineheight = self.lineheight.get();
        let edit = android::get_editable(self.composer_id).unwrap();

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
        Point::new(cursor_rect.x0 as f32, cursor_rect.y0 as f32)
    }

    pub async fn insert(&mut self, txt: &str, atom: &mut PropertyAtomicGuard) {
        android::commit_text(self.composer_id, txt);
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
        let edit = android::get_editable(self.composer_id).unwrap();
        if edit.select_start == edit.select_end {
            return None
        }
        let anchor = char16_to_byte_index(&edit.buffer, edit.select_start).unwrap();
        let index = char16_to_byte_index(&edit.buffer, edit.select_end).unwrap();
        let (start, end) = (min(anchor, index), max(anchor, index));
        Some(edit.buffer[start..end].to_string())
    }
    pub fn selection(&self, side: isize) -> parley::Selection {
        assert!(side.abs() == 1);
        let edit = android::get_editable(self.composer_id).unwrap();

        let select_start = char16_to_byte_index(&edit.buffer, edit.select_start).unwrap();
        let select_end = char16_to_byte_index(&edit.buffer, edit.select_end).unwrap();
        //t!("selection() -> ({select_start}, {select_end})");

        let (anchor, focus) = match side {
            -1 => (select_end, select_start),
            1 => (select_start, select_end),
            _ => panic!(),
        };

        let anchor =
            parley::Cursor::from_byte_index(&self.layout, anchor, parley::Affinity::Downstream);
        let focus =
            parley::Cursor::from_byte_index(&self.layout, focus, parley::Affinity::Downstream);

        parley::Selection::new(anchor, focus)
    }
    pub async fn set_selection(&mut self, select_start: usize, select_end: usize) {
        //t!("set_selection({select_start}, {select_end})");
        let edit = android::get_editable(self.composer_id).unwrap();
        let select_start = byte_to_char16_index(&edit.buffer, select_start).unwrap();
        let select_end = byte_to_char16_index(&edit.buffer, select_end).unwrap();
        android::set_selection(self.composer_id, select_start, select_end);
    }

    #[allow(dead_code)]
    pub fn buffer(&self) -> String {
        let edit = android::get_editable(self.composer_id).unwrap();
        edit.buffer
    }
}
