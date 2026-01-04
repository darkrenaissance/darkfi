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
    gfx::{DebugTag, DrawInstruction, DrawMesh, Point, Rectangle, RenderApi},
    mesh::{Color, MeshBuilder, COLOR_WHITE},
};

use super::atlas::{Atlas, RenderedAtlas, RunIdx};

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
    layout: &parley::Layout<Color>,
    render_api: &RenderApi,
    tag: DebugTag,
) -> Vec<DrawInstruction> {
    render_layout_with_opts(layout, DebugRenderOptions::OFF, render_api, tag)
}

pub fn render_layout_with_opts(
    layout: &parley::Layout<Color>,
    opts: DebugRenderOptions,
    render_api: &RenderApi,
    tag: DebugTag,
) -> Vec<DrawInstruction> {
    // First pass to create atlas
    let mut scale_ctx = swash::scale::ScaleContext::new();
    let mut atlas = Atlas::new(render_api, tag);
    let mut run_idx = 0;
    for line in layout.lines() {
        for item in line.items() {
            match item {
                parley::PositionedLayoutItem::GlyphRun(glyph_run) => {
                    push_glyphs(&mut atlas, &glyph_run, run_idx, &mut scale_ctx, render_api, tag);
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
    for line in layout.lines() {
        for item in line.items() {
            match item {
                parley::PositionedLayoutItem::GlyphRun(glyph_run) => {
                    let mesh = render_glyph_run(
                        &mut scale_ctx,
                        &glyph_run,
                        run_idx,
                        opts,
                        &atlas,
                        render_api,
                        tag,
                    );
                    instrs.push(DrawInstruction::Draw(mesh));
                    run_idx += 1;
                }
                parley::PositionedLayoutItem::InlineBox(_) => {}
            }
        }
    }
    instrs
}

fn push_glyphs(
    atlas: &mut Atlas,
    glyph_run: &parley::GlyphRun<'_, Color>,
    run_idx: RunIdx,
    scale_ctx: &mut swash::scale::ScaleContext,
    render_api: &RenderApi,
    tag: DebugTag,
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
    scale_ctx: &mut swash::scale::ScaleContext,
    glyph_run: &parley::GlyphRun<'_, Color>,
    run_idx: usize,
    opts: DebugRenderOptions,
    atlas: &RenderedAtlas,
    render_api: &RenderApi,
    tag: DebugTag,
) -> DrawMesh {
    let mut run_x = glyph_run.offset();
    let run_y = glyph_run.baseline();
    let style = glyph_run.style();
    let color = style.brush;
    //trace!(target: "text::render", "render_glyph_run run_idx={run_idx} baseline={run_y}");

    let mut mesh = MeshBuilder::new(tag);

    if let Some(underline) = &style.underline {
        render_underline(underline, glyph_run, &mut mesh);
    }

    for glyph in glyph_run.glyphs() {
        let glyph_inf = atlas.fetch_uv(glyph.id as u16, run_idx).expect("missing glyph UV rect");

        let glyph_x = run_x + glyph.x;
        let glyph_y = run_y - glyph.y;
        run_x += glyph.advance;

        let glyph_rect = Rectangle::new(
            glyph_x + glyph_inf.place.left as f32,
            glyph_y - glyph_inf.place.top as f32,
            glyph_inf.place.width as f32,
            glyph_inf.place.height as f32,
        );

        if opts.has(DebugRenderOptions::GLYPH) {
            mesh.draw_outline(&glyph_rect, [0., 1., 0., 0.7], 1.);
        }

        let color = if glyph_inf.is_color { COLOR_WHITE } else { color };
        mesh.draw_box(&glyph_rect, color, &glyph_inf.uv_rect);
    }

    if opts.has(DebugRenderOptions::BASELINE) {
        mesh.draw_filled_box(
            &Rectangle::new(glyph_run.offset(), glyph_run.baseline(), glyph_run.advance(), 1.),
            [0., 0., 1., 0.7],
        );
    }

    mesh.alloc(render_api).draw_with_textures(vec![atlas.texture.clone()])
}

fn render_underline(
    underline: &parley::layout::Decoration<Color>,
    glyph_run: &parley::GlyphRun<'_, Color>,
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
    let y = glyph_run.baseline() - offset + width / 2.;

    let start_x = glyph_run.offset();
    let end_x = start_x + glyph_run.advance();

    let start = Point::new(start_x, y);
    let end = Point::new(end_x, y);

    mesh.draw_line(start, end, color, width);
}

fn create_atlas(
    scale_ctx: &mut swash::scale::ScaleContext,
    glyph_run: &parley::GlyphRun<'_, Color>,
    run_idx: usize,
    render_api: &RenderApi,
    tag: DebugTag,
) -> RenderedAtlas {
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

    let mut atlas = Atlas::new(render_api, tag);
    for glyph in glyph_run.glyphs() {
        atlas.push_glyph(glyph.id as u16, run_idx, &mut scaler);
    }
    //atlas.dump(&format!("/tmp/atlas_{run_idx}.png"));
    atlas.make()
}
