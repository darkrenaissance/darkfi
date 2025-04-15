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
    prop::{
        PropertyBool, PropertyColor, PropertyFloat32, PropertyPtr, PropertyRect, PropertyStr,
        PropertyUint32, Role,
    },
    text::{self, Glyph, GlyphPositionIter, TextShaperPtr},
    util::{enumerate_ref, is_whitespace},
};

pub type TextPos = usize;
pub type TextIdx = usize;

/// Android composing text from autosuggest.
/// We need this because IMEs can arbitrary set a composing region after
/// the text has been committed.
#[derive(Clone)]
struct ComposingText {
    /// Text that is being composed
    compose_text: String,
    /// Text that has been committed
    commit_text: String,

    region_start: usize,
    region_end: usize,
}

impl ComposingText {
    fn new() -> Self {
        Self {
            compose_text: String::new(),
            commit_text: String::new(),
            region_start: 0,
            region_end: 0,
        }
    }

    fn clear(&mut self) -> String {
        self.region_start = 0;
        self.region_end = 0;
        let final_text =
            std::mem::take(&mut self.commit_text) + &std::mem::take(&mut self.compose_text);
        final_text
    }

    /// Set composing text.
    fn compose(&mut self, text: String) {
        self.compose_text = text;

        self.region_start = self.commit_text.len();
        self.region_end = self.region_start + self.compose_text.len();
    }

    /// Commit the composing text.
    fn commit(&mut self) {
        self.commit_text += &self.compose_text;
        self.compose_text.clear();

        self.region_start = self.commit_text.len();
        self.region_end = self.commit_text.len();
    }

    /// Override the composing region for display.
    /// Anyone who looks closely at this impl might thing it's wrong that subsequent
    /// calls to compose() will ignore what's set here, but indeed this is how Android behaves.
    fn set_compose_region(&mut self, start: usize, end: usize) {
        assert!(start <= end);
        assert!(end <= self.commit_text.len() + self.compose_text.len());
        self.region_start = start;
        self.region_end = end;
    }
}

#[derive(Clone)]
pub struct RenderedEditable {
    pub glyphs: Vec<Glyph>,
    pub under_start: TextPos,
    pub under_end: TextPos,
}

impl RenderedEditable {
    fn new(glyphs: Vec<Glyph>, under_start: TextPos, under_end: TextPos) -> Self {
        let mut self_ = Self { glyphs, under_start: 0, under_end: 0 };
        self_.under_start = self_.idx_to_pos(under_start);
        self_.under_end = self_.idx_to_pos(under_end);
        self_
    }

    /// Which glyph contains the char at idx?
    fn idx_to_pos(&self, idx: TextIdx) -> TextPos {
        let mut total = 0;
        for (i, glyph) in enumerate_ref(&self.glyphs) {
            total += glyph.substr.len();
            if idx < total {
                return i
            }
        }
        return self.glyphs.len()
    }

    /// Converts glyph pos to idx in the string
    pub fn pos_to_idx(&self, pos: TextPos) -> TextIdx {
        let mut idx = 0;
        for (i, glyph) in enumerate_ref(&self.glyphs) {
            if i == pos {
                return idx
            }
            idx += glyph.substr.len();
        }
        return idx
    }

    pub fn has_underline(&self) -> bool {
        self.under_start != self.under_end
    }

    /// Converts x offset to glyph pos. Will round to the closest side.
    pub fn x_to_pos(&self, x: f32, font_size: f32, window_scale: f32, baseline: f32) -> TextPos {
        debug!(target: "ui::editbox", "x_to_pos({x})");
        let glyph_pos_iter =
            GlyphPositionIter::new(font_size, window_scale, &self.glyphs, baseline);
        for (glyph_idx, glyph_rect) in glyph_pos_iter.enumerate() {
            if x >= glyph_rect.rhs() {
                continue
            }

            let midpoint = glyph_rect.x + glyph_rect.w / 2.;
            if x < midpoint {
                return glyph_idx
            }
            assert!(x >= midpoint);
            return glyph_idx + 1
        }
        // Everything to the right is at the end
        self.glyphs.len()
    }

    pub fn pos_to_xw(
        &self,
        pos: TextPos,
        font_size: f32,
        window_scale: f32,
        baseline: f32,
    ) -> (f32, f32) {
        debug!(target: "ui::editbox", "pos_to_xw({pos}) [glyphs_len={}]", self.glyphs.len());
        let mut glyph_pos_iter =
            GlyphPositionIter::new(font_size, window_scale, &self.glyphs, baseline);
        let mut end = 0.;
        for (glyph_idx, glyph_rect) in glyph_pos_iter.enumerate() {
            if glyph_idx == pos {
                return (glyph_rect.x, glyph_rect.w)
            }
            end = glyph_rect.rhs();
        }
        (end, 0.)
    }
}

/// Represents a string with a cursor. The cursor can be moved in terms of glyphs, which does
/// not always correspond to chars in the string. We refer to byte indexes as idx, and glyph
/// indexes as pos.
///
/// The full string is: `before_text + commit_text + compose_text + after_text`.
/// The commit/compose text is the current text being composed which is needed for Android
/// autosuggest input.
///
/// Before and after text refers to the cursor. The cursor position is always everything
/// before `after_text`, that is: `before_text + commit_text + compose_text`.
///
/// We have to be careful when adjusting the cursor and other related ops like selecting text
/// to first render and move in terms of glyphs due to kerning. For example the chars "ae"
/// may be rendered as a single glyph in some fonts. Same for emojis represented by multiple
/// chars which are often not even a single byte.
pub struct Editable {
    text_shaper: TextShaperPtr,

    composer: ComposingText,

    before_text: String,
    after_text: String,

    font_size: PropertyFloat32,
    window_scale: PropertyFloat32,
    baseline: PropertyFloat32,
}

impl Editable {
    pub fn new(
        text_shaper: TextShaperPtr,

        font_size: PropertyFloat32,
        window_scale: PropertyFloat32,
        baseline: PropertyFloat32,
    ) -> Self {
        Self {
            text_shaper,
            composer: ComposingText::new(),
            before_text: String::new(),
            after_text: String::new(),
            font_size,
            window_scale,
            baseline,
        }
    }

    // reset composition
    // set text
    // find pos
    // compose
    // commit
    // set_compose_region
    // delete (forward, back)
    // set cursor

    /// Reset any composition in progress
    pub fn end_compose(&mut self) {
        //#[cfg(target_os = "android")]
        //crate::android::cancel_composition();

        //debug!(target: "ui::editbox", "end_compose() [editable={self:?}]");
        let final_text = self.composer.clear();
        self.before_text += &final_text;
    }

    pub fn get_text_before(&self) -> String {
        let text =
            self.before_text.clone() + &self.composer.commit_text + &self.composer.compose_text;
        text
    }
    pub fn get_text(&self) -> String {
        let text = self.get_text_before() + &self.after_text;
        text
    }

    pub fn set_text(&mut self, before: String, after: String) {
        self.before_text = before;
        self.after_text = after;
    }

    pub fn compose(&mut self, suggest_text: &str, is_commit: bool) {
        //composer.activate_or_cont(self.cursor_pos.get() as usize);
        self.composer.compose(suggest_text.to_string());
        if is_commit {
            self.composer.commit();
        }
    }
    /// Convenience function
    pub fn set_compose_region(&mut self, start: usize, end: usize) {
        self.composer.set_compose_region(start, end);
    }

    pub fn delete(&mut self, before: usize, after: usize) {
        self.end_compose();

        let mut chars = self.before_text.chars();
        // nightly feature, commenting it for now
        //chars.advance_back_by(before);
        self.before_text = chars.as_str().to_string();

        let mut chars = self.after_text.chars();
        // nightly feature, commenting it for now
        //chars.advance_by(after);
        self.after_text = chars.as_str().to_string();
    }

    /// Move the cursor. This offset should be computed from the glyphs.
    pub fn move_cursor(&mut self, dir: isize) {
        self.end_compose();

        let rendered = self.render();
        let mut cursor_pos = self.get_cursor_pos(&rendered);

        // Move the cursor pos
        if dir < 0 {
            assert!(-dir >= 0);
            let dir = -dir as usize;
            if cursor_pos > 0 {
                cursor_pos -= dir;
            }
        } else {
            assert!(dir >= 0);
            cursor_pos += dir as usize;
            let glyphs_len = rendered.glyphs.len();
            if cursor_pos > glyphs_len {
                cursor_pos = glyphs_len;
            }
        }

        // Convert cursor pos to string idx
        let idx = rendered.pos_to_idx(cursor_pos);
        self.set_cursor_idx(idx);
    }

    pub fn set_cursor_idx(&mut self, idx: TextPos) {
        // move_cursor() also calls this, but should be fine.
        self.end_compose();

        let mut text = self.get_text();
        let after_text = text.split_off(idx);
        self.before_text = text;
        self.after_text = after_text;
    }

    pub fn move_start(&mut self) {
        self.end_compose();
        self.after_text = self.get_text();
        self.before_text.clear();
    }
    pub fn move_end(&mut self) {
        self.end_compose();
        self.before_text = self.get_text();
        self.after_text.clear();
    }

    pub fn get_cursor_pos(&self, rendered: &RenderedEditable) -> TextPos {
        let cursor_off = self.get_text_before().len();
        let cursor_pos = rendered.idx_to_pos(cursor_off);
        cursor_pos
    }

    pub fn render(&self) -> RenderedEditable {
        let font_size = self.font_size.get();
        let window_scale = self.window_scale.get();

        let text = self.get_text();
        let glyphs = self.text_shaper.shape(text, font_size, window_scale);

        let compose_off = self.before_text.len();
        RenderedEditable::new(
            glyphs,
            compose_off + self.composer.region_start,
            compose_off + self.composer.region_end,
        )
    }
}

impl std::fmt::Debug for Editable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "({}, {}, {}, {})",
            self.before_text,
            self.composer.commit_text,
            self.composer.compose_text,
            self.after_text
        )
    }
}

fn glyphs_to_string(glyphs: &Vec<Glyph>) -> String {
    let mut text = String::new();
    for (i, glyph) in glyphs.iter().enumerate() {
        text.push_str(&glyph.substr);
    }
    text
}

#[derive(Clone)]
pub struct Selection {
    pub start: TextPos,
    pub end: TextPos,
}

impl Selection {
    pub fn new(start: TextPos, end: TextPos) -> Self {
        Self { start, end }
    }
}

impl std::fmt::Debug for Selection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}, {}]", self.start, self.end)
    }
}
