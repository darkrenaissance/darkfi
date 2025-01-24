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

use miniquad::TextureId;

use crate::{
    error::Result,
    gfx::{Rectangle, RenderApi},
    util::ansi_texture,
};

use super::{Glyph, Sprite};

pub struct RenderedAtlas {
    pub uv_rects: Vec<Rectangle>,
    pub texture_id: TextureId,
}

const ATLAS_GAP: usize = 2;

pub async fn make_texture_atlas(
    render_api: &RenderApi,
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
                    x: (ATLAS_GAP + current_x) as f32 / total_width as f32,
                    y: ATLAS_GAP as f32 / total_height as f32,
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
