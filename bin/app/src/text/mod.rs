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

use std::{
    collections::HashMap,
    ffi::OsStr,
    path::PathBuf,
    sync::{Arc, Mutex as SyncMutex, Weak},
};

use crate::gfx::Rectangle;

mod atlas;
pub use atlas::{make_texture_atlas, Atlas, RenderedAtlas};
mod ft;
use ft::{render_glyph, FreetypeFace, Sprite, SpritePtr};
mod shape;
use shape::{set_face_size, shape};
mod wrap;
pub use wrap::wrap;

// Upscale emoji relative to font size
pub const EMOJI_SCALE_FACT: f32 = 1.6;
// How much of the emoji is above baseline?
pub const EMOJI_PROP_ABOVE_BASELINE: f32 = 0.8;

#[cfg(target_os = "android")]
fn custom_font_path() -> PathBuf {
    crate::android::get_external_storage_path().join("font")
}
#[cfg(not(target_os = "android"))]
fn custom_font_path() -> PathBuf {
    dirs::data_local_dir().unwrap().join("darkfi/app/font")
}

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

#[derive(Clone)]
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
        let start_y = baseline_y * window_scale;
        Self { font_size, window_scale, glyphs, current_x: 0., current_y: start_y, i: 0 }
    }
}

impl<'a> Iterator for GlyphPositionIter<'a> {
    type Item = Rectangle;

    fn next(&mut self) -> Option<Self::Item> {
        assert!(self.i <= self.glyphs.len());
        if self.i == self.glyphs.len() {
            return None
        }

        let glyph = &self.glyphs[self.i];
        let sprite = &glyph.sprite;

        // current_x/y is scaled real coords
        // but the returned rect is unscaled

        let rect = if sprite.has_fixed_sizes {
            // Downscale by height
            let w = (sprite.bmp_width as f32 * EMOJI_SCALE_FACT * self.font_size) /
                sprite.bmp_height as f32;
            let h = EMOJI_SCALE_FACT * self.font_size;

            let x = self.current_x / self.window_scale;
            let y = self.current_y / self.window_scale - (EMOJI_PROP_ABOVE_BASELINE * h);

            self.current_x += w * self.window_scale;

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

            // Downscale back again
            Rectangle { x, y, w, h } / self.window_scale
        };

        self.i += 1;
        Some(rect)
    }
}

struct TextShaperInternal {
    font_faces: FtFaces,
    cache: TextShaperCache,
}

impl TextShaperInternal {
    #[inline]
    fn faces<'a>(&'a mut self) -> &'a mut Vec<FreetypeFace> {
        &mut self.font_faces.0
    }
    #[inline]
    fn face<'a>(&'a mut self, idx: usize) -> &'a mut FreetypeFace {
        &mut self.font_faces.0[idx]
    }
}

pub struct TextShaper {
    intern: SyncMutex<TextShaperInternal>,
    _fonts_data: Vec<Vec<u8>>,
}

impl TextShaper {
    pub fn new() -> Arc<Self> {
        let ftlib = freetype::Library::init().unwrap();

        let mut fonts_data = vec![];
        if let Ok(read_dir) = std::fs::read_dir(custom_font_path()) {
            for entry in read_dir {
                let Ok(entry) = entry else {
                    warn!(target: "text", "Skipping unknown in custom font path");
                    continue
                };
                let font_path = entry.path();
                if font_path.is_dir() {
                    warn!(target: "text", "Skipping {font_path:?} in custom font path: is directory");
                    continue
                }
                let Some(font_ext) = font_path.extension().and_then(OsStr::to_str) else {
                    warn!(target: "text", "Skipping {font_path:?} in custom font path: missing file extension");
                    continue
                };
                if !["ttf", "otf"].contains(&font_ext) {
                    warn!(target: "text", "Skipping {font_path:?} in custom font path: unsupported file extension (supported: ttf, otf)");
                    continue
                }
                let font_data: Vec<u8> = match std::fs::read(&font_path) {
                    Ok(font_data) => font_data,
                    Err(err) => {
                        warn!(target: "text", "Unexpected error loading {font_path:?} in custom font path: {err}");
                        continue
                    }
                };
                info!(target: "text", "Loaded custom font: {font_path:?}");
                fonts_data.push(font_data);
            }
        }
        fonts_data.reserve_exact(fonts_data.len() + 2);

        let mut faces = vec![];

        let font_data = include_bytes!("../../ibm-plex-mono-regular.otf") as &[u8];
        let ft_face = ftlib.new_memory_face2(font_data, 0).unwrap();
        faces.push(ft_face);

        for font_data in &fonts_data {
            let face = unsafe { Self::load_font_face(&ftlib, font_data) };
            faces.push(face);
        }

        let font_data = include_bytes!("../../NotoColorEmoji.ttf") as &[u8];
        let ft_face = ftlib.new_memory_face2(font_data, 0).unwrap();
        faces.push(ft_face);

        Arc::new(Self {
            intern: SyncMutex::new(TextShaperInternal {
                font_faces: FtFaces(faces),
                cache: HashMap::new(),
            }),
            _fonts_data: fonts_data,
        })
    }

    /// Beware: recasts font_data as static. Make sure data outlives the face.
    unsafe fn load_font_face(ftlib: &freetype::Library, font_data: &[u8]) -> FreetypeFace {
        let font_data = &*(font_data as *const _);
        let ft_face = ftlib.new_memory_face2(font_data, 0).unwrap();
        ft_face
    }

    pub fn shape(&self, text: String, font_size: f32, window_scale: f32) -> Vec<Glyph> {
        //debug!(target: "text", "shape('{}', {})", text, font_size);
        if text.is_empty() {
            return vec![]
        }

        let text = &text;

        // Freetype faces are not threadsafe
        let mut intern = self.intern.lock().unwrap();

        let size = font_size * window_scale;
        for face in intern.faces() {
            set_face_size(face, size);
        }

        let mut glyphs: Vec<Glyph> = vec![];
        'next_glyph: for glyph_info in shape(intern.faces(), text) {
            let face_idx = glyph_info.face_idx;
            let face = intern.face(face_idx);
            let glyph_id = glyph_info.id;
            let substr = glyph_info.substr(text).to_string();

            let x_offset = glyph_info.x_offset as f32 / 64.;
            let y_offset = glyph_info.y_offset as f32 / 64.;
            let x_advance = glyph_info.x_advance as f32 / 64.;
            let y_advance = glyph_info.y_advance as f32 / 64.;

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
                        substr,
                        sprite,
                        x_offset,
                        y_offset,
                        x_advance,
                        y_advance,
                    };

                    glyphs.push(glyph);
                    continue 'next_glyph;
                }
            }

            let face = intern.face(face_idx);
            let Some(sprite) = render_glyph(&face, glyph_id) else { continue };

            let sprite = Arc::new(sprite);
            intern.cache.insert(cache_key, Arc::downgrade(&sprite));

            let glyph =
                Glyph { glyph_id, substr, sprite, x_offset, y_offset, x_advance, y_advance };

            //debug!(target: "text", "pushing glyph...");
            glyphs.push(glyph);
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

struct FtFaces(Vec<FreetypeFace>);

unsafe impl Send for FtFaces {}
unsafe impl Sync for FtFaces {}

pub type TextShaperPtr = Arc<TextShaper>;

type TextShaperCache = HashMap<CacheKey, Weak<Sprite>>;
