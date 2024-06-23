use async_lock::Mutex;
use freetype as ft;
use miniquad::TextureId;
use std::{
    collections::HashMap,
    sync::{Arc, Weak},
};

use crate::{
    error::Result,
    gfx2::{Rectangle, RenderApiPtr},
    util::ansi_texture,
};

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
// * FT faces protected behind an async Mutex
// * Glyph cache. Key is (glyph_id, font_size)
// * Glyph texture cache: (glyph_id, font_size, color)

pub struct RenderedAtlas {
    pub uv_rects: Vec<Rectangle>,
    pub texture_id: TextureId,
}

const ATLAS_GAP: usize = 1;

pub async fn make_texture_atlas(
    render_api: RenderApiPtr,
    font_size: f32,
    glyphs: &Vec<Glyph>,
) -> Result<RenderedAtlas> {
    // First compute total size of the atlas
    let mut total_width = ATLAS_GAP;
    let mut total_height = ATLAS_GAP;

    // Glyph IDs already rendered so we don't do it twice
    let mut rendered = vec![];

    for (idx, glyph) in glyphs.iter().enumerate() {
        let sprite = &glyph.sprite;
        assert_eq!(sprite.bmp.len(), 4 * sprite.bmp_width * sprite.bmp_height);

        // Already done this one so skip
        if rendered.contains(&glyph.glyph_id) {
            continue
        }
        rendered.push(glyph.glyph_id);

        total_width += sprite.bmp_width + ATLAS_GAP;
        total_height = std::cmp::max(total_height, sprite.bmp_height);
    }
    total_width += ATLAS_GAP;
    total_height += 2 * ATLAS_GAP;

    // Allocate the big texture now
    let mut atlas_bmp = vec![0; 4 * total_width * total_height];
    // For debug lines we want a single white pixel.
    atlas_bmp[0] = 255;
    atlas_bmp[1] = 255;
    atlas_bmp[2] = 255;
    atlas_bmp[3] = 255;

    // Calculate dimensions of final product first
    let mut current_x = ATLAS_GAP;
    let mut rendered_glyphs: Vec<u32> = vec![];
    let mut uv_rects: Vec<Rectangle> = vec![];

    for (idx, glyph) in glyphs.iter().enumerate() {
        let sprite = &glyph.sprite;

        // Did we already rendered this glyph?
        // If so just copy the UV rect from before.
        let mut uv_rect = None;
        for (rendered_glyph_id, rendered_uv_rect) in rendered_glyphs.iter().zip(uv_rects.iter()) {
            if *rendered_glyph_id == glyph.glyph_id {
                uv_rect = Some(rendered_uv_rect.clone());
            }
        }

        let uv_rect = match uv_rect {
            Some(uv_rect) => uv_rect,
            // Allocating a new glyph sprite in the atlas
            None => {
                copy_image(sprite, &mut atlas_bmp, total_width, current_x);

                // Compute UV coords
                let uv_rect = Rectangle {
                    x: current_x as f32 / total_width as f32,
                    y: 0.,
                    w: sprite.bmp_width as f32 / total_width as f32,
                    h: sprite.bmp_height as f32 / total_height as f32,
                };

                current_x += sprite.bmp_width + ATLAS_GAP;

                uv_rect
            }
        };

        rendered_glyphs.push(glyph.glyph_id);
        uv_rects.push(uv_rect);
    }

    // Finally allocate the texture
    let texture_id =
        render_api.new_texture(total_width as u16, total_height as u16, atlas_bmp).await?;

    Ok(RenderedAtlas { uv_rects, texture_id })
}

fn copy_image(sprite: &Sprite, atlas_bmp: &mut Vec<u8>, total_width: usize, current_x: usize) {
    for i in 0..sprite.bmp_height {
        for j in 0..sprite.bmp_width {
            let off_dest = 4 * ((i + ATLAS_GAP) * total_width + j + current_x + ATLAS_GAP);
            let off_src = 4 * (i * sprite.bmp_width + j);
            atlas_bmp[off_dest] = sprite.bmp[off_src];
            atlas_bmp[off_dest + 1] = sprite.bmp[off_src + 1];
            atlas_bmp[off_dest + 2] = sprite.bmp[off_src + 2];
            atlas_bmp[off_dest + 3] = sprite.bmp[off_src + 3];
        }
    }
}

pub struct GlyphPositionIter<'a> {
    font_size: f32,
    glyphs: &'a Vec<Glyph>,
    current_x: f32,
    current_y: f32,
    i: usize,
}

impl<'a> GlyphPositionIter<'a> {
    pub fn new(font_size: f32, glyphs: &'a Vec<Glyph>, baseline_y: f32) -> Self {
        Self { font_size, glyphs, current_x: 0., current_y: baseline_y, i: 0 }
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

        self.i += 1;
        Some(rect)
    }
}

pub struct TextShaper {
    font_faces: Mutex<FtFaces>,
    cache: Mutex<TextShaperCache>,
}

impl TextShaper {
    pub fn new() -> Arc<Self> {
        let ftlib = ft::Library::init().unwrap();

        let mut faces = vec![];

        let font_data = include_bytes!("../ibm-plex-mono-light.otf") as &[u8];
        let ft_face = ftlib.new_memory_face2(font_data, 0).unwrap();
        faces.push(ft_face);

        let font_data = include_bytes!("../NotoColorEmoji.ttf") as &[u8];
        let ft_face = ftlib.new_memory_face2(font_data, 0).unwrap();
        faces.push(ft_face);

        Arc::new(Self { font_faces: Mutex::new(FtFaces(faces)), cache: Mutex::new(HashMap::new()) })
    }

    pub fn split_into_substrs(
        font_faces: &Vec<FreetypeFace>,
        text: String,
    ) -> Vec<(usize, String)> {
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
                drop(font_faces);

                warn!(target: "text", "no font fallback for char: '{}'", chr);
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

    /*
    pub async fn shape(&self, text: String, font_size: f32) -> Result<Vec<Glyph>> {
        let preglyphs = self.preshape(text, font_size).await;

        let mut glyphs = vec![];
        for preglyph in preglyphs {
            let texture = match preglyph.texture {
                PreGlyphTexture::Cached(texture) => texture,
                PreGlyphTexture::Uncached(raw_texture) => self.cache_texture(preglyph.glyph_id, font_size, preglyph.face_idx, raw_texture).await?,
            };

            let glyph = Glyph {
                glyph_id: preglyph.glyph_id,
                substr: preglyph.substr,
                texture,
                x_offset: preglyph.x_offset,
                y_offset: preglyph.y_offset,
                x_advance: preglyph.x_advance,
                y_advance: preglyph.y_advance,
            };

            glyphs.push(glyph);
        }
        Ok(glyphs)
    }
        */

    /*
    async fn cache_texture(&self, glyph_id: u32, font_size: f32, face_idx: usize, raw_texture: RawTextureData) -> Result<ManagedTexturePtr> {
        let cache_key = CacheKey {
            glyph_id,
            font_size: if raw_texture.has_fixed_sizes {
                FontSize::Fixed
            } else {
                FontSize::from(font_size)
            },
            face_idx,
        };

        // Not an issue to lock this intermittently preshape will request a glyph
        // to be cached. The worst that can happen is we double cache a glyph which
        // is no big deal.
        // Ofc we should benchmark this to see how well it performs in practice.
        let mut cache = self.cache.lock().await;

        // If we're shaping the a string like "anon1", then n will be cached twice
        // So lets check again if it exists first.
        if let Some(texture) = cache.get(&cache_key) {
            if let Some(texture) = texture.upgrade() {
                return Ok(texture)
            }
        }

        let texture_id = self
            .render_api
            .new_texture(
                raw_texture.bmp_width as u16,
                raw_texture.bmp_height as u16,
                raw_texture.bmp,
            )
            .await?;

        let texture = Arc::new(ManagedTexture {
            texture_id,
            render_api: self.render_api.clone(),
            bmp_width: raw_texture.bmp_width,
            bmp_height: raw_texture.bmp_height,
            bearing_x: raw_texture.bearing_x,
            bearing_y: raw_texture.bearing_y,
            has_fixed_sizes: raw_texture.has_fixed_sizes,
        });

        cache.insert(cache_key, Arc::downgrade(&texture));

        Ok(texture)
    }
    */

    pub async fn shape(&self, text: String, font_size: f32) -> Vec<Glyph> {
        // Lock font faces
        // Freetype faces are not threadsafe
        let faces = self.font_faces.lock().await;
        let mut cache = self.cache.lock().await;

        let substrs = Self::split_into_substrs(&faces.0, text.clone());

        let mut glyphs: Vec<Glyph> = vec![];

        let mut current_x = 0.;
        let mut current_y = 0.;

        for (face_idx, text) in substrs {
            //debug!("substr {}", text);
            let face = &faces.0[face_idx];
            if face.has_fixed_sizes() {
                // emojis required a fixed size
                //face.set_char_size(109 * 64, 0, 72, 72).unwrap();
                face.select_size(0).unwrap();
            } else {
                face.set_char_size(font_size as isize * 64, 0, 72, 72).unwrap();
            }

            let hb_font = harfbuzz_rs::Font::from_freetype_face(face.clone());
            let buffer = harfbuzz_rs::UnicodeBuffer::new()
                .set_cluster_level(harfbuzz_rs::ClusterLevel::MonotoneCharacters)
                .add_str(&text);
            let output = harfbuzz_rs::shape(&hb_font, buffer, &[]);

            let positions = output.get_glyph_positions();
            let infos = output.get_glyph_infos();

            let mut prev_cluster = 0;

            for (i, (position, info)) in positions.iter().zip(infos).enumerate() {
                let glyph_id = info.codepoint;
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
                        FontSize::from(font_size)
                    },
                    face_idx,
                };
                //debug!(target: "text", "cache_key: {:?}", cache_key);
                if let Some(sprite) = cache.get(&cache_key) {
                    let Some(sprite) = sprite.upgrade() else { break };

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
                    continue
                }

                let mut flags = ft::face::LoadFlag::DEFAULT;
                if face.has_color() {
                    flags |= ft::face::LoadFlag::COLOR;
                }

                // FIXME: glyph 884 hangs on android
                // For now just avoid using emojis on android
                //debug!("load_glyph {}", gid);
                face.load_glyph(glyph_id, flags).unwrap();
                //debug!("load_glyph {} [done]", gid);

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
                            .flat_map(|coverage| {
                                let r = 255;
                                let g = 255;
                                let b = 255;
                                let α = ((*coverage as f32) * 255.) as u8;
                                vec![r, g, b, α]
                            })
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
                });

                cache.insert(cache_key, Arc::downgrade(&sprite));

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
            }

            let substr = text[prev_cluster..].to_string();
            glyphs.last_mut().unwrap().substr = substr;
        }

        glyphs
    }
}

#[derive(Eq, Hash, PartialEq, Debug)]
enum FontSize {
    Fixed,
    Size(u32),
}

impl FontSize {
    /// You can't use f32 in Hash and Eq impls
    fn from(size: f32) -> Self {
        Self::Size((size * 1000.).round() as u32)
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
}

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

type FreetypeFace = ft::Face<&'static [u8]>;

struct FtFaces(Vec<FreetypeFace>);

unsafe impl Send for FtFaces {}
unsafe impl Sync for FtFaces {}

pub type TextShaperPtr = Arc<TextShaper>;

type TextShaperCache = HashMap<CacheKey, Weak<Sprite>>;
