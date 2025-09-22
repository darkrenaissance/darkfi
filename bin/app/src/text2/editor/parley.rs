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
    prop::{PropertyAtomicGuard, PropertyColor, PropertyFloat32, PropertyStr},
    text2::{TextContext, FONT_STACK, TEXT_CTX},
};

pub struct Editor {
    editor: parley::PlainEditor<Color>,

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
        let editor = parley::PlainEditor::new(1.);
        //let atxt = "A berry is a small, pulpy, and often edible fruit. Typically, berries are juicy, rounded, brightly colored, sweet, sour or tart, and do not have a stone or pit, although many pips or seeds may be present. Common examples of berries in the culinary sense are strawberries, raspberries, blueberries, blackberries, white currants, blackcurrants, and redcurrants. In Britain, soft fruit is a horticultural term for such fruits. The common usage of the term berry is different from the scientific or botanical definition of a berry, which refers to a fruit produced from the ovary of a single flower where the outer layer of the ovary wall develops into an edible fleshy portion (pericarp). The botanical definition includes many fruits that are not commonly known or referred to as berries, such as grapes, tomatoes, cucumbers, eggplants, bananas, and chili peppers.";
        //editor.set_text(atxt);
        Self { text, editor, font_size, text_color, window_scale, lineheight }
    }

    // These are android specific
    #[allow(dead_code)]
    pub fn init(&mut self) {}
    #[allow(dead_code)]
    pub fn setup(&mut self) {}
    pub fn focus(&self) {}
    pub fn unfocus(&self) {}

    pub async fn on_text_prop_changed(&mut self) {
        // Get modified text property
        let txt = self.text.get();
        // Update Parley text buffer
        self.editor.set_text(&txt);
        // Refresh our layout
        self.refresh().await;
    }
    pub async fn on_buffer_changed(&mut self, atom: &mut PropertyAtomicGuard) {
        self.text.set(atom, self.editor.raw_text());
    }

    pub async fn refresh(&mut self) {
        let font_size = self.font_size.get();
        let text_color = self.text_color.get();
        let window_scale = self.window_scale.get();
        let lineheight = self.lineheight.get();

        self.editor.set_scale(window_scale);
        let mut styles = parley::StyleSet::new(font_size);
        styles.insert(parley::StyleProperty::LineHeight(parley::LineHeight::FontSizeRelative(
            lineheight,
        )));
        styles.insert(parley::StyleProperty::FontStack(parley::FontStack::List(FONT_STACK.into())));
        styles.insert(parley::StyleProperty::Brush(text_color));
        styles.insert(parley::StyleProperty::OverflowWrap(parley::OverflowWrap::Anywhere));
        *self.editor.edit_styles() = styles;

        let mut txt_ctx = TEXT_CTX.get().await;
        let (font_ctx, layout_ctx) = txt_ctx.borrow();
        self.editor.refresh_layout(font_ctx, layout_ctx);
    }

    pub fn layout(&self) -> &parley::Layout<Color> {
        self.editor.try_layout().unwrap()
    }

    pub fn move_to_pos(&self, _pos: Point) {
        unimplemented!()
    }
    pub fn select_word_at_point(&self, _pos: Point) {
        unimplemented!()
    }

    pub fn get_cursor_pos(&self) -> Point {
        let cursor_rect = self.editor.cursor_geometry(0.).unwrap();
        let cursor_pos = Point::new(cursor_rect.x0 as f32, cursor_rect.y0 as f32);
        cursor_pos
    }

    pub async fn insert(&mut self, txt: &str, atom: &mut PropertyAtomicGuard) {
        let mut txt_ctx = TEXT_CTX.get().await;
        let (font_ctx, layout_ctx) = txt_ctx.borrow();
        let mut drv = self.editor.driver(font_ctx, layout_ctx);
        drv.insert_or_replace_selection(&txt);
        self.on_buffer_changed(atom).await;
    }

    pub fn driver<'a>(
        &'a mut self,
        txt_ctx: &'a mut TextContext,
    ) -> Option<parley::PlainEditorDriver<'a, Color>> {
        let (font_ctx, layout_ctx) = txt_ctx.borrow();
        Some(self.editor.driver(font_ctx, layout_ctx))
    }

    pub fn set_width(&mut self, w: f32) {
        self.editor.set_width(Some(w));
    }
    pub fn width(&self) -> f32 {
        self.layout().full_width()
    }
    pub fn height(&self) -> f32 {
        self.layout().height()
    }

    pub fn selected_text(&self) -> Option<String> {
        self.editor.selected_text().map(str::to_string)
    }
    pub fn selection(&self) -> parley::Selection {
        *self.editor.raw_selection()
    }
    pub fn set_selection(&self, _select_start: usize, _select_end: usize) {
        unimplemented!()
    }

    #[allow(dead_code)]
    pub fn buffer(&self) -> String {
        self.editor.raw_text().to_string()
    }
}
