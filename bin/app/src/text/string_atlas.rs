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

use crate::{
    gfx::{DebugTag, Rectangle, RectangleUnion, Renderer},
    mesh::COLOR_WHITE,
};

use super::{
    atlas::{Atlas, RenderedAtlas},
    make_layout,
    render::push_glyphs,
};

/// One rasterized glyph of a string, placed relative to the string's
/// layout origin and mapped into the shared atlas texture.
pub struct StringAtlasGlyph {
    /// Glyph rect in virtual units relative to the string layout origin
    pub rect: Rectangle,
    /// UV rect within the shared atlas texture
    pub uv_rect: Rectangle,
}

/// Atlas-backed geometry for one input string.
pub struct StringAtlasEntry {
    pub glyphs: Vec<StringAtlasGlyph>,
    /// Union of the glyph rects in virtual units relative to the string
    /// layout origin
    pub ink_bounds: Rectangle,
}

/// A single shared atlas texture covering a fixed list of strings.
pub struct StringAtlas {
    /// The rendered atlas holding every glyph of every input string
    pub rendered: RenderedAtlas,
    /// One entry per input string, index-aligned with the input
    pub entries: Vec<StringAtlasEntry>,
}

/// Rasterize every glyph of every string once and pack them into a
/// single shared atlas texture. Returns per-string glyph geometry so
/// callers can draw each string as textured quads referencing the
/// shared texture instead of allocating per-string GPU resources.
pub fn make_string_atlas(
    strings: &[&str],
    font_size: f32,
    window_scale: f32,
    max_row_width: usize,
    renderer: &Renderer,
    tag: DebugTag,
) -> StringAtlas {
    let layouts: Vec<_> = strings
        .iter()
        .map(|string| make_layout(string, COLOR_WHITE, font_size, 1., window_scale, None, &[]))
        .collect();

    let mut atlas = Atlas::with_max_row_width(renderer, tag, max_row_width);
    let mut scale_ctx = swash::scale::ScaleContext::new();
    let mut run_idx = 0;

    for layout in &layouts {
        for line in layout.lines() {
            for item in line.items() {
                let parley::PositionedLayoutItem::GlyphRun(glyph_run) = item else { continue };
                push_glyphs(&mut atlas, &glyph_run, run_idx, &mut scale_ctx);
                run_idx += 1;
            }
        }
    }

    let rendered = atlas.make();

    let mut entries = Vec::with_capacity(layouts.len());
    let mut run_idx = 0;
    for layout in &layouts {
        let mut glyphs = vec![];
        let mut bounds = RectangleUnion::new();

        for line in layout.lines() {
            for item in line.items() {
                let parley::PositionedLayoutItem::GlyphRun(glyph_run) = item else { continue };
                let mut run_x = glyph_run.offset();
                let run_y = glyph_run.baseline();

                for glyph in glyph_run.glyphs() {
                    let glyph_inf =
                        rendered.fetch_uv(glyph.id as u16, run_idx).expect("missing glyph UV rect");

                    let glyph_x = run_x + glyph.x;
                    let glyph_y = run_y - glyph.y;
                    run_x += glyph.advance;

                    let rect = Rectangle::new(
                        (glyph_x + glyph_inf.place.left as f32) / layout.scale(),
                        (glyph_y - glyph_inf.place.top as f32) / layout.scale(),
                        glyph_inf.place.width as f32 / layout.scale(),
                        glyph_inf.place.height as f32 / layout.scale(),
                    );
                    bounds.add(rect);

                    let uv_rect = glyph_inf.uv_rect;
                    glyphs.push(StringAtlasGlyph { rect, uv_rect });
                }

                run_idx += 1;
            }
        }

        let ink_bounds = bounds.get().unwrap_or(Rectangle::zero());
        entries.push(StringAtlasEntry { glyphs, ink_bounds });
    }

    StringAtlas { rendered, entries }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_make_string_atlas() {
        let (method_send, _method_recv) = async_channel::unbounded();
        let renderer = Renderer::new(method_send);

        let strings = ["a", ":", "\u{1F600}", "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}"];
        let atlas = make_string_atlas(&strings, 40., 1., 4096, &renderer, None);

        assert_eq!(atlas.entries.len(), 4);
        for entry in &atlas.entries {
            assert!(!entry.glyphs.is_empty());
            assert!(entry.ink_bounds.w > 0.);
            assert!(entry.ink_bounds.h > 0.);
            for glyph in &entry.glyphs {
                assert!(glyph.rect.w > 0.);
                assert!(glyph.rect.h > 0.);
                assert!((0. ..=1.).contains(&glyph.uv_rect.x));
                assert!((0. ..=1.).contains(&glyph.uv_rect.y));
                assert!((0. ..=1.).contains(&(glyph.uv_rect.x + glyph.uv_rect.w)));
                assert!((0. ..=1.).contains(&(glyph.uv_rect.y + glyph.uv_rect.h)));
            }
        }
    }
}
