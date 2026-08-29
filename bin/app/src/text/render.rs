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
    gfx::{DebugTag, DrawInstruction, DrawMesh, Point, Rectangle, RectangleUnion, Renderer},
    mesh::{Color, MeshBuilder, COLOR_WHITE},
};

use super::{
    atlas::{Atlas, RenderedAtlas, RunIdx},
    TextLayout,
};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct DebugRenderOptions(u32);

impl DebugRenderOptions {
    pub const OFF: DebugRenderOptions = DebugRenderOptions(0b00);
    pub const GLYPH: DebugRenderOptions = DebugRenderOptions(0b01);
    pub const BASELINE: DebugRenderOptions = DebugRenderOptions(0b10);

    pub fn has(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }
}

impl std::ops::BitOr for DebugRenderOptions {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}
impl std::ops::BitOrAssign for DebugRenderOptions {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

pub fn render_layout(
    layout: &TextLayout,
    renderer: &Renderer,
    tag: DebugTag,
) -> Vec<DrawInstruction> {
    render_layout_with_opts(layout, DebugRenderOptions::OFF, renderer, tag)
}

/// Render a raw parley layout that was built with the given scale. Used
/// by editors that own their layout internally (e.g. `PlainEditor`).
#[cfg(not(target_os = "android"))]
pub fn render_raw_layout(
    layout: &parley::Layout<Color>,
    scale: f32,
    renderer: &Renderer,
    tag: DebugTag,
) -> Vec<DrawInstruction> {
    render_raw_layout_impl(layout, scale, DebugRenderOptions::OFF, renderer, tag).0
}

/// Draw a filled (and optionally outlined) background box behind every glyph run
/// whose style brush equals `match_brush`. The box tracks the run's horizontal
/// advance and the font-metric ascent/descent vertically, so a run that wraps
/// across lines gets one box per wrapped line.
///
/// Matching by brush (rather than by byte range) is required because a parley
/// `GlyphRun` does not expose its own byte range — only its parent font run does,
/// and a font run is coarser than the per-color segment (e.g. the nick, body, and
/// URL of one line all share a font run). Matching `style().brush` pins the box to
/// exactly the color segment, so only the intended runs (here: the URL runs) are
/// highlighted. The fill is skipped when `bg_color` alpha is ~0; the outline is
/// skipped when `border_size` is ~0 or `border_color` alpha is ~0.
pub fn render_backgrounds(
    layout: &TextLayout,
    match_brush: Color,
    bg_color: Color,
    border_color: Color,
    border_size: f32,
    renderer: &Renderer,
    tag: DebugTag,
) -> Vec<DrawInstruction> {
    let mut instrs = vec![];
    let has_fill = bg_color[3] > 0.;
    let has_border = border_size > 0. && border_color[3] > 0.;
    if !has_fill && !has_border {
        return instrs
    }

    let scale = layout.scale();
    for line in layout.lines() {
        for item in line.items() {
            let parley::PositionedLayoutItem::GlyphRun(glyph_run) = item else { continue };
            if glyph_run.style().brush != match_brush {
                continue
            }

            let metrics = glyph_run.run().metrics();
            let x = glyph_run.offset();
            let y = glyph_run.baseline() - metrics.ascent;
            let w = glyph_run.advance();
            let h = metrics.ascent + metrics.descent;
            let rect = Rectangle::new(x, y, w, h) / scale;

            let mut mesh = MeshBuilder::new(tag);
            if has_fill {
                mesh.draw_filled_box(&rect, bg_color);
            }
            if has_border {
                mesh.draw_outline(&rect, border_color, border_size);
            }
            instrs.push(DrawInstruction::Draw(mesh.alloc(renderer).draw_untextured()));
        }
    }

    instrs
}

pub fn render_layout_with_opts(
    layout: &TextLayout,
    opts: DebugRenderOptions,
    renderer: &Renderer,
    tag: DebugTag,
) -> Vec<DrawInstruction> {
    render_raw_layout_impl(layout, layout.scale(), opts, renderer, tag).0
}

/// Layout coordinates are physical (scale is baked in by parley) while
/// meshes are consumed in virtual units and scaled up again by the
/// renderer's `SetScale`. So every emitted coordinate is divided by
/// `scale` here. Glyphs are still rasterized at physical resolution so
/// the final on-screen texel mapping stays crisp.
fn render_raw_layout_impl(
    layout: &parley::Layout<Color>,
    scale: f32,
    opts: DebugRenderOptions,
    renderer: &Renderer,
    tag: DebugTag,
) -> (Vec<DrawInstruction>, Rectangle) {
    // First pass to create atlas
    let mut scale_ctx = swash::scale::ScaleContext::new();
    let mut atlas = Atlas::new(renderer, tag);
    let mut run_idx = 0;
    for line in layout.lines() {
        for item in line.items() {
            match item {
                parley::PositionedLayoutItem::GlyphRun(glyph_run) => {
                    push_glyphs(&mut atlas, &glyph_run, run_idx, &mut scale_ctx);
                    run_idx += 1;
                }
                parley::PositionedLayoutItem::InlineBox(_) => {}
            }
        }
    }

    // Render the atlas
    let atlas = atlas.make();

    // Second pass to draw glyphs
    let mut run_idx = 0;
    let mut instrs = vec![];
    let mut bounds = RectangleUnion::new();
    for line in layout.lines() {
        for item in line.items() {
            match item {
                parley::PositionedLayoutItem::GlyphRun(glyph_run) => {
                    let (mesh, run_bounds) =
                        render_glyph_run(&glyph_run, run_idx, opts, &atlas, scale, renderer, tag);
                    bounds.join(run_bounds);
                    instrs.push(DrawInstruction::Draw(mesh));
                    run_idx += 1;
                }
                parley::PositionedLayoutItem::InlineBox(_) => {}
            }
        }
    }
    (instrs, bounds.get().unwrap_or(Rectangle::zero()))
}

pub(super) fn push_glyphs(
    atlas: &mut Atlas,
    glyph_run: &parley::GlyphRun<'_, Color>,
    run_idx: RunIdx,
    scale_ctx: &mut swash::scale::ScaleContext,
) {
    let run = glyph_run.run();
    let font = run.font();
    let font_size = run.font_size();
    let normalized_coords = run.normalized_coords();
    let font_ref = swash::FontRef::from_index(font.data.as_ref(), font.index as usize).unwrap();

    let mut scaler = scale_ctx
        .builder(font_ref)
        .size(font_size)
        .hint(true)
        .normalized_coords(normalized_coords)
        .build();

    for glyph in glyph_run.glyphs() {
        atlas.push_glyph(glyph.id as u16, run_idx, &mut scaler);
    }
}

fn render_glyph_run(
    glyph_run: &parley::GlyphRun<'_, Color>,
    run_idx: usize,
    opts: DebugRenderOptions,
    atlas: &RenderedAtlas,
    scale: f32,
    renderer: &Renderer,
    tag: DebugTag,
) -> (DrawMesh, RectangleUnion) {
    let mut run_x = glyph_run.offset();
    let run_y = glyph_run.baseline();
    let style = glyph_run.style();
    let color = style.brush;
    //trace!(target: "text::render", "render_glyph_run run_idx={run_idx} baseline={run_y}");

    let mut mesh = MeshBuilder::new(tag);
    let mut bounds = RectangleUnion::new();

    if let Some(underline) = &style.underline {
        render_underline(underline, glyph_run, scale, &mut mesh);
    }

    for glyph in glyph_run.glyphs() {
        let glyph_inf = atlas.fetch_uv(glyph.id as u16, run_idx).expect("missing glyph UV rect");

        let glyph_x = run_x + glyph.x;
        let glyph_y = run_y - glyph.y;
        run_x += glyph.advance;

        let glyph_rect = Rectangle::new(
            (glyph_x + glyph_inf.place.left as f32) / scale,
            (glyph_y - glyph_inf.place.top as f32) / scale,
            glyph_inf.place.width as f32 / scale,
            glyph_inf.place.height as f32 / scale,
        );

        bounds.add(glyph_rect);

        if opts.has(DebugRenderOptions::GLYPH) {
            mesh.draw_outline(&glyph_rect, [0., 1., 0., 0.7], 1.);
        }

        let color = if glyph_inf.is_color { COLOR_WHITE } else { color };
        mesh.draw_box(&glyph_rect, color, &glyph_inf.uv_rect);
    }

    if opts.has(DebugRenderOptions::BASELINE) {
        let rect =
            Rectangle::new(glyph_run.offset(), glyph_run.baseline(), glyph_run.advance(), 1.) /
                scale;
        mesh.draw_filled_box(&rect, [0., 0., 1., 0.7]);
    }

    (mesh.alloc(renderer).draw_with_textures(vec![atlas.texture.clone()]), bounds)
}

fn render_underline(
    underline: &parley::layout::Decoration<Color>,
    glyph_run: &parley::GlyphRun<'_, Color>,
    scale: f32,
    mesh: &mut MeshBuilder,
) {
    let color = underline.brush;
    let run_metrics = glyph_run.run().metrics();
    let offset = match underline.offset {
        Some(offset) => offset,
        None => run_metrics.underline_offset,
    };
    let width = match underline.size {
        Some(size) => size,
        None => run_metrics.underline_size,
    };
    // The `offset` is the distance from the baseline to the top of the underline
    // so we move the line down by half the width
    // Remember that we are using a y-down coordinate system
    // If there's a custom width, because this is an underline, we want the custom
    // width to go down from the default expectation
    let y = (glyph_run.baseline() - offset + width / 2.) / scale;

    let start_x = glyph_run.offset() / scale;
    let end_x = start_x + glyph_run.advance() / scale;

    let start = Point::new(start_x, y);
    let end = Point::new(end_x, y);

    mesh.draw_line(start, end, color, width / scale);
}
