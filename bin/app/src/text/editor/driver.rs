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

#[cfg(not(target_os = "android"))]
use crate::{mesh::Color, text};

pub struct ParleyDriverWrapper<'a> {
    #[cfg(not(target_os = "android"))]
    editor: &'a mut parley::PlainEditor<Color>,
}

impl<'a> ParleyDriverWrapper<'a> {
    #[cfg(not(target_os = "android"))]
    pub fn new(editor: &'a mut parley::PlainEditor<Color>) -> Self {
        Self { editor }
    }

    #[cfg(target_os = "android")]
    pub fn new(_layout: &mut parley::Layout<Color>) -> Self {
        Self {}
    }

    #[cfg(not(target_os = "android"))]
    fn with_driver<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce(&mut parley::PlainEditorDriver<'_, Color>) -> R,
    {
        let mut font_ctx = text::GLOBAL_FONT_CTX.clone();
        text::THREAD_LAYOUT_CTX.with(|layout_ctx| {
            let mut layout_ctx = layout_ctx.borrow_mut();
            let mut driver = self.editor.driver(&mut font_ctx, &mut layout_ctx);
            f(&mut driver)
        })
    }

    pub fn select_all(&mut self) {
        #[cfg(not(target_os = "android"))]
        self.with_driver(|drv| drv.select_all());

        #[cfg(target_os = "android")]
        unimplemented!()
    }

    pub fn select_word_left(&mut self) {
        #[cfg(not(target_os = "android"))]
        self.with_driver(|drv| drv.select_word_left());

        #[cfg(target_os = "android")]
        unimplemented!()
    }

    pub fn move_word_left(&mut self) {
        #[cfg(not(target_os = "android"))]
        self.with_driver(|drv| drv.move_word_left());

        #[cfg(target_os = "android")]
        unimplemented!()
    }

    pub fn select_left(&mut self) {
        #[cfg(not(target_os = "android"))]
        self.with_driver(|drv| drv.select_left());

        #[cfg(target_os = "android")]
        unimplemented!()
    }

    pub fn move_left(&mut self) {
        #[cfg(not(target_os = "android"))]
        self.with_driver(|drv| drv.move_left());

        #[cfg(target_os = "android")]
        unimplemented!()
    }

    pub fn select_word_right(&mut self) {
        #[cfg(not(target_os = "android"))]
        self.with_driver(|drv| drv.select_word_right());

        #[cfg(target_os = "android")]
        unimplemented!()
    }

    pub fn move_word_right(&mut self) {
        #[cfg(not(target_os = "android"))]
        self.with_driver(|drv| drv.move_word_right());

        #[cfg(target_os = "android")]
        unimplemented!()
    }

    pub fn select_right(&mut self) {
        #[cfg(not(target_os = "android"))]
        self.with_driver(|drv| drv.select_right());

        #[cfg(target_os = "android")]
        unimplemented!()
    }

    pub fn move_right(&mut self) {
        #[cfg(not(target_os = "android"))]
        self.with_driver(|drv| drv.move_right());

        #[cfg(target_os = "android")]
        unimplemented!()
    }

    pub fn select_up(&mut self) {
        #[cfg(not(target_os = "android"))]
        self.with_driver(|drv| drv.select_up());

        #[cfg(target_os = "android")]
        unimplemented!()
    }

    pub fn move_up(&mut self) {
        #[cfg(not(target_os = "android"))]
        self.with_driver(|drv| drv.move_up());

        #[cfg(target_os = "android")]
        unimplemented!()
    }

    pub fn select_down(&mut self) {
        #[cfg(not(target_os = "android"))]
        self.with_driver(|drv| drv.select_down());

        #[cfg(target_os = "android")]
        unimplemented!()
    }

    pub fn move_down(&mut self) {
        #[cfg(not(target_os = "android"))]
        self.with_driver(|drv| drv.move_down());

        #[cfg(target_os = "android")]
        unimplemented!()
    }

    pub fn insert_or_replace_selection(&mut self, text: &str) {
        #[cfg(not(target_os = "android"))]
        self.with_driver(|drv| drv.insert_or_replace_selection(text));

        #[cfg(target_os = "android")]
        unimplemented!()
    }

    pub fn delete(&mut self) {
        #[cfg(not(target_os = "android"))]
        self.with_driver(|drv| drv.delete());

        #[cfg(target_os = "android")]
        unimplemented!()
    }

    pub fn delete_word(&mut self) {
        #[cfg(not(target_os = "android"))]
        self.with_driver(|drv| drv.delete_word());

        #[cfg(target_os = "android")]
        unimplemented!()
    }

    pub fn backdelete(&mut self) {
        #[cfg(not(target_os = "android"))]
        self.with_driver(|drv| drv.backdelete());

        #[cfg(target_os = "android")]
        unimplemented!()
    }

    pub fn backdelete_word(&mut self) {
        #[cfg(not(target_os = "android"))]
        self.with_driver(|drv| drv.backdelete_word());

        #[cfg(target_os = "android")]
        unimplemented!()
    }

    pub fn select_to_text_start(&mut self) {
        #[cfg(not(target_os = "android"))]
        self.with_driver(|drv| drv.select_to_text_start());

        #[cfg(target_os = "android")]
        unimplemented!()
    }

    pub fn move_to_text_start(&mut self) {
        #[cfg(not(target_os = "android"))]
        self.with_driver(|drv| drv.move_to_text_start());

        #[cfg(target_os = "android")]
        unimplemented!()
    }

    pub fn select_to_line_start(&mut self) {
        #[cfg(not(target_os = "android"))]
        self.with_driver(|drv| drv.select_to_line_start());

        #[cfg(target_os = "android")]
        unimplemented!()
    }

    pub fn move_to_line_start(&mut self) {
        #[cfg(not(target_os = "android"))]
        self.with_driver(|drv| drv.move_to_line_start());

        #[cfg(target_os = "android")]
        unimplemented!()
    }

    pub fn select_to_text_end(&mut self) {
        #[cfg(not(target_os = "android"))]
        self.with_driver(|drv| drv.select_to_text_end());

        #[cfg(target_os = "android")]
        unimplemented!()
    }

    pub fn move_to_text_end(&mut self) {
        #[cfg(not(target_os = "android"))]
        self.with_driver(|drv| drv.move_to_text_end());

        #[cfg(target_os = "android")]
        unimplemented!()
    }

    pub fn select_to_line_end(&mut self) {
        #[cfg(not(target_os = "android"))]
        self.with_driver(|drv| drv.select_to_line_end());

        #[cfg(target_os = "android")]
        unimplemented!()
    }

    pub fn move_to_line_end(&mut self) {
        #[cfg(not(target_os = "android"))]
        self.with_driver(|drv| drv.move_to_line_end());

        #[cfg(target_os = "android")]
        unimplemented!()
    }

    pub fn move_to_point(&mut self, x: f32, y: f32) {
        #[cfg(not(target_os = "android"))]
        self.with_driver(|drv| drv.move_to_point(x, y));

        #[cfg(target_os = "android")]
        unimplemented!()
    }

    pub fn extend_selection_to_point(&mut self, x: f32, y: f32) {
        #[cfg(not(target_os = "android"))]
        self.with_driver(|drv| drv.extend_selection_to_point(x, y));

        #[cfg(target_os = "android")]
        unimplemented!()
    }

    pub fn select_byte_range(&mut self, start: usize, end: usize) {
        #[cfg(not(target_os = "android"))]
        self.with_driver(|drv| drv.select_byte_range(start, end));

        #[cfg(target_os = "android")]
        unimplemented!()
    }
}
