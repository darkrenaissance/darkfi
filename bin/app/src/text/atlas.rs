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

/// Conservative upper bound for atlas texture dimensions. Row wrapping
/// is capped to this and capped atlases assert their final size against
/// it so oversized builds fail loudly at build time instead of rendering
/// garbage at draw time.
pub const MAX_TEXTURE_DIMENSION: usize = 4096;

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

/// Pure packing state for the atlas: decides where each sprite goes and
/// how large the texture must be. Sprites are packed left to right into
/// rows no wider than `max_row_width`, wrapping to a new row as needed.
/// Every sprite keeps an `ATLAS_GAP` margin on all sides to avoid
/// bleeding. `usize::MAX` keeps the original single-row strip behavior.
struct AtlasLayout {
    /// (x, y) position of each sprite
    positions: Vec<(usize, usize)>,
    /// Height of the row currently being packed
    row_height: usize,
    /// LHS x pos for the next sprite
    x: usize,
    /// Top y pos of the row currently being packed
    y: usize,

    width: usize,
    height: usize,

    /// Row width cap. GPUs have a hard limit on texture dimensions
    /// (`GL_MAX_TEXTURE_SIZE`, only guaranteed 2048 on GLES3/WebGL2,
    /// typically 16384 on desktop), so a single-row strip cannot hold
    /// large icon sets (e.g. ~574 emoji at 120px would be ~70,000px
    /// wide). Sprites wrap into rows so the atlas grows as a roughly
    /// square texture that fits within GPU limits. `usize::MAX` keeps
    /// the original single-row strip behavior.
    max_row_width: usize,
}

impl AtlasLayout {
    fn new(max_row_width: usize) -> Self {
        Self {
            positions: vec![],
            row_height: 0,
            x: ATLAS_GAP,
            y: ATLAS_GAP,

            width: ATLAS_GAP,
            // Not really important to set a value here since it will
            // get overwritten.
            // FYI glyphs have a gap on all sides (top and bottom here).
            height: 2 * ATLAS_GAP,

            max_row_width,
        }
    }

    fn push(&mut self, glyph_width: usize, glyph_height: usize) {
        let row_end = self.x + glyph_width + ATLAS_GAP;
        if self.x > ATLAS_GAP && row_end > self.max_row_width {
            // Wrap to a new row, leaving a gap below the previous one
            self.y += self.row_height + ATLAS_GAP;
            self.x = ATLAS_GAP;
            self.row_height = 0;
        }

        self.positions.push((self.x, self.y));

        self.row_height = std::cmp::max(glyph_height, self.row_height);
        self.x += glyph_width + ATLAS_GAP;
        self.width = std::cmp::max(self.width, self.x);

        let height = self.y + glyph_height + ATLAS_GAP;
        self.height = std::cmp::max(height, self.height);
    }

    /// UV rect for sprite `i` in the range [0, 1]
    fn uv_rect(&self, i: usize, sprite_w: usize, sprite_h: usize) -> Rectangle {
        let (x, y) = self.positions[i];
        let (self_w, self_h) = (self.width as f32, self.height as f32);
        Rectangle {
            x: x as f32 / self_w,
            y: y as f32 / self_h,
            w: sprite_w as f32 / self_w,
            h: sprite_h as f32 / self_h,
        }
    }
}

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
    layout: AtlasLayout,

    renderer: &'a Renderer,
    tag: DebugTag,
}

impl<'a> Atlas<'a> {
    pub fn new(renderer: &'a Renderer, tag: DebugTag) -> Self {
        Self::with_max_row_width(renderer, tag, usize::MAX)
    }

    /// Create an atlas whose sprites wrap into rows no wider than
    /// `max_row_width`, keeping the texture within GPU limits.
    pub fn with_max_row_width(renderer: &'a Renderer, tag: DebugTag, max_row_width: usize) -> Self {
        Self {
            glyph_keys: vec![],
            sprites: vec![],
            layout: AtlasLayout::new(max_row_width),

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

        self.layout.push(glyph_width, glyph_height);
    }

    fn render(&self) -> Vec<u8> {
        let mut atlas = vec![255, 255, 255, 0].repeat(self.layout.width * self.layout.height);
        // For drawing debug lines we want a single white pixel.
        // This is very useful to have in our texture for debugging.
        atlas[0] = 255;
        atlas[1] = 255;
        atlas[2] = 255;
        atlas[3] = 255;

        // Copy all the sprites to our atlas.
        // They should have ATLAS_GAP spacing on all sides to avoid bleeding.
        for (sprite, (x, y)) in self.sprites.iter().zip(self.layout.positions.iter()) {
            copy_image(sprite, *x, *y, &mut atlas, self.layout.width);
        }

        atlas
    }

    fn compute_uvs(&self) -> Vec<Rectangle> {
        // UV coords are in the range [0, 1]
        let mut uvs = Vec::with_capacity(self.sprites.len());

        for (i, sprite) in self.sprites.iter().enumerate() {
            let uv_rect = self.layout.uv_rect(
                i,
                sprite.placement.width as usize,
                sprite.placement.height as usize,
            );
            uvs.push(uv_rect);
        }

        uvs
    }

    /// Debug method
    #[allow(dead_code)]
    pub fn dump(&self, output_path: &str) {
        let atlas = self.render();
        let img =
            image::RgbaImage::from_raw(self.layout.width as u32, self.layout.height as u32, atlas)
                .unwrap();
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
        assert_eq!(self.glyph_keys.len(), self.layout.positions.len());

        if self.layout.max_row_width != usize::MAX {
            assert!(
                self.layout.width <= MAX_TEXTURE_DIMENSION,
                "atlas width {} exceeds MAX_TEXTURE_DIMENSION {}",
                self.layout.width,
                MAX_TEXTURE_DIMENSION
            );
            assert!(
                self.layout.height <= MAX_TEXTURE_DIMENSION,
                "atlas height {} exceeds MAX_TEXTURE_DIMENSION {}",
                self.layout.height,
                MAX_TEXTURE_DIMENSION
            );
        }

        let atlas = self.render();
        let texture = self.renderer.new_texture(
            self.layout.width as u16,
            self.layout.height as u16,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_row_without_wrap() {
        let mut layout = AtlasLayout::new(usize::MAX);
        layout.push(10, 20);
        layout.push(5, 8);

        assert_eq!(layout.positions[0], (ATLAS_GAP, ATLAS_GAP));
        assert_eq!(layout.positions[1], (ATLAS_GAP + 12, ATLAS_GAP));
        assert_eq!(layout.width, ATLAS_GAP + 12 + 7);
        assert_eq!(layout.height, ATLAS_GAP + 20 + ATLAS_GAP);
    }

    #[test]
    fn test_sprites_wrap_into_rows() {
        let mut layout = AtlasLayout::new(50);
        for _ in 0..5 {
            layout.push(10, 10);
        }

        // Each sprite consumes 10 + ATLAS_GAP = 12 units of row width
        // starting at ATLAS_GAP, so four fit in a 50-wide row and the
        // fifth wraps onto a new one.
        assert_eq!(layout.positions.len(), 5);
        for i in 0..4 {
            assert_eq!(layout.positions[i].1, ATLAS_GAP);
            assert!(layout.positions[i].0 + 10 + ATLAS_GAP <= 50);
        }
        assert_eq!(layout.positions[4], (ATLAS_GAP, ATLAS_GAP + 10 + ATLAS_GAP));
        assert_eq!(layout.width, 50);
        assert_eq!(layout.height, layout.positions[4].1 + 10 + ATLAS_GAP);
    }

    #[test]
    fn test_uv_rects_match_placements() {
        let mut layout = AtlasLayout::new(50);
        for _ in 0..5 {
            layout.push(10, 10);
        }

        for i in 0..layout.positions.len() {
            let uv = layout.uv_rect(i, 10, 10);
            let (x, y) = layout.positions[i];

            assert!((0. ..=1.).contains(&uv.x));
            assert!((0. ..=1.).contains(&uv.y));
            assert!((0. ..=1.).contains(&(uv.x + uv.w)));
            assert!((0. ..=1.).contains(&(uv.y + uv.h)));
            assert_eq!(uv.x, x as f32 / layout.width as f32);
            assert_eq!(uv.y, y as f32 / layout.height as f32);
            assert_eq!(uv.w, 10. / layout.width as f32);
            assert_eq!(uv.h, 10. / layout.height as f32);
        }
    }
}
