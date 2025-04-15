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
    gfx::Point,
    mesh::Color,
    prop::{PropertyColor, PropertyFloat32},
    text2::{get_ctx, TextContext, FONT_STACK},
};

macro_rules! t { ($($arg:tt)*) => { trace!(target: "text::editor", $($arg)*); } }

pub struct Editor {
    editor: parley::PlainEditor<Color>,

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
        let editor = parley::PlainEditor::new(1.);
        let mut self_ = Self { editor, font_size, text_color, window_scale, lineheight };
        self_.refresh().await;
        self_
    }

    pub fn init(&mut self) {}
    pub fn setup(&mut self) {}

    pub async fn refresh(&mut self) {
        let font_size = self.font_size.get();
        let text_color = self.text_color.get();
        let window_scale = self.window_scale.get();
        let lineheight = self.lineheight.get();

        self.editor.set_scale(window_scale);
        let mut styles = parley::StyleSet::new(font_size);
        styles.insert(parley::StyleProperty::LineHeight(lineheight));
        styles.insert(parley::StyleProperty::FontStack(parley::FontStack::List(FONT_STACK.into())));
        styles.insert(parley::StyleProperty::Brush(text_color));
        *self.editor.edit_styles() = styles;

        let mut txt_ctx = get_ctx().await;
        let (font_ctx, layout_ctx) = txt_ctx.borrow();
        self.editor.refresh_layout(font_ctx, layout_ctx);
    }

    pub fn layout(&self) -> &parley::Layout<Color> {
        self.editor.try_layout().unwrap()
    }

    pub fn move_to_pos(&self, pos: Point) {}

    pub fn get_cursor_pos(&self) -> Option<Point> {
        let lineheight = self.lineheight.get();
        let cursor_rect = self.editor.cursor_geometry(lineheight).unwrap();
        let cursor_pos = Point::new(cursor_rect.x0 as f32, cursor_rect.y0 as f32);
        Some(cursor_pos)
    }

    pub async fn driver<'a>(&'a mut self) -> Option<DriverWrapper<'a>> {
        let mut txt_ctx = get_ctx().await;
        // I'm one billion percent sure this is safe and don't want to waste time
        let (font_ctx, layout_ctx) = {
            let (f, l) = txt_ctx.borrow();
            let f: &'a mut parley::FontContext = unsafe { std::mem::transmute(f) };
            let l: &'a mut parley::LayoutContext<Color> = unsafe { std::mem::transmute(l) };
            (f, l)
        };
        let drv = self.editor.driver(font_ctx, layout_ctx);
        // Storing the MutexGuard together with its dependent value drv ensures we cannot
        // have a race condition and the lifetime rules are respected.
        let drv = DriverWrapper { txt_ctx, drv };
        Some(drv)
    }
}

pub struct DriverWrapper<'a> {
    txt_ctx: async_lock::MutexGuard<'static, TextContext>,
    drv: parley::PlainEditorDriver<'a, Color>,
}

impl<'a> std::ops::Deref for DriverWrapper<'a> {
    type Target = parley::PlainEditorDriver<'a, Color>;

    fn deref(&self) -> &Self::Target {
        &self.drv
    }
}

impl<'a> std::ops::DerefMut for DriverWrapper<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.drv
    }
}
