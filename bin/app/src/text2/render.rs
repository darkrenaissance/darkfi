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
    gfx::{GfxDrawInstruction, GfxDrawMesh, Rectangle, RenderApi},
    mesh::{Color, MeshBuilder, COLOR_WHITE},
};

use super::atlas::Atlas;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct DebugRenderOptions(u32);

impl DebugRenderOptions {
    pub const Off: DebugRenderOptions = DebugRenderOptions(0b00);
    pub const Glyph: DebugRenderOptions = DebugRenderOptions(0b01);
    pub const Baseline: DebugRenderOptions = DebugRenderOptions(0b10);

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

pub fn render_layout(
    layout: &parley::Layout<Color>,
    render_api: &RenderApi,
) -> Vec<GfxDrawInstruction> {
    render_layout_with_opts(layout, DebugRenderOptions::Off, render_api)
}

pub fn render_layout_with_opts(
    layout: &parley::Layout<Color>,
    opts: DebugRenderOptions,
    render_api: &RenderApi,
) -> Vec<GfxDrawInstruction> {
    let mut scale_cx = swash::scale::ScaleContext::new();
    let mut run_idx = 0;
    let mut instrs = vec![];
    for line in layout.lines() {
        for item in line.items() {
            match item {
                parley::PositionedLayoutItem::GlyphRun(glyph_run) => {
                    let mesh =
                        render_glyph_run(&mut scale_cx, &glyph_run, run_idx, opts, render_api);
                    instrs.push(GfxDrawInstruction::Draw(mesh));
                    run_idx += 1;
                }
                parley::PositionedLayoutItem::InlineBox(_) => {}
            }
        }
    }
    instrs
}

fn render_glyph_run(
    scale_ctx: &mut swash::scale::ScaleContext,
    glyph_run: &parley::GlyphRun<'_, Color>,
    run_idx: usize,
    opts: DebugRenderOptions,
    render_api: &RenderApi,
) -> GfxDrawMesh {
    let mut run_x = glyph_run.offset();
    let run_y = glyph_run.baseline();
    let style = glyph_run.style();
    let color = style.brush;

    let run = glyph_run.run();
    trace!(target: "text::render", "render_glyph_run run_idx={run_idx}");

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

    let mut atlas = Atlas::new(scaler, render_api);
    for glyph in glyph_run.glyphs() {
        atlas.push_glyph(glyph);
    }
    //atlas.dump(&format!("/tmp/atlas_{run_idx}.png"));
    let atlas = atlas.make();

    let mut mesh = MeshBuilder::new();
    for glyph in glyph_run.glyphs() {
        let glyph_inf = atlas.fetch_uv(glyph.id).expect("missing glyph UV rect");

        let glyph_x = run_x + glyph.x;
        let glyph_y = run_y - glyph.y;
        run_x += glyph.advance;

        let glyph_rect = Rectangle::new(
            glyph_x + glyph_inf.place.left as f32,
            glyph_y - glyph_inf.place.top as f32,
            glyph_inf.place.width as f32,
            glyph_inf.place.height as f32,
        );

        if opts.has(DebugRenderOptions::Glyph) {
            mesh.draw_outline(&glyph_rect, [0., 1., 0., 0.7], 1.);
        }

        let color = if glyph_inf.is_color { COLOR_WHITE } else { color };
        mesh.draw_box(&glyph_rect, color, &glyph_inf.uv_rect);
    }

    if opts.has(DebugRenderOptions::Baseline) {
        mesh.draw_filled_box(
            &Rectangle::new(glyph_run.offset(), glyph_run.baseline(), glyph_run.advance(), 1.),
            [0., 0., 1., 0.7],
        );
    }

    mesh.alloc(render_api).draw_with_texture(atlas.texture)
}
