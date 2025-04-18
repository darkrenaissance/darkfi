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
    text2::{TextContext, FONT_STACK, TEXT_CTX},
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

        let mut txt_ctx = TEXT_CTX.get().await;
        let (font_ctx, layout_ctx) = txt_ctx.borrow();
        self.editor.refresh_layout(font_ctx, layout_ctx);
    }

    pub fn layout(&self) -> &parley::Layout<Color> {
        self.editor.try_layout().unwrap()
    }

    pub fn move_to_pos(&self, pos: Point) {}

    pub fn get_cursor_pos(&self) -> Option<Point> {
        let cursor_rect = self.editor.cursor_geometry(0.).unwrap();
        let cursor_pos = Point::new(cursor_rect.x0 as f32, cursor_rect.y0 as f32);
        Some(cursor_pos)
    }

    pub async fn driver<'a>(
        &'a mut self,
        txt_ctx: &'a mut TextContext,
    ) -> Option<parley::PlainEditorDriver<'a, Color>> {
        let (font_ctx, layout_ctx) = txt_ctx.borrow();
        Some(self.editor.driver(font_ctx, layout_ctx))
    }

    pub fn set_width(&mut self, w: f32) {
        self.editor.set_width(Some(w));
    }
    pub fn height(&self) -> f32 {
        self.layout().height()
    }

    pub fn selected_text(&self) -> Option<&str> {
        self.editor.selected_text()
    }
}
