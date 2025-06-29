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
    gfx::{DebugTag, ManagedTexturePtr, Rectangle, RenderApi},
};

use super::{
    ft::{Sprite, SpritePtr},
    Glyph,
};

/// Prevents render artifacts from aliasing.
/// Even with aliasing turned off, some bleed still appears possibly
/// due to UV coord calcs. Adding a gap perfectly fixes this.
const ATLAS_GAP: usize = 2;

/// Convenience wrapper fn. Use if rendering a single line of glyphs.
pub fn make_texture_atlas(
    render_api: &RenderApi,
    tag: DebugTag,
    glyphs: &Vec<Glyph>,
) -> RenderedAtlas {
    let mut atlas = Atlas::new(render_api, tag);
    atlas.push(&glyphs);
    atlas.make()
}

/// Responsible for aggregating glyphs, and then producing a single software
/// blitted texture usable in a single draw call.
/// This makes OpenGL batch precomputation of meshes efficient.
///
/// ```rust
///     let mut atlas = Atlas::new(&render_api);
///     atlas.push(&glyphs);    // repeat as needed for shaped lines
///     let atlas = atlas.make().unwrap();
///     let uv = atlas.fetch_uv(glyph_id).unwrap();
///     let atlas_texture_id = atlas.texture_id;
/// ```
#[derive(Clone)]
pub struct Atlas<'a> {
    glyph_ids: Vec<u32>,
    sprites: Vec<SpritePtr>,
    // LHS x pos of glyph
    x_pos: Vec<usize>,

    width: usize,
    height: usize,

    render_api: &'a RenderApi,
    tag: DebugTag,
}

impl<'a> Atlas<'a> {
    pub fn new(render_api: &'a RenderApi, tag: DebugTag) -> Self {
        Self {
            glyph_ids: vec![],
            sprites: vec![],
            x_pos: vec![],
            width: ATLAS_GAP,
            // Not really important to set a value here since it will
            // get overwritten.
            // FYI glyphs have a gap on all sides (top and bottom here).
            height: 2 * ATLAS_GAP,
            render_api,
            tag,
        }
    }

    fn push_glyph(&mut self, glyph: &Glyph) {
        if self.glyph_ids.contains(&glyph.glyph_id) {
            return
        }

        self.glyph_ids.push(glyph.glyph_id);
        self.sprites.push(glyph.sprite.clone());

        let sprite = &glyph.sprite;
        self.x_pos.push(self.width);

        // Gap on the top and bottom
        let height = ATLAS_GAP + sprite.bmp_height + ATLAS_GAP;
        self.height = std::cmp::max(height, self.height);

        // Gap between glyphs and on both sides
        self.width += sprite.bmp_width + ATLAS_GAP;
    }

    /// Push a line of shaped text represented as `Vec<Glyph>`
    /// to this atlas.
    pub fn push(&mut self, glyphs: &Vec<Glyph>) {
        for glyph in glyphs {
            self.push_glyph(glyph);
        }
    }

    fn render(&self) -> Vec<u8> {
        let mut atlas = vec![0; 4 * self.width * self.height];
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
        let mut uvs = vec![];

        let (self_w, self_h) = (self.width as f32, self.height as f32);
        let y = ATLAS_GAP as f32;

        for (sprite, x) in self.sprites.iter().zip(self.x_pos.iter()) {
            let x = *x as f32;
            let sprite_w = sprite.bmp_width as f32;
            let sprite_h = sprite.bmp_height as f32;

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

    /// Invalidate this atlas and produce the finalized result.
    /// Each glyph is given a sub-rect within the texture, accessible by calling
    /// `rendered_atlas.fetch_uv(my_glyph_id)`.
    /// The texture ID is a struct member: `rendered_atlas.texture_id`.
    pub fn make(self) -> RenderedAtlas {
        //if self.glyph_ids.is_empty() {
        //    return Err(Error::AtlasIsEmpty)
        //}

        assert_eq!(self.glyph_ids.len(), self.sprites.len());
        assert_eq!(self.glyph_ids.len(), self.x_pos.len());

        let atlas = self.render();
        let texture =
            self.render_api.new_texture(self.width as u16, self.height as u16, atlas, self.tag);

        let uv_rects = self.compute_uvs();
        let glyph_ids = self.glyph_ids;

        RenderedAtlas { glyph_ids, uv_rects, texture }
    }
}

/// Copy a sprite to (x, y) position within the atlas texture.
/// Both image formats are RGBA flat vecs.
fn copy_image(sprite: &Sprite, x: usize, y: usize, atlas: &mut Vec<u8>, atlas_width: usize) {
    for i in 0..sprite.bmp_height {
        for j in 0..sprite.bmp_width {
            let src_y = i * sprite.bmp_width;
            let off_src = 4 * (src_y + j);

            let dest_y = (y + i) * atlas_width;
            let off_dest = 4 * (dest_y + j + x);

            atlas[off_dest] = sprite.bmp[off_src];
            atlas[off_dest + 1] = sprite.bmp[off_src + 1];
            atlas[off_dest + 2] = sprite.bmp[off_src + 2];
            atlas[off_dest + 3] = sprite.bmp[off_src + 3];
        }
    }
}

/// Final result computed from `Atlas::make()`.
#[derive(Clone)]
pub struct RenderedAtlas {
    glyph_ids: Vec<u32>,
    /// UV rectangle within the texture.
    uv_rects: Vec<Rectangle>,
    /// Allocated atlas texture.
    pub texture: ManagedTexturePtr,
}

impl RenderedAtlas {
    /// Get UV coords for a glyph within the rendered atlas.
    pub fn fetch_uv(&self, glyph_id: u32) -> Option<&Rectangle> {
        let glyphs_len = self.glyph_ids.len();
        assert_eq!(glyphs_len, self.uv_rects.len());

        for i in 0..glyphs_len {
            if self.glyph_ids[i] == glyph_id {
                return Some(&self.uv_rects[i])
            }
        }
        None
    }
}
