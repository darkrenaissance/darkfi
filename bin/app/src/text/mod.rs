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

//! GPU text rendering pipeline.
//!
//! Text is drawn as textured quads on the GPU; no platform text API is
//! involved. The pipeline has three layers:
//!
//! * Layout (`make_layout`): parley shapes a string into glyph runs
//!   using the font stack (IBM Plex Mono, Noto Color Emoji, DarkIRC
//!   Emoji) registered in `GLOBAL_FONT_CTX`. Coordinates are physical
//!   pixels (window scale is baked in by parley); `TextLayout` remembers
//!   the scale, and consumers divide by it exactly once to get the
//!   virtual units the renderer expects. Rasterization still happens at
//!   physical resolution so text stays crisp while the renderer
//!   re-applies the scale via `SetScale`.
//!
//! * Rendering (`render`): two passes over a layout. First every glyph
//!   of every run is rasterized with swash and packed into an `Atlas`:
//!   one RGBA8 texture per call, color glyphs stored as RGBA and mask
//!   glyphs as alpha. Then each run becomes a `DrawMesh` of quads whose
//!   UVs reference that texture, so a whole run draws in one call. This
//!   atlas is transient (one per layout): the right trade-off for
//!   arbitrary dynamic text such as chat messages and labels, where
//!   layouts are short-lived and each draws from its own texture.
//!
//! * Batching for fixed sets (`string_atlas`): the opposite trade-off.
//!   `make_string_atlas` lays out many fixed strings up front and packs
//!   all their glyphs into a single shared atlas: one texture and one
//!   raster per glyph for the entire set, returning per-string quad
//!   geometry to the caller. Used by the emoji picker, where rendering
//!   each icon through the dynamic path allocated one texture plus a
//!   vertex/index buffer pair per emoji (~574 icons, ~2s of generation)
//!   for glyphs that only ever needed to reference a shared sheet.
//!
//! The packer itself lives in `atlas`: sprites are separated by a 2px
//! gap on all sides to prevent UV bleed, and can optionally wrap into
//! rows capped at `MAX_TEXTURE_DIMENSION` so large fixed sets stay
//! within GPU texture size limits (GLES3/WebGL2 only guarantee 2048),
//! failing loudly at build time instead of corrupting at draw time.

use parley::fontique::{Collection, CollectionOptions, SourceCache, SourceCacheOptions};
use std::{
    cell::RefCell,
    ops::Range,
    sync::{Arc, LazyLock},
};

use crate::mesh::Color;

pub mod atlas;
mod editor;
pub use editor::Editor;
mod render;
#[cfg(not(target_os = "android"))]
pub use render::render_raw_layout;
pub use render::{render_backgrounds, render_layout, render_layout_with_opts, DebugRenderOptions};
mod string_atlas;
pub use string_atlas::make_string_atlas;

pub static GLOBAL_FONT_CTX: LazyLock<parley::FontContext> = LazyLock::new(|| {
    let mut font_ctx = parley::FontContext {
        collection: Collection::new(CollectionOptions { shared: true, system_fonts: true }),
        source_cache: SourceCache::new(SourceCacheOptions { shared: true }),
    };

    let font_data = include_bytes!("../../data/font/ibm-plex-mono-regular.otf") as &[u8];
    font_ctx.collection.register_fonts(peniko::Blob::new(Arc::new(font_data)), None);

    let font_data = include_bytes!("../../data/font/NotoColorEmoji.ttf") as &[u8];
    font_ctx.collection.register_fonts(peniko::Blob::new(Arc::new(font_data)), None);

    let font_data = include_bytes!("../../data/font/darkfi-custom-emoji.ttf") as &[u8];
    font_ctx.collection.register_fonts(peniko::Blob::new(Arc::new(font_data)), None);

    font_ctx
});

thread_local! {
    pub static THREAD_LAYOUT_CTX: RefCell<parley::LayoutContext<Color>> =
        RefCell::new(parley::LayoutContext::new());
}

const FONT_STACK: &[parley::FontFamilyName<'_>] = &[
    parley::FontFamilyName::named("IBM Plex Mono"),
    parley::FontFamilyName::named("Noto Color Emoji"),
    parley::FontFamilyName::named("DarkIRC Emoji"),
];

/// A parley layout paired with the window scale it was built with.
///
/// Parley bakes the builder scale into every coordinate (font size,
/// advances, glyph positions), so the underlying layout is in physical
/// pixels. The renderer applies the window scale again via `SetScale`,
/// so geometry consumed in virtual units must be divided by the scale
/// exactly once. The accessors below do that division. Rendering code
/// in `render.rs` divides when emitting meshes. Glyph rasterization
/// still happens at physical resolution so text stays crisp.
#[derive(Clone)]
pub struct TextLayout {
    layout: parley::Layout<Color>,
    /// Scale parley baked into `layout`. Divide physical coords by this
    /// to get virtual units.
    scale: f32,
}

impl std::ops::Deref for TextLayout {
    type Target = parley::Layout<Color>;

    fn deref(&self) -> &Self::Target {
        &self.layout
    }
}

impl std::ops::DerefMut for TextLayout {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.layout
    }
}

impl Default for TextLayout {
    fn default() -> Self {
        Self { layout: parley::Layout::default(), scale: 1. }
    }
}

impl TextLayout {
    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// Height in virtual units
    pub fn height(&self) -> f32 {
        self.layout.height() / self.scale
    }

    /// Width in virtual units
    pub fn width(&self) -> f32 {
        self.layout.width() / self.scale
    }
}

pub fn make_layout(
    text: &str,
    text_color: Color,
    font_size: f32,
    lineheight: f32,
    window_scale: f32,
    width: Option<f32>,
    underlines: &[Range<usize>],
) -> TextLayout {
    make_layout2(
        text,
        text_color,
        font_size,
        lineheight,
        window_scale,
        width,
        underlines,
        &[],
        parley::Alignment::Start,
        parley::OverflowWrap::Normal,
    )
}

pub fn make_layout2(
    text: &str,
    text_color: Color,
    font_size: f32,
    lineheight: f32,
    window_scale: f32,
    width: Option<f32>,
    underlines: &[Range<usize>],
    foreground_colors: &[(Range<usize>, Color)],
    text_align: parley::Alignment,
    overflow_wrap: parley::OverflowWrap,
) -> TextLayout {
    THREAD_LAYOUT_CTX.with(|layout_ctx| {
        let mut layout_ctx = layout_ctx.borrow_mut();
        let mut font_ctx = GLOBAL_FONT_CTX.clone();

        let mut builder = layout_ctx.ranged_builder(&mut font_ctx, text, window_scale, false);
        builder.push_default(parley::LineHeight::FontSizeRelative(lineheight));
        builder.push_default(parley::StyleProperty::FontSize(font_size));
        builder.push_default(parley::StyleProperty::from(FONT_STACK));
        builder.push_default(parley::StyleProperty::Brush(text_color));
        builder.push_default(parley::StyleProperty::OverflowWrap(overflow_wrap));

        for underline in underlines {
            builder.push(parley::StyleProperty::Underline(true), underline.clone());
        }

        for (range, color) in foreground_colors {
            builder.push(parley::StyleProperty::Brush(*color), range.clone());
        }

        let mut layout: parley::Layout<Color> = builder.build(text);
        // The wrap width is given in virtual units while the layout
        // coordinates are physical, so scale it up before breaking.
        layout.break_all_lines(width.map(|w| w * window_scale));
        layout.align(text_align, parley::AlignmentOptions::default());
        TextLayout { layout, scale: window_scale }
    })
}
