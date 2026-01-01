/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free sofreetypeware: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Sofreetypeware Foundation, either version 3 of the
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

use freetype::face::LoadFlag as FtLoadFlag;
use std::sync::Arc;

pub type FreetypeFace = freetype::Face<&'static [u8]>;

pub type SpritePtr = Arc<Sprite>;

pub struct Sprite {
    pub bmp: Vec<u8>,
    pub bmp_width: usize,
    pub bmp_height: usize,

    pub bearing_x: f32,
    pub bearing_y: f32,
    pub has_fixed_sizes: bool,
    pub has_color: bool,
}

fn load_ft_glyph<'a>(
    face: &'a FreetypeFace,
    glyph_id: u32,
    flags: FtLoadFlag,
) -> Option<&'a freetype::GlyphSlot> {
    //debug!("load_glyph {} flags={flags:?}", glyph_id);
    if let Err(err) = face.load_glyph(glyph_id, flags) {
        error!(target: "text", "error loading glyph {glyph_id}: {err}");
        return None
    }
    //debug!("load_glyph {} [done]", glyph_id);

    // https://gist.github.com/jokertarot/7583938?permalink_comment_id=3327566#gistcomment-3327566

    let glyph = face.glyph();
    glyph.render_glyph(freetype::RenderMode::Normal).ok()?;
    Some(glyph)
}

pub fn render_glyph(face: &FreetypeFace, glyph_id: u32) -> Option<Sprite> {
    // If color is available then attempt to load it.
    // Otherwise fallback to black and white.
    let glyph = if face.has_color() {
        match load_ft_glyph(face, glyph_id, FtLoadFlag::DEFAULT | FtLoadFlag::COLOR) {
            Some(glyph) => glyph,
            None => load_ft_glyph(face, glyph_id, FtLoadFlag::DEFAULT)?,
        }
    } else {
        load_ft_glyph(face, glyph_id, FtLoadFlag::DEFAULT)?
    };

    let bmp = glyph.bitmap();
    let buffer = bmp.buffer();
    let bmp_width = bmp.width() as usize;
    let bmp_height = bmp.rows() as usize;
    let bearing_x = glyph.bitmap_left() as f32;
    let bearing_y = glyph.bitmap_top() as f32;
    let has_fixed_sizes = face.has_fixed_sizes();

    let pixel_mode = bmp.pixel_mode().unwrap();
    let bmp = match pixel_mode {
        freetype::bitmap::PixelMode::Bgra => {
            let mut tdata = vec![];
            tdata.resize(4 * bmp_width * bmp_height, 0);
            // Convert from BGRA to RGBA
            for i in 0..bmp_width * bmp_height {
                let idx = i * 4;
                let b = buffer[idx];
                let g = buffer[idx + 1];
                let r = buffer[idx + 2];
                let a = buffer[idx + 3];
                tdata[idx] = r;
                tdata[idx + 1] = g;
                tdata[idx + 2] = b;
                tdata[idx + 3] = a;
            }
            tdata
        }
        freetype::bitmap::PixelMode::Gray => {
            // Convert from greyscale to RGBA8
            let tdata: Vec<_> =
                buffer.iter().flat_map(|coverage| vec![255, 255, 255, *coverage]).collect();
            tdata
        }
        freetype::bitmap::PixelMode::Mono => {
            // Convert from mono to RGBA8
            let tdata: Vec<_> =
                buffer.iter().flat_map(|coverage| vec![255, 255, 255, *coverage]).collect();
            tdata
        }
        _ => panic!("unsupport pixel mode: {:?}", pixel_mode),
    };

    Some(Sprite {
        bmp,
        bmp_width,
        bmp_height,
        bearing_x,
        bearing_y,
        has_fixed_sizes,
        has_color: face.has_color(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_simple() {
        let ftlib = freetype::Library::init().unwrap();
        let font_data = include_bytes!("../../ibm-plex-mono-regular.otf") as &[u8];
        let face = ftlib.new_memory_face2(font_data, 0).unwrap();

        // glyph 11 in IBM plex mono regular is 'h'
        let glyph = render_glyph(&face, 11).unwrap();
    }

    #[test]
    fn render_custom_glyph() {
        let ftlib = freetype::Library::init().unwrap();
        let font_data = include_bytes!("../../darkirc-emoji-svg.ttf") as &[u8];
        let face = ftlib.new_memory_face2(font_data, 0).unwrap();

        let glyph = render_glyph(&face, 4).unwrap();
    }
}
