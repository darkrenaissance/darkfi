/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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
    hb_font_destroy, hb_glyph_info_t, hb_glyph_position_t, hb_shape,
    HB_BUFFER_CLUSTER_LEVEL_MONOTONE_CHARACTERS, HB_BUFFER_CONTENT_TYPE_UNICODE,
};
use std::{
    collections::HashMap,
    os,
    sync::{Arc, Mutex as SyncMutex, Weak},
};

use crate::gfx::Rectangle;

mod atlas;
pub use atlas::{make_texture_atlas, Atlas, RenderedAtlas};

//mod old_atlas;
//pub use old_atlas::{make_texture_atlas, RenderedAtlas};

mod wrap;
pub use wrap::{glyph_str, wrap};

// From https://sourceforge.net/projects/freetype/files/freetype2/2.6/
//
// * An `FT_Face' object can only be safely used from one thread at
//   a time.
//
// * An `FT_Library'  object can  now be used  without modification
//   from multiple threads at the same time.
//
// * `FT_Face' creation and destruction  with the same `FT_Library'
//   object can only be done from one thread at a time.
//
// One can use a single  `FT_Library' object across threads as long
// as a mutex lock is used around `FT_New_Face' and `FT_Done_Face'.
// Any calls to `FT_Load_Glyph' and similar API are safe and do not
// need the lock  to be held as  long as the same  `FT_Face' is not
// used from multiple threads at the same time.

// Harfbuzz is threadsafe.

// Notes:
// * All ft init and face creation should happen at startup.
// * FT faces protected behind a Mutex
// * Glyph cache. Key is (glyph_id, font_size)
// * Glyph texture cache: (glyph_id, font_size, color)

pub struct GlyphPositionIter<'a> {
    font_size: f32,
    window_scale: f32,
    glyphs: &'a Vec<Glyph>,
    current_x: f32,
    current_y: f32,
    i: usize,
}

impl<'a> GlyphPositionIter<'a> {
    pub fn new(font_size: f32, window_scale: f32, glyphs: &'a Vec<Glyph>, baseline_y: f32) -> Self {
        Self {
            font_size,
            window_scale,
            glyphs,
            current_x: 0.,
            current_y: baseline_y * window_scale,
            i: 0,
        }
    }
}

impl<'a> Iterator for GlyphPositionIter<'a> {
    type Item = Rectangle;

    fn next(&mut self) -> Option<Self::Item> {
        assert!(self.i <= self.glyphs.len());
        if self.i == self.glyphs.len() {
            return None;
        }

        let glyph = &self.glyphs[self.i];
        let sprite = &glyph.sprite;

        let rect = if sprite.has_fixed_sizes {
            // Downscale by height
            let w = (sprite.bmp_width as f32 * self.font_size) / sprite.bmp_height as f32;
            let h = self.font_size;

            let x = self.current_x;
            let y = self.current_y - h;

            self.current_x += w;

            Rectangle { x, y, w, h }
        } else {
            let (w, h) = (sprite.bmp_width as f32, sprite.bmp_height as f32);

            let off_x = glyph.x_offset as f32 / 64.;
            let off_y = glyph.y_offset as f32 / 64.;

            let x = self.current_x + off_x + sprite.bearing_x;
            let y = self.current_y - off_y - sprite.bearing_y;

            let x_advance = glyph.x_advance;
            let y_advance = glyph.y_advance;
            self.current_x += x_advance;
            self.current_y += y_advance;

            Rectangle { x, y, w, h }
        };

        let mut rect = rect / self.window_scale;

        self.i += 1;
        Some(rect)
    }
}

struct TextShaperInternal {
    font_faces: FtFaces,
    cache: TextShaperCache,
}

pub struct TextShaper {
    intern: SyncMutex<TextShaperInternal>,
}

impl TextShaper {
    pub fn new() -> Arc<Self> {
        let ftlib = ft::Library::init().unwrap();

        let mut faces = vec![];

        let font_data = include_bytes!("../../ibm-plex-mono-regular.otf") as &[u8];
        let ft_face = ftlib.new_memory_face2(font_data, 0).unwrap();
        faces.push(ft_face);

        let font_data = include_bytes!("../../NotoColorEmoji.ttf") as &[u8];
        let ft_face = ftlib.new_memory_face2(font_data, 0).unwrap();
        faces.push(ft_face);

        Arc::new(Self {
            intern: SyncMutex::new(TextShaperInternal {
                font_faces: FtFaces(faces),
                cache: HashMap::new(),
            }),
        })
    }

    fn split_into_substrs(font_faces: &Vec<FreetypeFace>, text: String) -> Vec<(usize, String)> {
        let mut current_idx = 0;
        let mut current_str = String::new();
        let mut substrs = vec![];
        'next_char: for chr in text.chars() {
            let idx = 'get_idx: {
                for i in 0..font_faces.len() {
                    let ft_face = &font_faces[i];
                    if ft_face.get_char_index(chr as usize).is_some() {
                        break 'get_idx i
                    }
                }

                //warn!(target: "text", "no font fallback for char: '{}'", chr);
                // Skip this char
                continue 'next_char
            };
            if current_idx != idx {
                if !current_str.is_empty() {
                    // Push
                    substrs.push((current_idx, current_str.clone()));
                }

                current_str.clear();
                current_idx = idx;
            }
            current_str.push(chr);
        }
        if !current_str.is_empty() {
            // Push
            substrs.push((current_idx, current_str));
        }
        substrs
    }

    pub fn shape(&self, text: String, font_size: f32, window_scale: f32) -> Vec<Glyph> {
        //debug!(target: "text", "shape('{}', {})", text, font_size);
        // Lock font faces
        // Freetype faces are not threadsafe
        let mut intern = self.intern.lock().unwrap();
        //let faces = &mut intern.font_faces;
        //let cache = &mut intern.cache;

        let substrs = Self::split_into_substrs(&intern.font_faces.0, text.clone());

        let mut glyphs: Vec<Glyph> = vec![];

        for (face_idx, text) in substrs {
            //debug!("substr {}", text);
            let face = &mut intern.font_faces.0[face_idx];
            if face.has_fixed_sizes() {
                // emojis required a fixed size
                //face.set_char_size(109 * 64, 0, 72, 72).unwrap();
                face.select_size(0).unwrap();
            } else {
                let size = font_size * window_scale;
                face.set_char_size(size as isize * 64, 0, 96, 96).unwrap();
            }

            /*
            let hb_font = harfbuzz_rs::Font::from_freetype_face(face.clone());
            let buffer = harfbuzz_rs::UnicodeBuffer::new()
                .set_cluster_level(harfbuzz_rs::ClusterLevel::MonotoneCharacters)
                .add_str(&text);
            let output = harfbuzz_rs::shape(&hb_font, buffer, &[]);

            let positions = output.get_glyph_positions();
            let infos = output.get_glyph_infos();
            */

            let utf8_ptr = text.as_ptr() as *const _;
            // https://harfbuzz.github.io/a-simple-shaping-example.html
            let (hb_font, buf, _glyph_infos, _glyph_pos, glyph_infos_iter, glyph_pos_iter) = unsafe {
                let ft_face_ptr: freetype::freetype_sys::FT_Face = face.raw_mut();
                let hb_font = hb_ft_font_create_referenced(ft_face_ptr);
                let buf = hb_buffer_create();
                hb_buffer_set_content_type(buf, HB_BUFFER_CONTENT_TYPE_UNICODE);
                hb_buffer_set_cluster_level(buf, HB_BUFFER_CLUSTER_LEVEL_MONOTONE_CHARACTERS);
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
                let glyph_infos_iter: &[hb_glyph_info_t] =
                    std::slice::from_raw_parts(glyph_infos as *const _, length as usize);

                let glyph_pos = hb_buffer_get_glyph_positions(buf, &mut length as *mut u32);
                let glyph_pos_iter: &[hb_glyph_position_t] =
                    std::slice::from_raw_parts(glyph_pos as *const _, length as usize);

                // Return glyph_(infos|pos) since iters depend on it
                (hb_font, buf, glyph_infos, glyph_pos, glyph_infos_iter, glyph_pos_iter)
            };

            let mut prev_cluster = 0;

            //for (i, (position, info)) in positions.iter().zip(infos).enumerate() {
            'iter_glyphs: for (i, (position, info)) in
                glyph_pos_iter.iter().zip(glyph_infos_iter.iter()).enumerate()
            {
                let face = &mut intern.font_faces.0[face_idx];

                let glyph_id = info.codepoint as u32;
                // Index within this substr
                let curr_cluster = info.cluster as usize;

                // Skip first time
                if i != 0 {
                    let substr = text[prev_cluster..curr_cluster].to_string();
                    glyphs.last_mut().unwrap().substr = substr;
                }

                prev_cluster = curr_cluster;

                let x_offset = position.x_offset as f32 / 64.;
                let y_offset = position.y_offset as f32 / 64.;
                let x_advance = position.x_advance as f32 / 64.;
                let y_advance = position.y_advance as f32 / 64.;

                // Check cache
                // If it exists in the cache then skip
                // Relevant info:
                // * glyph_id
                // * font_size (for non-fixed size faces)
                // * face_idx
                let cache_key = CacheKey {
                    glyph_id,
                    font_size: if face.has_fixed_sizes() {
                        FontSize::Fixed
                    } else {
                        FontSize::from((font_size, window_scale))
                    },
                    face_idx,
                };

                //debug!(target: "text", "cache_key: {:?}", cache_key);
                'load_sprite: {
                    if let Some(sprite) = intern.cache.get(&cache_key) {
                        let Some(sprite) = sprite.upgrade() else {
                            break 'load_sprite;
                        };

                        //debug!(target: "text", "found glyph!");
                        let glyph = Glyph {
                            glyph_id,
                            substr: String::new(),
                            sprite,
                            x_offset,
                            y_offset,
                            x_advance,
                            y_advance,
                        };

                        glyphs.push(glyph);
                        continue 'iter_glyphs;
                    }
                }

                let face = &mut intern.font_faces.0[face_idx];
                let mut flags = ft::face::LoadFlag::DEFAULT;
                if face.has_color() {
                    flags |= ft::face::LoadFlag::COLOR;
                }

                //debug!("load_glyph {}", glyph_id);
                if let Err(err) = face.load_glyph(glyph_id, flags) {
                    error!(target: "text", "error loading glyph {glyph_id}: {err}");
                    continue
                }
                //debug!("load_glyph {} [done]", glyph_id);

                // https://gist.github.com/jokertarot/7583938?permalink_comment_id=3327566#gistcomment-3327566

                let glyph = face.glyph();
                glyph.render_glyph(ft::RenderMode::Normal).unwrap();

                let bmp = glyph.bitmap();
                let buffer = bmp.buffer();
                let bmp_width = bmp.width() as usize;
                let bmp_height = bmp.rows() as usize;
                let bearing_x = glyph.bitmap_left() as f32;
                let bearing_y = glyph.bitmap_top() as f32;
                let has_fixed_sizes = face.has_fixed_sizes();

                let pixel_mode = bmp.pixel_mode().unwrap();
                let bmp = match pixel_mode {
                    ft::bitmap::PixelMode::Bgra => {
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
                    ft::bitmap::PixelMode::Gray => {
                        // Convert from greyscale to RGBA8
                        let tdata: Vec<_> = buffer
                            .iter()
                            .flat_map(|coverage| vec![255, 255, 255, *coverage])
                            .collect();
                        tdata
                    }
                    _ => panic!("unsupport pixel mode: {:?}", pixel_mode),
                };

                let sprite = Arc::new(Sprite {
                    bmp,
                    bmp_width,
                    bmp_height,
                    bearing_x,
                    bearing_y,
                    has_fixed_sizes,
                    has_color: face.has_color(),
                });

                intern.cache.insert(cache_key, Arc::downgrade(&sprite));

                let glyph = Glyph {
                    glyph_id,
                    substr: String::new(),
                    sprite,
                    x_offset,
                    y_offset,
                    x_advance,
                    y_advance,
                };

                //debug!(target: "text", "pushing glyph...");
                glyphs.push(glyph);
            }

            let substr = text[prev_cluster..].to_string();
            glyphs.last_mut().unwrap().substr = substr;

            unsafe {
                hb_buffer_destroy(buf);
                hb_font_destroy(hb_font);
            }
        }

        glyphs
    }
}

#[derive(Eq, Hash, PartialEq, Debug)]
enum FontSize {
    Fixed,
    Size((u32, u32)),
}

impl FontSize {
    /// You can't use f32 in Hash and Eq impls
    fn from(size: (f32, f32)) -> Self {
        let font_size = (size.0 * 1000.).round() as u32;
        let scale = (size.1 * 1000.).round() as u32;
        Self::Size((font_size, scale))
    }
}

#[derive(Eq, Hash, PartialEq, Debug)]
struct CacheKey {
    glyph_id: u32,
    font_size: FontSize,
    face_idx: usize,
}

pub type SpritePtr = Arc<Sprite>;

pub struct Sprite {
    bmp: Vec<u8>,
    pub bmp_width: usize,
    pub bmp_height: usize,

    pub bearing_x: f32,
    pub bearing_y: f32,
    pub has_fixed_sizes: bool,
    pub has_color: bool,
}

#[derive(Clone)]
pub struct Glyph {
    pub glyph_id: u32,
    // Substring this glyph corresponds to
    pub substr: String,

    pub sprite: SpritePtr,

    // Normally these are i32, we provide the conversions
    pub x_offset: f32,
    pub y_offset: f32,
    pub x_advance: f32,
    pub y_advance: f32,
}

impl std::fmt::Debug for Glyph {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Glyph")
            .field("glyph_id", &self.glyph_id)
            .field("substr", &self.substr)
            .finish()
    }
}

type FreetypeFace = ft::Face<&'static [u8]>;

struct FtFaces(Vec<FreetypeFace>);

unsafe impl Send for FtFaces {}
unsafe impl Sync for FtFaces {}

pub type TextShaperPtr = Arc<TextShaper>;

type TextShaperCache = HashMap<CacheKey, Weak<Sprite>>;
