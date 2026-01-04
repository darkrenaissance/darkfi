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

use async_lock::Mutex as AsyncMutex;
use std::{
    ops::Range,
    sync::{Arc, OnceLock},
};

use crate::{mesh::Color, util::spawn_thread};

pub mod atlas;
mod editor;
pub use editor::Editor;
mod render;
pub use render::{render_layout, render_layout_with_opts, DebugRenderOptions};

use darkfi::system::CondVar;

pub struct AsyncGlobal<T> {
    cv: CondVar,
    val: OnceLock<AsyncMutex<T>>,
}

impl<T> AsyncGlobal<T> {
    const fn new() -> Self {
        Self { cv: CondVar::new(), val: OnceLock::new() }
    }

    fn set(&self, val: T) {
        self.val.set(AsyncMutex::new(val)).ok().unwrap();
        self.cv.notify();
    }

    pub async fn get<'a>(&'a self) -> async_lock::MutexGuard<'a, T> {
        self.cv.wait().await;
        self.val.get().unwrap().lock().await
    }
}

pub static TEXT_CTX: AsyncGlobal<TextContext> = AsyncGlobal::new();

pub fn init_txt_ctx() {
    spawn_thread("init_txt_ctx", || {
        // This is quite slow. It takes 300ms
        let txt_ctx = TextContext::new();
        TEXT_CTX.set(txt_ctx);
    });
}

/// Initializing this is expensive ~300ms, but storage is ~2kb.
/// It has to be created once and reused. Currently we use thread local storage.
pub struct TextContext {
    font_ctx: parley::FontContext,
    layout_ctx: parley::LayoutContext<Color>,
}

impl TextContext {
    fn new() -> Self {
        let layout_ctx = parley::LayoutContext::new();
        let mut font_ctx = parley::FontContext::new();

        let font_data = include_bytes!("../../ibm-plex-mono-regular.otf") as &[u8];
        let _font_inf =
            font_ctx.collection.register_fonts(peniko::Blob::new(Arc::new(font_data)), None);

        let font_data = include_bytes!("../../NotoColorEmoji.ttf") as &[u8];
        let _font_inf =
            font_ctx.collection.register_fonts(peniko::Blob::new(Arc::new(font_data)), None);

        //for (family_id, _) in font_inf {
        //    let family_name = font_ctx.collection.family_name(family_id).unwrap();
        //    trace!(target: "text", "Loaded font: {family_name}");
        //}

        Self { font_ctx, layout_ctx }
    }

    #[cfg(not(target_os = "android"))]
    pub fn borrow(&mut self) -> (&mut parley::FontContext, &mut parley::LayoutContext<Color>) {
        (&mut self.font_ctx, &mut self.layout_ctx)
    }

    pub fn make_layout(
        &mut self,
        text: &str,
        text_color: Color,
        font_size: f32,
        lineheight: f32,
        window_scale: f32,
        width: Option<f32>,
        underlines: &[Range<usize>],
    ) -> parley::Layout<Color> {
        self.make_layout2(
            text,
            text_color,
            font_size,
            lineheight,
            window_scale,
            width,
            underlines,
            &[],
        )
    }

    pub fn make_layout2(
        &mut self,
        text: &str,
        text_color: Color,
        font_size: f32,
        lineheight: f32,
        window_scale: f32,
        width: Option<f32>,
        underlines: &[Range<usize>],
        foreground_colors: &[(Range<usize>, Color)],
    ) -> parley::Layout<Color> {
        let mut builder =
            self.layout_ctx.ranged_builder(&mut self.font_ctx, &text, window_scale, false);
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

        let mut layout: parley::Layout<Color> = builder.build(&text);
        layout.break_all_lines(width);
        layout.align(width, parley::Alignment::Start, parley::AlignmentOptions::default());
        layout
    }
}

pub const FONT_STACK: &[parley::FontFamily<'_>] = &[
    parley::FontFamily::Named(std::borrow::Cow::Borrowed("IBM Plex Mono")),
    parley::FontFamily::Named(std::borrow::Cow::Borrowed("Noto Color Emoji")),
];
