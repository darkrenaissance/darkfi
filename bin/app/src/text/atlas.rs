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

use miniquad::TextureFormat;

use crate::gfx::{DebugTag, ManagedTexturePtr, Rectangle, RenderApi, Renderer};

/// Prevents render artifacts from aliasing.
/// Even with aliasing turned off, some bleed still appears possibly
/// due to UV coord calcs. Adding a gap perfectly fixes this.
const ATLAS_GAP: usize = 2;

/*
/// Convenience wrapper fn. Use if rendering a single line of glyphs.
pub fn make_texture_atlas(renderer: &Renderer, glyphs: &Vec<Glyph>) -> RenderedAtlas {
    let mut atlas = Atlas::new(renderer);
    atlas.push(&glyphs);
    atlas.make()
}
*/

pub(super) type RunIdx = usize;
type GlyphKey = (swash::GlyphId, RunIdx);

/// Responsible for aggregating glyphs, and then producing a single software
/// blitted texture usable in a single draw call.
/// This makes OpenGL batch precomputation of meshes efficient.
///
/// ```rust
///     let mut atlas = Atlas::new(&renderer);
///     atlas.push_glyph(glyph, run_idx, &mut scaler);
///     let atlas = atlas.make().unwrap();
///     let uv = atlas.fetch_uv(glyph_id, run_idx).unwrap();
///     let atlas_texture_id = atlas.texture_id;
/// ```
pub struct Atlas<'a> {
    glyph_keys: Vec<GlyphKey>,
    sprites: Vec<swash::scale::image::Image>,
    // LHS x pos of glyph
    x_pos: Vec<usize>,

    width: usize,
    height: usize,

    renderer: &'a Renderer,
    tag: DebugTag,
}

impl<'a> Atlas<'a> {
    pub fn new(renderer: &'a Renderer, tag: DebugTag) -> Self {
        Self {
            glyph_keys: vec![],
            sprites: vec![],
            x_pos: vec![],

            width: ATLAS_GAP,
            // Not really important to set a value here since it will
            // get overwritten.
            // FYI glyphs have a gap on all sides (top and bottom here).
            height: 2 * ATLAS_GAP,

            renderer,
            tag,
        }
    }

    pub fn push_glyph(
        &mut self,
        glyph_id: swash::GlyphId,
        run_idx: RunIdx,
        scaler: &mut swash::scale::Scaler,
    ) {
        let glyph_key = (glyph_id, run_idx);
        if self.glyph_keys.contains(&glyph_key) {
            return
        }

        self.glyph_keys.push(glyph_key);

        let rendered_glyph = swash::scale::Render::new(
            // Select our source order
            &[
                swash::scale::Source::ColorOutline(0),
                swash::scale::Source::ColorBitmap(swash::scale::StrikeWith::BestFit),
                swash::scale::Source::Outline,
            ],
        )
        // Select the simple alpha (non-subpixel) format
        .format(zeno::Format::Alpha)
        .render(scaler, glyph_id)
        .unwrap();

        let glyph_width = rendered_glyph.placement.width as usize;
        let glyph_height = rendered_glyph.placement.height as usize;

        self.sprites.push(rendered_glyph);

        self.x_pos.push(self.width);

        // Gap on the top and bottom
        let height = ATLAS_GAP + glyph_height + ATLAS_GAP;
        self.height = std::cmp::max(height, self.height);

        // Gap between glyphs and on both sides
        self.width += glyph_width + ATLAS_GAP;
    }

    fn render(&self) -> Vec<u8> {
        let mut atlas = vec![255, 255, 255, 0].repeat(self.width * self.height);
        // For drawing debug lines we want a single white pixel.
        // This is very useful to have in our texture for debugging.
        atlas[0] = 255;
        atlas[1] = 255;
        atlas[2] = 255;
        atlas[3] = 255;

        let y = ATLAS_GAP;
        // Copy all the sprites to our atlas.
        // They should have ATLAS_GAP spacing on all sides to avoid bleeding.
        for (sprite, x) in self.sprites.iter().zip(self.x_pos.iter()) {
            copy_image(sprite, *x, y, &mut atlas, self.width);
        }

        atlas
    }

    fn compute_uvs(&self) -> Vec<Rectangle> {
        // UV coords are in the range [0, 1]
        let mut uvs = Vec::with_capacity(self.sprites.len());

        let (self_w, self_h) = (self.width as f32, self.height as f32);
        let y = ATLAS_GAP as f32;

        for (sprite, x) in self.sprites.iter().zip(self.x_pos.iter()) {
            let x = *x as f32;
            let sprite_w = sprite.placement.width as f32;
            let sprite_h = sprite.placement.height as f32;

            let uv = Rectangle {
                x: x / self_w,
                y: y / self_h,
                w: sprite_w / self_w,
                h: sprite_h / self_h,
            };
            uvs.push(uv);
        }

        uvs
    }

    /// Debug method
    #[allow(dead_code)]
    pub fn dump(&self, output_path: &str) {
        let atlas = self.render();
        let img = image::RgbaImage::from_raw(self.width as u32, self.height as u32, atlas).unwrap();
        img.save(output_path).unwrap();
    }

    /// Invalidate this atlas and produce the finalized result.
    /// Each glyph is given a sub-rect within the texture, accessible by calling
    /// `rendered_atlas.fetch_uv(my_glyph_id)`.
    /// The texture ID is a struct member: `rendered_atlas.texture_id`.
    pub fn make(self) -> RenderedAtlas {
        //if self.glyph_keys.is_empty() {
        //    return Err(Error::AtlasIsEmpty)
        //}

        assert_eq!(self.glyph_keys.len(), self.sprites.len());
        assert_eq!(self.glyph_keys.len(), self.x_pos.len());

        let atlas = self.render();
        let texture = self.renderer.new_texture(
            self.width as u16,
            self.height as u16,
            atlas,
            TextureFormat::RGBA8,
            self.tag,
        );

        let uv_rects = self.compute_uvs();
        let glyph_keys = self.glyph_keys;

        let mut infos = Vec::with_capacity(self.sprites.len());
        for (uv_rect, sprite) in uv_rects.into_iter().zip(self.sprites.into_iter()) {
            let is_color = match sprite.content {
                swash::scale::image::Content::Mask => false,
                swash::scale::image::Content::SubpixelMask => unimplemented!(),
                swash::scale::image::Content::Color => true,
            };
            infos.push(GlyphInfo { uv_rect, place: sprite.placement, is_color });
        }

        RenderedAtlas { glyph_keys, infos, texture }
    }
}

/// Copy a sprite to (x, y) position within the atlas texture.
/// Both image formats are RGBA flat vecs.
fn copy_image(
    sprite: &swash::scale::image::Image,
    x: usize,
    y: usize,
    atlas: &mut Vec<u8>,
    atlas_width: usize,
) {
    let sprite_width = sprite.placement.width as usize;
    let sprite_height = sprite.placement.height as usize;

    match sprite.content {
        swash::scale::image::Content::Mask => {
            let mut i = 0;
            for pixel_y in 0..sprite_height {
                for pixel_x in 0..sprite_width {
                    let src_alpha = sprite.data[i];

                    let dest_y = (y + pixel_y) * atlas_width;
                    let off_dest = 4 * (dest_y + pixel_x + x);

                    //atlas[off_dest] = 255;
                    //atlas[off_dest + 1] = 255;
                    //atlas[off_dest + 2] = 255;
                    atlas[off_dest + 3] = src_alpha;

                    i += 1;
                }
            }
        }
        swash::scale::image::Content::SubpixelMask => unimplemented!(),
        swash::scale::image::Content::Color => {
            let row_size = sprite_width * 4;
            for (pixel_y, row) in sprite.data.chunks_exact(row_size).enumerate() {
                for (pixel_x, pixel) in row.chunks_exact(4).enumerate() {
                    assert_eq!(pixel.len(), 4);
                    //let src_y = pixel_y * sprite_width;
                    //let off_src = 4 * (src_y + pixel_x);

                    let dest_y = (y + pixel_y) * atlas_width;
                    let off_dest = 4 * (dest_y + pixel_x + x);

                    atlas[off_dest] = pixel[0];
                    atlas[off_dest + 1] = pixel[1];
                    atlas[off_dest + 2] = pixel[2];
                    atlas[off_dest + 3] = pixel[3];
                }
            }
        }
    }
}

#[derive(Clone)]
pub struct GlyphInfo {
    /// UV rectangle within the texture.
    pub uv_rect: Rectangle,
    /// Placement of the sprite used to calc the rect
    pub place: zeno::Placement,
    pub is_color: bool,
}

/// Final result computed from `Atlas::make()`.
#[derive(Clone)]
pub struct RenderedAtlas {
    glyph_keys: Vec<GlyphKey>,
    infos: Vec<GlyphInfo>,
    /// Allocated atlas texture.
    pub texture: ManagedTexturePtr,
}

impl RenderedAtlas {
    /// Get UV coords for a glyph within the rendered atlas.
    pub fn fetch_uv(&self, glyph_id: swash::GlyphId, run_idx: RunIdx) -> Option<&GlyphInfo> {
        let glyphs_len = self.glyph_keys.len();
        assert_eq!(glyphs_len, self.infos.len());

        let glyph_key = (glyph_id, run_idx);
        for i in 0..glyphs_len {
            if self.glyph_keys[i] == glyph_key {
                return Some(&self.infos[i])
            }
        }
        None
    }
}
