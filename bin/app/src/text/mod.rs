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
pub use render::{render_layout, render_layout_with_opts, DebugRenderOptions};

// Global shared FontContext (thread-safe via internal Arc<Mutex<>>)
pub static GLOBAL_FONT_CTX: LazyLock<parley::FontContext> = LazyLock::new(|| {
    let collection = Collection::new(CollectionOptions { shared: true, system_fonts: false });

    let source_cache = SourceCache::new(SourceCacheOptions { shared: true });

    let mut font_ctx = parley::FontContext { collection, source_cache };

    let font_data = include_bytes!("../../ibm-plex-mono-regular.otf") as &[u8];
    font_ctx.collection.register_fonts(peniko::Blob::new(Arc::new(font_data)), None);

    let font_data = include_bytes!("../../NotoColorEmoji.ttf") as &[u8];
    font_ctx.collection.register_fonts(peniko::Blob::new(Arc::new(font_data)), None);

    font_ctx
});

// Thread-local LayoutContext
thread_local! {
    pub static THREAD_LAYOUT_CTX: RefCell<parley::LayoutContext<Color>> =
        RefCell::new(parley::LayoutContext::new());
}

// Public constants
pub const FONT_STACK: &[parley::FontFamily<'_>] = &[
    parley::FontFamily::Named(std::borrow::Cow::Borrowed("IBM Plex Mono")),
    parley::FontFamily::Named(std::borrow::Cow::Borrowed("Noto Color Emoji")),
];

// FREE FUNCTIONS (no TextContext wrapper!)
pub fn make_layout(
    text: &str,
    text_color: Color,
    font_size: f32,
    lineheight: f32,
    window_scale: f32,
    width: Option<f32>,
    underlines: &[Range<usize>],
) -> parley::Layout<Color> {
    make_layout2(text, text_color, font_size, lineheight, window_scale, width, underlines, &[])
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
) -> parley::Layout<Color> {
    THREAD_LAYOUT_CTX.with(|layout_ctx| {
        let mut layout_ctx = layout_ctx.borrow_mut();
        let mut font_ctx = GLOBAL_FONT_CTX.clone();

        let mut builder = layout_ctx.ranged_builder(&mut font_ctx, text, window_scale, false);
        builder.push_default(parley::LineHeight::FontSizeRelative(lineheight));
        builder.push_default(parley::StyleProperty::FontSize(font_size));
        builder.push_default(parley::StyleProperty::FontStack(parley::FontStack::List(
            FONT_STACK.into(),
        )));
        builder.push_default(parley::StyleProperty::Brush(text_color));
        builder.push_default(parley::StyleProperty::OverflowWrap(parley::OverflowWrap::Anywhere));

        for underline in underlines {
            builder.push(parley::StyleProperty::Underline(true), underline.clone());
        }

        for (range, color) in foreground_colors {
            builder.push(parley::StyleProperty::Brush(*color), range.clone());
        }

        let mut layout: parley::Layout<Color> = builder.build(text);
        layout.break_all_lines(width);
        layout.align(width, parley::Alignment::Start, parley::AlignmentOptions::default());
        layout
    })
}
