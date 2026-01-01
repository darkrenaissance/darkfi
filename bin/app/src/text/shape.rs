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

use freetype as ft;
use harfbuzz_sys::{
    freetype::hb_ft_font_create_referenced, hb_buffer_add_utf8, hb_buffer_create,
    hb_buffer_destroy, hb_buffer_get_glyph_infos, hb_buffer_get_glyph_positions,
    hb_buffer_guess_segment_properties, hb_buffer_set_cluster_level, hb_buffer_set_content_type,
    hb_buffer_t, hb_font_destroy, hb_font_t, hb_glyph_info_t, hb_glyph_position_t, hb_shape,
    HB_BUFFER_CLUSTER_LEVEL_MONOTONE_GRAPHEMES, HB_BUFFER_CONTENT_TYPE_UNICODE,
};
use std::{iter::Peekable, os, vec::IntoIter};

type FreetypeFace = ft::Face<&'static [u8]>;

struct HarfBuzzInfo<'a> {
    info: &'a hb_glyph_info_t,
    pos: &'a hb_glyph_position_t,
}

struct HarfBuzzIter<'a> {
    hb_font: *mut hb_font_t,
    buf: *mut hb_buffer_t,
    infos_iter: std::slice::Iter<'a, hb_glyph_info_t>,
    pos_iter: std::slice::Iter<'a, hb_glyph_position_t>,
}

impl<'a> Iterator for HarfBuzzIter<'a> {
    type Item = HarfBuzzInfo<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        let info = self.infos_iter.next()?;
        let pos = self.pos_iter.next()?;
        Some(HarfBuzzInfo { info, pos })
    }
}

impl<'a> Drop for HarfBuzzIter<'a> {
    fn drop(&mut self) {
        unsafe {
            hb_buffer_destroy(self.buf);
            hb_font_destroy(self.hb_font);
        }
    }
}

pub(super) fn set_face_size(face: &mut FreetypeFace, size: f32) {
    if face.has_fixed_sizes() {
        //debug!(target: "text", "fixed sizes");
        // emojis required a fixed size
        //face.set_char_size(109 * 64, 0, 72, 72).unwrap();
        face.select_size(0).unwrap();
    } else {
        //debug!(target: "text", "set char size");
        face.set_char_size(size as isize * 64, 0, 96, 96).unwrap();
    }
}

fn harfbuzz_shape<'a>(face: &mut FreetypeFace, text: &str) -> HarfBuzzIter<'a> {
    let utf8_ptr = text.as_ptr() as *const _;
    // https://harfbuzz.github.io/a-simple-shaping-example.html
    let (hb_font, buf, glyph_infos, glyph_pos) = unsafe {
        let ft_face_ptr: freetype::freetype_sys::FT_Face = face.raw_mut();
        let hb_font = hb_ft_font_create_referenced(ft_face_ptr);
        let buf = hb_buffer_create();
        hb_buffer_set_content_type(buf, HB_BUFFER_CONTENT_TYPE_UNICODE);
        hb_buffer_set_cluster_level(buf, HB_BUFFER_CLUSTER_LEVEL_MONOTONE_GRAPHEMES);
        hb_buffer_add_utf8(
            buf,
            utf8_ptr,
            text.len() as os::raw::c_int,
            0 as os::raw::c_uint,
            text.len() as os::raw::c_int,
        );
        hb_buffer_guess_segment_properties(buf);
        hb_shape(hb_font, buf, std::ptr::null(), 0 as os::raw::c_uint);

        let mut length: u32 = 0;
        let glyph_infos = hb_buffer_get_glyph_infos(buf, &mut length as *mut u32);
        let glyph_infos: &[hb_glyph_info_t] =
            std::slice::from_raw_parts(glyph_infos as *const _, length as usize);

        let glyph_pos = hb_buffer_get_glyph_positions(buf, &mut length as *mut u32);
        let glyph_pos: &[hb_glyph_position_t] =
            std::slice::from_raw_parts(glyph_pos as *const _, length as usize);

        (hb_font, buf, glyph_infos, glyph_pos)
    };

    let infos_iter = glyph_infos.iter();
    let pos_iter = glyph_pos.iter();

    HarfBuzzIter { hb_font, buf, infos_iter, pos_iter }
}

pub(super) struct GlyphInfo {
    pub face_idx: usize,
    pub id: u32,
    pub cluster_start: usize,
    pub cluster_end: usize,
    pub x_offset: i32,
    pub y_offset: i32,
    pub x_advance: i32,
    pub y_advance: i32,
}

impl GlyphInfo {
    pub fn substr<'a>(&self, text: &'a str) -> &'a str {
        // RTL
        let start = std::cmp::min(self.cluster_start, self.cluster_end);
        let end = std::cmp::max(self.cluster_start, self.cluster_end);
        &text[start..end]
    }
}

struct ShapedGlyphs {
    glyphs: Vec<GlyphInfo>,
}

impl ShapedGlyphs {
    fn new(glyphs: Vec<GlyphInfo>) -> Self {
        Self { glyphs }
    }

    fn has_zero(&self) -> bool {
        self.glyphs.iter().any(|g| g.id == 0)
    }

    fn fill_zeros(&mut self, fallback: Vec<GlyphInfo>) {
        let mut primary_iter = std::mem::take(&mut self.glyphs).into_iter().peekable();
        let mut fallback_iter = fallback.into_iter().peekable();
        assert!(self.glyphs.is_empty());

        while let Some(primary_glyph) = primary_iter.next() {
            if primary_glyph.id != 0 {
                Self::consume(&mut fallback_iter, primary_glyph.cluster_start);
                self.glyphs.push(primary_glyph);
                continue
            }

            let mut fallbacks = Self::consume(&mut fallback_iter, primary_glyph.cluster_start);

            let Some(last_fallback) = fallbacks.last() else { continue };
            let cluster_end = last_fallback.cluster_end;

            self.glyphs.append(&mut fallbacks);

            Self::drop_replaced(&mut primary_iter, cluster_end);
        }
    }

    fn consume(iter: &mut Peekable<IntoIter<GlyphInfo>>, cluster_bound: usize) -> Vec<GlyphInfo> {
        let mut consumed = vec![];
        while let Some(glyph) = iter.peek() {
            if glyph.cluster_start > cluster_bound {
                break
            }

            let glyph = iter.next().unwrap();
            consumed.push(glyph);
        }
        consumed
    }
    fn drop_replaced(iter: &mut Peekable<IntoIter<GlyphInfo>>, cluster_end: usize) {
        while let Some(glyph) = iter.peek() {
            if glyph.cluster_start >= cluster_end {
                break
            }
            let _ = iter.next();
        }
    }
}

/*
fn print_glyphs(ctx: &str, glyphs: &Vec<GlyphInfo>, indent: usize) {
    let ws = " ".repeat(2 * indent);
    println!("{ws}{} ------------------", ctx);
    for (i, glyph) in glyphs.iter().enumerate() {
        println!(
            "{ws}{i}: {}/{} [{}, {}]",
            glyph.face_idx, glyph.id, glyph.cluster_start, glyph.cluster_end
        );
    }
    println!("{ws}---------------------");
}
*/

fn face_shape(face: &mut FreetypeFace, text: &str, face_idx: usize) -> Vec<GlyphInfo> {
    let mut glyphs: Vec<GlyphInfo> = vec![];
    for (i, hbinf) in harfbuzz_shape(face, text).enumerate() {
        let glyph_id = hbinf.info.codepoint as u32;
        // Index within this substr
        let cluster = hbinf.info.cluster as usize;
        //println!("  {i}: glyph_id = {glyph_id}, cluster = {cluster}");

        if i != 0 {
            glyphs.last_mut().unwrap().cluster_end = cluster;
        }

        glyphs.push(GlyphInfo {
            face_idx,
            id: glyph_id,
            cluster_start: cluster,
            cluster_end: 0,
            x_offset: hbinf.pos.x_offset,
            y_offset: hbinf.pos.y_offset,
            x_advance: hbinf.pos.x_advance,
            y_advance: hbinf.pos.y_advance,
        });
    }
    if let Some(last) = glyphs.last_mut() {
        last.cluster_end = text.len();
    }
    glyphs
}

/// Shape text using fallback fonts. We shape it using the primary font, then go down through
/// the list of fallbacks. For every zero we encounter, take the remaining text on that line
/// and try to shape it. Then replace that glyph + any others in the cluster with the new one.
/// [More info](https://zachbayl.in/blog/font_fallback_revery/)
pub(super) fn shape(faces: &mut Vec<FreetypeFace>, text: &str) -> Vec<GlyphInfo> {
    let glyphs = face_shape(&mut faces[0], text, 0);
    let mut shaped = ShapedGlyphs::new(glyphs);

    // Go down successively in our fallbacks
    for face_idx in 1..faces.len() {
        if !shaped.has_zero() {
            break
        }

        let glyphs = face_shape(&mut faces[face_idx], text, face_idx);
        shaped.fill_zeros(glyphs);
    }
    shaped.glyphs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load_faces() -> Vec<FreetypeFace> {
        let ftlib = freetype::Library::init().unwrap();

        let mut faces = vec![];
        let font_data = include_bytes!("../../ibm-plex-mono-regular.otf") as &[u8];
        let face = ftlib.new_memory_face2(font_data, 0).unwrap();
        faces.push(face);

        let font_data = include_bytes!("../../NotoColorEmoji.ttf") as &[u8];
        let face = ftlib.new_memory_face2(font_data, 0).unwrap();
        faces.push(face);

        //let font_data = include_bytes!("../noto-serif-cjk-jp-regular.otf") as &[u8];
        //let face = ftlib.new_memory_face2(font_data, 0).unwrap();
        //faces.push(face);

        faces
    }

    #[test]
    fn simple_shape_test() {
        let mut faces = load_faces();
        let text = "\u{01f3f3}\u{fe0f}\u{200d}\u{26a7}\u{fe0f}";
        let glyphs = shape(&mut faces, text);

        assert_eq!(glyphs.len(), 1);
        assert_eq!(glyphs[0].face_idx, 1);
        assert_eq!(glyphs[0].id, 1895);
        assert_eq!(glyphs[0].cluster_start, 0);
        assert_eq!(glyphs[0].cluster_end, 16);
    }

    #[test]
    fn simple_double_shape_test() {
        let mut faces = load_faces();
        let text =
            "\u{01f3f3}\u{fe0f}\u{200d}\u{26a7}\u{fe0f}\u{01f3f3}\u{fe0f}\u{200d}\u{26a7}\u{fe0f}";
        let glyphs = shape(&mut faces, text);

        assert_eq!(glyphs.len(), 2);
        assert_eq!(glyphs[0].face_idx, 1);
        assert_eq!(glyphs[0].id, 1895);
        assert_eq!(glyphs[0].cluster_start, 0);
        assert_eq!(glyphs[0].cluster_end, 16);
        assert_eq!(glyphs[1].face_idx, 1);
        assert_eq!(glyphs[1].id, 1895);
        assert_eq!(glyphs[1].cluster_start, 16);
        assert_eq!(glyphs[1].cluster_end, 32);
    }

    #[test]
    fn mixed_shape_test() {
        //let text = "日本語";
        //let text = "hel 日本語\u{01f3f3}\u{fe0f}\u{200d}\u{26a7}\u{fe0f} ally";

        let mut faces = load_faces();
        let text = "hel \u{01f3f3}\u{fe0f}\u{200d}\u{26a7}\u{fe0f} 123 X\u{01f44d}\u{01f3fe}X br";
        let glyphs = shape(&mut faces, text);

        assert_eq!(glyphs[0].face_idx, 0);
        assert_eq!(glyphs[0].id, 11);
        assert_eq!(glyphs[0].cluster_start, 0);
        assert_eq!(glyphs[0].cluster_end, 1);
        assert_eq!(glyphs[1].face_idx, 0);
        assert_eq!(glyphs[1].id, 6);
        assert_eq!(glyphs[1].cluster_start, 1);
        assert_eq!(glyphs[1].cluster_end, 2);
        assert_eq!(glyphs[1].cluster_start, glyphs[0].cluster_end);
        assert_eq!(glyphs[2].face_idx, 0);
        assert_eq!(glyphs[2].id, 15);
        assert_eq!(glyphs[2].cluster_start, 2);
        assert_eq!(glyphs[2].cluster_end, 3);
        assert_eq!(glyphs[2].cluster_start, glyphs[1].cluster_end);
        assert_eq!(glyphs[3].face_idx, 0);
        assert_eq!(glyphs[3].id, 1099);
        assert_eq!(glyphs[3].cluster_start, 3);
        assert_eq!(glyphs[3].cluster_end, 4);
        assert_eq!(glyphs[3].cluster_start, glyphs[2].cluster_end);
        assert_eq!(glyphs[4].face_idx, 1);
        assert_eq!(glyphs[4].id, 1895);
        assert_eq!(glyphs[4].cluster_start, 4);
        assert_eq!(glyphs[4].cluster_end, 20);
        assert_eq!(glyphs[4].cluster_start, glyphs[3].cluster_end);
        assert_eq!(glyphs[5].face_idx, 0);
        assert_eq!(glyphs[5].id, 1099);
        assert_eq!(glyphs[5].cluster_start, 20);
        assert_eq!(glyphs[5].cluster_end, 21);
        assert_eq!(glyphs[5].cluster_start, glyphs[4].cluster_end);
        assert_eq!(glyphs[6].face_idx, 0);
        assert_eq!(glyphs[6].id, 59);
        assert_eq!(glyphs[6].cluster_start, 21);
        assert_eq!(glyphs[6].cluster_end, 22);
        assert_eq!(glyphs[6].cluster_start, glyphs[5].cluster_end);
        assert_eq!(glyphs[7].face_idx, 0);
        assert_eq!(glyphs[7].id, 60);
        assert_eq!(glyphs[7].cluster_start, 22);
        assert_eq!(glyphs[7].cluster_end, 23);
        assert_eq!(glyphs[7].cluster_start, glyphs[6].cluster_end);
        assert_eq!(glyphs[8].face_idx, 0);
        assert_eq!(glyphs[8].id, 61);
        assert_eq!(glyphs[8].cluster_start, 23);
        assert_eq!(glyphs[8].cluster_end, 24);
        assert_eq!(glyphs[8].cluster_start, glyphs[7].cluster_end);
        assert_eq!(glyphs[9].face_idx, 0);
        assert_eq!(glyphs[9].id, 1099);
        assert_eq!(glyphs[9].cluster_start, 24);
        assert_eq!(glyphs[9].cluster_end, 25);
        assert_eq!(glyphs[9].cluster_start, glyphs[8].cluster_end);
        assert_eq!(glyphs[10].face_idx, 0);
        assert_eq!(glyphs[10].id, 53);
        assert_eq!(glyphs[10].cluster_start, 25);
        assert_eq!(glyphs[10].cluster_end, 26);
        assert_eq!(glyphs[10].cluster_start, glyphs[9].cluster_end);
        assert_eq!(glyphs[11].face_idx, 1);
        assert_eq!(glyphs[11].id, 1955);
        assert_eq!(glyphs[11].cluster_start, 26);
        assert_eq!(glyphs[11].cluster_end, 34);
        assert_eq!(glyphs[11].cluster_start, glyphs[10].cluster_end);
        assert_eq!(glyphs[12].face_idx, 0);
        assert_eq!(glyphs[12].id, 53);
        assert_eq!(glyphs[12].cluster_start, 34);
        assert_eq!(glyphs[12].cluster_end, 35);
        assert_eq!(glyphs[12].cluster_start, glyphs[11].cluster_end);
        assert_eq!(glyphs[13].face_idx, 0);
        assert_eq!(glyphs[13].id, 1099);
        assert_eq!(glyphs[13].cluster_start, 35);
        assert_eq!(glyphs[13].cluster_end, 36);
        assert_eq!(glyphs[13].cluster_start, glyphs[12].cluster_end);
        assert_eq!(glyphs[14].face_idx, 0);
        assert_eq!(glyphs[14].id, 3);
        assert_eq!(glyphs[14].cluster_start, 36);
        assert_eq!(glyphs[14].cluster_end, 37);
        assert_eq!(glyphs[14].cluster_start, glyphs[13].cluster_end);
        assert_eq!(glyphs[15].face_idx, 0);
        assert_eq!(glyphs[15].id, 21);
        assert_eq!(glyphs[15].cluster_start, 37);
        assert_eq!(glyphs[15].cluster_end, 38);
        assert_eq!(glyphs[15].cluster_start, glyphs[14].cluster_end);
    }

    #[test]
    fn hb_shape_custom_emoji() {
        let ftlib = ft::Library::init().unwrap();

        let font_data = include_bytes!("../../darkirc-emoji-svg.ttf") as &[u8];
        let mut face = ftlib.new_memory_face2(font_data, 0).unwrap();

        let text = "\u{f0001}";

        for (i, hbinf) in harfbuzz_shape(&mut face, text).enumerate() {
            let glyph_id = hbinf.info.codepoint as u32;
            // Index within this substr
            let cluster = hbinf.info.cluster as usize;
            //println!("  {i}: glyph_id = {glyph_id}, cluster = {cluster}");
        }
    }

    #[test]
    fn custom_emoji() {
        let ftlib = ft::Library::init().unwrap();

        let font_data = include_bytes!("../../darkirc-emoji-svg.ttf") as &[u8];
        let face = ftlib.new_memory_face2(font_data, 0).unwrap();

        let mut faces = vec![face];
        let text = "\u{f0001}";
        let glyphs = shape(&mut faces, text);
        //print_glyphs("", &glyphs);
    }

    /*
    #[test]
    fn weird_stuff() {
        let mut faces = load_faces();
        //let text = "( \u{361}° \u{35c}ʖ \u{361}°)";
        let text = "\u{35c}ʖ \u{361}a";
        let glyphs = shape(&mut faces, text);
    }
    */
}
