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

use async_gen::{gen as async_gen, AsyncIter};
use async_trait::async_trait;
use chrono::{Local, NaiveDate, TimeZone};
use darkfi_serial::{Decodable, FutAsyncWriteExt, SerialDecodable, SerialEncodable};
use futures::stream::{Stream, StreamExt};
use image::{ImageBuffer, ImageReader, Rgba};
use miniquad::TextureFormat;
use parking_lot::Mutex as SyncMutex;
use std::{
    collections::HashMap,
    hash::{DefaultHasher, Hash, Hasher},
    io::Cursor,
    pin::pin,
    sync::Arc,
};
use url::Url;

use super::{max, MessageId, Timestamp};
use crate::{
    gfx::{gfxtag, DrawMesh, ManagedTexturePtr, Rectangle, RenderApi},
    mesh::{
        Color, MeshBuilder, COLOR_BLUE, COLOR_CYAN, COLOR_GREEN, COLOR_PINK, COLOR_RED, COLOR_WHITE,
    },
    prop::{PropertyBool, PropertyColor, PropertyFloat32, PropertyPtr},
    text::{self, Glyph, GlyphPositionIter, TextShaper, TextShaperPtr},
    util::enumerate_mut,
};

macro_rules! t { ($($arg:tt)*) => { trace!(target: "ui::chatview::message_buffer", $($arg)*); } }

//const PAGE_SIZE: usize = 10;
//const PRELOAD_PAGES: usize = 10;

const UNCONF_COLOR: [f32; 4] = [0.4, 0.4, 0.4, 1.];

fn is_whitespace(s: &str) -> bool {
    s.chars().all(char::is_whitespace)
}

#[derive(Clone)]
pub struct PrivMessage {
    font_size: f32,
    timestamp_font_size: f32,
    window_scale: f32,

    timestamp: Timestamp,
    id: MessageId,
    nick: String,
    text: String,
    pub confirmed: bool,

    is_selected: bool,

    time_glyphs: Vec<Glyph>,
    unwrapped_glyphs: Vec<Glyph>,
    wrapped_lines: Vec<Vec<Glyph>>,

    atlas: text::RenderedAtlas,
    mesh_cache: Option<DrawMesh>,
}

impl PrivMessage {
    pub fn new(
        mut font_size: f32,
        timestamp_font_size: f32,
        window_scale: f32,

        timestamp: Timestamp,
        id: MessageId,
        nick: String,
        text: String,

        line_width: f32,
        timestamp_width: f32,

        text_shaper: &TextShaper,
        render_api: &RenderApi,
    ) -> Message {
        let timestr = Self::gen_timestr(timestamp);
        let time_glyphs = text_shaper.shape(timestr, timestamp_font_size, window_scale);

        let linetext = if nick == "NOTICE" { text.clone() } else { format!("{nick} {text}") };
        if nick == "NOTICE" {
            font_size *= 0.8;
        }
        let unwrapped_glyphs = text_shaper.shape(linetext, font_size, window_scale);

        let mut atlas = text::Atlas::new(render_api, gfxtag!("chatview_privmsg"));
        atlas.push(&time_glyphs);
        atlas.push(&unwrapped_glyphs);
        let atlas = atlas.make();

        let mut self_ = Self {
            font_size,
            timestamp_font_size,
            window_scale,
            timestamp,
            id,
            nick,
            text,
            confirmed: true,
            is_selected: false,
            time_glyphs,
            unwrapped_glyphs,
            wrapped_lines: vec![],
            atlas,
            mesh_cache: None,
        };
        self_.adjust_width(line_width, timestamp_width);
        Message::Priv(self_)
    }

    fn gen_timestr(timestamp: Timestamp) -> String {
        let dt = Local.timestamp_millis_opt(timestamp as i64).unwrap();
        let timestr = dt.format("%H:%M").to_string();
        timestr
    }

    fn height(&self, line_height: f32) -> f32 {
        self.wrapped_lines.len() as f32 * line_height
    }

    fn gen_mesh(
        &mut self,
        clip: &Rectangle,
        line_height: f32,
        msg_spacing: f32,
        baseline: f32,
        timestamp_width: f32,
        nick_colors: &[Color],
        timestamp_color: Color,
        text_color: Color,
        hi_bg_color: Color,
        debug_render: bool,
        render_api: &RenderApi,
    ) -> DrawMesh {
        if let Some(mesh) = &self.mesh_cache {
            return mesh.clone()
        }

        //t!("gen_mesh({})", glyph_str(&self.unwrapped_glyphs));
        let mut mesh = MeshBuilder::new(gfxtag!("chatview_privmsg"));

        if self.is_selected {
            let height = self.height(line_height) + msg_spacing;
            mesh.draw_filled_box(
                &Rectangle { x: 0., y: -height, w: clip.w, h: height },
                hi_bg_color,
            );
        }

        self.render_timestamp(&mut mesh, baseline, line_height, timestamp_color);
        let off_x = timestamp_width;

        let nick_color = select_nick_color(&self.nick, nick_colors);

        let last_idx = self.wrapped_lines.len() - 1;
        for (i, line) in self.wrapped_lines.iter().rev().enumerate() {
            let off_y = (i + 1) as f32 * line_height;
            let is_last_line = i == last_idx;

            // debug draw baseline
            if debug_render {
                let y = baseline - off_y;
                mesh.draw_filled_box(&Rectangle { x: 0., y: y - 1., w: clip.w, h: 1. }, COLOR_BLUE);
            }

            self.render_line(
                &mut mesh,
                line,
                off_x,
                off_y,
                is_last_line,
                baseline,
                nick_color,
                text_color,
                debug_render,
            );
        }

        if debug_render {
            let height = self.height(line_height);
            mesh.draw_outline(
                &Rectangle { x: 0., y: -height, w: clip.w, h: height },
                COLOR_PINK,
                1.,
            );
        }

        let mesh = mesh.alloc(render_api);
        let mesh = mesh.draw_with_textures(vec![self.atlas.texture.clone()]);
        self.mesh_cache = Some(mesh.clone());

        mesh
    }

    fn render_timestamp(
        &self,
        mesh: &mut MeshBuilder,
        baseline: f32,
        line_height: f32,
        timestamp_color: Color,
    ) {
        let off_y = self.wrapped_lines.len() as f32 * line_height;

        let glyph_pos_iter = GlyphPositionIter::new(
            self.timestamp_font_size,
            self.window_scale,
            &self.time_glyphs,
            baseline,
        );
        for (mut glyph_rect, glyph) in glyph_pos_iter.zip(self.time_glyphs.iter()) {
            let uv_rect = self.atlas.fetch_uv(glyph.glyph_id).expect("missing glyph UV rect");
            glyph_rect.y -= off_y;

            mesh.draw_box(&glyph_rect, timestamp_color, uv_rect);
        }
    }

    fn render_line(
        &self,
        mesh: &mut MeshBuilder,
        line: &Vec<Glyph>,
        off_x: f32,
        off_y: f32,
        is_last: bool,
        baseline: f32,
        nick_color: Color,
        mut text_color: Color,
        _debug_render: bool,
    ) {
        //debug!(target: "ui::chatview", "render_line({})", glyph_str(line));
        // Keep track of the 'section'
        // Section 0   is the nickname (colorized)
        // Section >=1 is just the message itself
        let mut section = 1;
        if is_last && self.nick != "NOTICE" {
            section = 0;
        }

        if self.nick == "NOTICE" {
            text_color[0] = 0.35;
            text_color[1] = 0.81;
            text_color[2] = 0.89;
        }

        let glyph_pos_iter =
            GlyphPositionIter::new(self.font_size, self.window_scale, line, baseline);
        let Some(last_rect) = glyph_pos_iter.last() else { return };
        let rhs = last_rect.rhs();
        if self.nick == "NOTICE" {
            mesh.draw_box(
                &Rectangle::new(off_x, -off_y, rhs, baseline),
                [0., 0.14, 0.16, 1.],
                &Rectangle::zero(),
            );
        }

        let glyph_pos_iter =
            GlyphPositionIter::new(self.font_size, self.window_scale, line, baseline);
        for (mut glyph_rect, glyph) in glyph_pos_iter.zip(line.iter()) {
            let uv_rect = self.atlas.fetch_uv(glyph.glyph_id).expect("missing glyph UV rect");

            glyph_rect.x += off_x;
            glyph_rect.y -= off_y;

            let mut color = match section {
                0 => nick_color,
                _ => {
                    if self.confirmed {
                        text_color
                    } else {
                        UNCONF_COLOR
                    }
                }
            };

            //if debug_render {
            //    mesh.draw_outline(&glyph_rect, COLOR_BLUE, 2.);
            //}

            if glyph.sprite.has_color {
                color = COLOR_WHITE;
            }

            mesh.draw_box(&glyph_rect, color, uv_rect);

            if is_last && section < 1 && is_whitespace(&glyph.substr) {
                section += 1;
            }
        }
    }

    /// clear_mesh() must be called after this.
    fn adjust_params(
        &mut self,
        mut font_size: f32,
        timestamp_font_size: f32,
        window_scale: f32,
        line_width: f32,
        timestamp_width: f32,
        text_shaper: &TextShaper,
        render_api: &RenderApi,
    ) {
        if self.nick == "NOTICE" {
            font_size *= 0.8;
        }
        self.font_size = font_size;
        self.timestamp_font_size = timestamp_font_size;
        self.window_scale = window_scale;

        let timestr = Self::gen_timestr(self.timestamp);
        self.time_glyphs = text_shaper.shape(timestr, timestamp_font_size, window_scale);

        let linetext = if self.nick == "NOTICE" {
            self.text.clone()
        } else {
            format!("{} {}", self.nick, self.text)
        };
        self.unwrapped_glyphs = text_shaper.shape(linetext, font_size, window_scale);

        let mut atlas = text::Atlas::new(render_api, gfxtag!("chatview_privmsg"));
        atlas.push(&self.time_glyphs);
        atlas.push(&self.unwrapped_glyphs);
        self.atlas = atlas.make();

        // We need to rewrap the glyphs since they've been reloaded
        self.adjust_width(line_width, timestamp_width);
    }

    /// clear_mesh() must be called after this.
    fn adjust_width(&mut self, line_width: f32, timestamp_width: f32) {
        let width = line_width - timestamp_width;
        // clamp to > 0
        let width = max(width, 0.);

        // Invalidate wrapped_glyphs and recalc
        self.wrapped_lines =
            text::wrap(width, self.font_size, self.window_scale, &self.unwrapped_glyphs);
    }

    fn clear_mesh(&mut self) {
        // Auto-deletes when refs are dropped
        self.mesh_cache = None;
    }

    fn select(&mut self) {
        self.is_selected = true;
    }
}

impl std::fmt::Debug for PrivMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let dt = Local.timestamp_millis_opt(self.timestamp as i64).unwrap();
        let timestr = dt.format("%H:%M").to_string();
        write!(f, "{} <{}> {}", timestr, self.nick, self.text)
    }
}

#[derive(Clone)]
pub struct DateMessage {
    font_size: f32,
    window_scale: f32,

    timestamp: Timestamp,
    glyphs: Vec<Glyph>,

    atlas: text::RenderedAtlas,
    mesh_cache: Option<DrawMesh>,
}

impl DateMessage {
    pub fn new(
        font_size: f32,
        window_scale: f32,

        timestamp: Timestamp,

        text_shaper: &TextShaper,
        render_api: &RenderApi,
    ) -> Message {
        let datestr = Self::datestr(timestamp);
        let timestamp = Self::timest_to_midnight(timestamp);

        let glyphs = text_shaper.shape(datestr, font_size, window_scale);

        let mut atlas = text::Atlas::new(render_api, gfxtag!("chatview_datemsg"));
        atlas.push(&glyphs);
        let atlas = atlas.make();

        Message::Date(Self { font_size, window_scale, timestamp, glyphs, atlas, mesh_cache: None })
    }

    fn datestr(timestamp: Timestamp) -> String {
        let dt = Local.timestamp_millis_opt(timestamp as i64).unwrap();
        let datestr = dt.format("%a %-d %b %Y").to_string();
        datestr
    }

    fn timest_to_midnight(timestamp: Timestamp) -> Timestamp {
        let dt = Local.timestamp_millis_opt(timestamp as i64).unwrap();
        let dt2 = dt.date_naive().and_hms_opt(0, 0, 0).unwrap();
        assert_eq!(dt.date_naive(), dt2.date());
        let timestamp = Local.from_local_datetime(&dt2).unwrap().timestamp_millis() as u64;
        timestamp
    }

    /// clear_mesh() must be called after this.
    fn adjust_params(
        &mut self,
        font_size: f32,
        window_scale: f32,
        text_shaper: &TextShaper,
        render_api: &RenderApi,
    ) {
        self.font_size = font_size;
        self.window_scale = window_scale;

        let datestr = Self::datestr(self.timestamp);
        self.glyphs = text_shaper.shape(datestr, font_size, window_scale);

        let mut atlas = text::Atlas::new(render_api, gfxtag!("chatview_datemsg"));
        atlas.push(&self.glyphs);
        self.atlas = atlas.make();
    }

    //fn adjust_width(&mut self, line_width: f32) { }

    fn clear_mesh(&mut self) {
        // Auto-deletes when refs are dropped
        self.mesh_cache = None;
    }

    fn gen_mesh(
        &mut self,
        clip: &Rectangle,
        line_height: f32,
        baseline: f32,
        _nick_colors: &[Color],
        timestamp_color: Color,
        _text_color: Color,
        debug_render: bool,
        render_api: &RenderApi,
    ) -> DrawMesh {
        let mut mesh = MeshBuilder::new(gfxtag!("chatview_datemsg"));

        let glyph_pos_iter =
            GlyphPositionIter::new(self.font_size, self.window_scale, &self.glyphs, baseline);
        for (mut glyph_rect, glyph) in glyph_pos_iter.zip(self.glyphs.iter()) {
            let uv_rect = self.atlas.fetch_uv(glyph.glyph_id).expect("missing glyph UV rect");
            glyph_rect.y -= line_height;
            mesh.draw_box(&glyph_rect, timestamp_color, uv_rect);
        }

        if debug_render {
            mesh.draw_outline(
                &Rectangle { x: 0., y: -line_height, w: clip.w, h: line_height },
                COLOR_PINK,
                1.,
            );
        }

        let mesh = mesh.alloc(render_api);
        let mesh = mesh.draw_with_textures(vec![self.atlas.texture.clone()]);
        self.mesh_cache = Some(mesh.clone());

        mesh
    }
}

impl std::fmt::Debug for DateMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let dt = Local.timestamp_millis_opt(self.timestamp as i64).unwrap();
        let datestr = dt.format("%a %-d %b %Y").to_string();
        write!(f, "{}", datestr)
    }
}

#[derive(Clone, SerialEncodable, SerialDecodable)]
pub enum FileMessageStatus {
    Initializing,
    Downloading { progress: f32 },
    Downloaded { path: String },
    Error { msg: String },
}

type GenericImageBuffer = ImageBuffer<Rgba<u8>, Vec<u8>>;

#[derive(Clone)]
pub struct FileMessage {
    font_size: f32,
    window_scale: f32,
    max_width: f32,

    file_url: Url,
    status: FileMessageStatus,
    imgbuf: Arc<SyncMutex<Option<GenericImageBuffer>>>,
    timestamp: Timestamp,
    glyphs: Vec<Vec<Glyph>>,

    atlas: text::RenderedAtlas,
}

impl FileMessage {
    const GLOW_SIZE: f32 = 20.;
    const MARGIN_TOP: f32 = 4.;
    const MARGIN_BOTTOM: f32 = 10.;
    const BOX_PADDING_TOP: f32 = 15.;
    const BOX_PADDING_BOTTOM: f32 = 8.;
    const BOX_PADDING_X: f32 = 15.;
    const IMG_MAX_HEIGHT: f32 = 500.;

    pub fn new(
        font_size: f32,
        window_scale: f32,

        file_url: Url,
        status: FileMessageStatus,
        timestamp: Timestamp,
        _nick: String,

        text_shaper: &TextShaper,
        render_api: &RenderApi,
    ) -> Message {
        let mut glyphs = Vec::new();
        let mut atlas = text::Atlas::new(render_api, gfxtag!("chatview_filemsg"));

        for str in Self::filestr(&file_url, &status) {
            let glyphs_ = text_shaper.shape(str, font_size, window_scale);
            atlas.push(&glyphs_);
            glyphs.push(glyphs_);
        }

        let atlas = atlas.make();

        Message::File(Self {
            font_size,
            window_scale,
            max_width: 0.,
            file_url,
            status,
            imgbuf: Arc::new(SyncMutex::new(None)),
            timestamp,
            glyphs,
            atlas,
        })
    }

    fn filestr(file_url: &Url, status: &FileMessageStatus) -> Vec<String> {
        let status_str = match status {
            FileMessageStatus::Initializing => "starting fud".to_string(),
            FileMessageStatus::Downloading { progress } => format!("downloading [{progress:.1}%]"),
            FileMessageStatus::Downloaded { .. } => "downloaded".to_string(),
            FileMessageStatus::Error { msg } => msg.to_lowercase(),
        };

        vec![
            file_url
                .host_str()
                .map(|file_hash| {
                    if file_hash.len() >= 12 {
                        let first_part = &file_hash[..4];
                        let last_part = &file_hash[file_hash.len() - 4..];
                        format!("{}...{}", first_part, last_part)
                    } else {
                        file_hash.to_string()
                    }
                })
                .unwrap_or("???".to_string()),
            status_str,
        ]
    }

    pub fn set_status(&mut self, status: &FileMessageStatus) {
        self.status = status.clone();

        if let FileMessageStatus::Downloaded { .. } = status {
            let mut imgbuf = self.imgbuf.lock();
            *imgbuf = self.load_img();
        }
    }

    fn adjust_params(
        &mut self,
        font_size: f32,
        window_scale: f32,
        text_shaper: &TextShaper,
        render_api: &RenderApi,
    ) {
        self.font_size = font_size;
        self.window_scale = window_scale;

        self.glyphs = Vec::new();
        let mut atlas = text::Atlas::new(render_api, gfxtag!("chatview_filemsg"));

        for str in Self::filestr(&self.file_url, &self.status) {
            let glyphs = text_shaper.shape(str, font_size, window_scale);
            atlas.push(&glyphs);
            self.glyphs.push(glyphs);
        }

        self.atlas = atlas.make();
    }

    fn adjust_width(&mut self, line_width: f32, timestamp_width: f32) {
        let width = line_width - timestamp_width;
        // clamp to > 0
        self.max_width = max(width, 0.);
    }

    fn clear_mesh(&mut self) {}

    fn get_img_size(&self, imgbuf: &ImageBuffer<Rgba<u8>, Vec<u8>>) -> (f32, f32) {
        let img_w = imgbuf.width() as f32;
        let img_h = imgbuf.height() as f32;

        let width_scale = (self.max_width - Self::GLOW_SIZE) / img_w;
        let height_scale = Self::IMG_MAX_HEIGHT / img_h;

        let scale = width_scale.min(height_scale);
        (img_w * scale, img_h * scale)
    }

    fn gen_mesh(
        &mut self,
        _clip: &Rectangle,
        line_height: f32,
        baseline: f32,
        timestamp_width: f32,
        _nick_colors: &[Color],
        timestamp_color: Color,
        _text_color: Color,
        _debug_render: bool,
        render_api: &RenderApi,
    ) -> Vec<DrawMesh> {
        let uv_rect = Rectangle::from([0., 0., 1., 1.]);

        let imgbuf_ = self.imgbuf.lock();
        if let Some(ref imgbuf) = *imgbuf_ {
            let (img_w, img_h) = self.get_img_size(imgbuf);
            drop(imgbuf_);

            let mesh_rect =
                Rectangle::from([timestamp_width, -img_h - Self::MARGIN_BOTTOM, img_w, img_h]);
            let texture = self.load_texture(render_api);
            let mut mesh_gradient = MeshBuilder::new(gfxtag!("file_gradient"));
            let glow_color = [timestamp_color[0], timestamp_color[1], timestamp_color[2], 0.5];
            mesh_gradient.draw_box_shadow(&mesh_rect, glow_color, Self::GLOW_SIZE);

            let mesh_gradient = mesh_gradient.alloc(render_api);
            let mesh_gradient = mesh_gradient.draw_untextured();

            let mut mesh_img = MeshBuilder::new(gfxtag!("file_img"));
            mesh_img.draw_box(&mesh_rect, COLOR_WHITE, &uv_rect);
            let mesh_img = mesh_img.alloc(render_api);
            let mesh_img = mesh_img.draw_with_textures(vec![texture]);
            return vec![mesh_img, mesh_gradient];
        }
        drop(imgbuf_);

        let mut mesh = MeshBuilder::new(gfxtag!("chatview_filemsg"));

        let color = match self.status {
            FileMessageStatus::Initializing => timestamp_color,
            FileMessageStatus::Downloading { .. } => COLOR_CYAN,
            FileMessageStatus::Downloaded { .. } => COLOR_GREEN,
            FileMessageStatus::Error { .. } => COLOR_RED,
        };

        let mut text_width = 0.;
        for (i, glyphs) in self.glyphs.iter().enumerate() {
            let glyph_pos_iter =
                GlyphPositionIter::new(self.font_size, self.window_scale, &glyphs, baseline);
            for (mut glyph_rect, glyph) in glyph_pos_iter.zip(glyphs.iter()) {
                let uv_rect = self.atlas.fetch_uv(glyph.glyph_id).expect("missing glyph UV rect");
                if glyph_rect.x + glyph_rect.w > text_width {
                    text_width = glyph_rect.x + glyph_rect.w;
                }
                glyph_rect.x += timestamp_width + Self::BOX_PADDING_X;
                glyph_rect.y -= line_height * (self.glyphs.len() - i) as f32 +
                    Self::BOX_PADDING_BOTTOM +
                    Self::MARGIN_BOTTOM;
                mesh.draw_box(&glyph_rect, color, uv_rect);
            }
        }

        let box_width = text_width + Self::BOX_PADDING_X * 2.;
        let box_height = self.glyphs.len() as f32 * line_height +
            Self::BOX_PADDING_TOP +
            Self::BOX_PADDING_BOTTOM;
        let mesh_rect = Rectangle::from([
            timestamp_width,
            -box_height - Self::MARGIN_BOTTOM,
            box_width,
            box_height,
        ]);
        mesh.draw_outline(&mesh_rect, color, 1.);

        let glow_color = [color[0], color[1], color[2], 0.3];
        mesh.draw_box_shadow(&mesh_rect, glow_color, Self::GLOW_SIZE);

        let mesh = mesh.alloc(render_api);
        let mesh = mesh.draw_with_textures(vec![self.atlas.texture.clone()]);

        vec![mesh]
    }

    fn load_img(&self) -> Option<ImageBuffer<Rgba<u8>, Vec<u8>>> {
        if let FileMessageStatus::Downloaded { path } = &self.status {
            let path = path.as_str();

            let data = Arc::new(SyncMutex::new(vec![]));
            let data2 = data.clone();
            miniquad::fs::load_file(path, move |res| match res {
                Ok(res) => *data2.lock() = res,
                Err(e) => {
                    error!("Resource not found! {e}");
                }
            });
            let data = std::mem::take(&mut *data.lock());
            let Ok(img) =
                ImageReader::new(Cursor::new(data)).with_guessed_format().unwrap().decode()
            else {
                return None;
            };
            return Some(img.to_rgba8());
        }

        None
    }

    fn load_texture(&self, render_api: &RenderApi) -> ManagedTexturePtr {
        let imgbuf = self.imgbuf.lock();
        let img = imgbuf.as_ref().unwrap();

        let width = img.width() as u16;
        let height = img.height() as u16;
        let bmp = img.as_raw().clone();
        drop(imgbuf);

        render_api.new_texture(
            width,
            height,
            bmp,
            TextureFormat::RGBA8,
            gfxtag!("file_img_texture"),
        )
    }

    pub fn height(&self, line_height: f32) -> f32 {
        let imgbuf = self.imgbuf.lock();
        imgbuf
            .as_ref()
            .map(|buf| self.get_img_size(buf).1 as f32 + Self::MARGIN_TOP + Self::MARGIN_BOTTOM)
            .unwrap_or(
                line_height * self.glyphs.len() as f32 +
                    Self::BOX_PADDING_TOP +
                    Self::BOX_PADDING_BOTTOM +
                    Self::MARGIN_TOP +
                    Self::MARGIN_BOTTOM,
            )
    }

    fn select(&mut self) {}
}

impl std::fmt::Debug for FileMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "file: {}", self.file_url)
    }
}

/// Easier than fucking around with traits nonsense
#[derive(Debug)]
pub enum Message {
    Priv(PrivMessage),
    Date(DateMessage),
    File(FileMessage),
}

impl Message {
    fn timestamp(&self) -> u64 {
        match self {
            Self::Priv(m) => m.timestamp,
            Self::Date(m) => m.timestamp,
            Self::File(m) => m.timestamp,
        }
    }

    fn height(&self, line_height: f32) -> f32 {
        match self {
            Self::Priv(m) => m.height(line_height),
            Self::Date(_) => line_height,
            Self::File(m) => m.height(line_height),
        }
    }

    fn adjust_params(
        &mut self,
        font_size: f32,
        timestamp_font_size: f32,
        window_scale: f32,
        line_width: f32,
        timestamp_width: f32,
        text_shaper: &TextShaper,
        render_api: &RenderApi,
    ) {
        match self {
            Self::Priv(m) => m.adjust_params(
                font_size,
                timestamp_font_size,
                window_scale,
                line_width,
                timestamp_width,
                text_shaper,
                render_api,
            ),
            Self::Date(m) => m.adjust_params(font_size, window_scale, text_shaper, render_api),
            Self::File(m) => m.adjust_params(font_size, window_scale, text_shaper, render_api),
        }
    }

    fn adjust_width(&mut self, line_width: f32, timestamp_width: f32) {
        match self {
            Self::Priv(m) => m.adjust_width(line_width, timestamp_width),
            Self::Date(_) => {}
            Self::File(m) => m.adjust_width(line_width, timestamp_width),
        }
    }

    fn clear_mesh(&mut self) {
        match self {
            Self::Priv(m) => m.clear_mesh(),
            Self::Date(m) => m.clear_mesh(),
            Self::File(m) => m.clear_mesh(),
        }
    }

    fn gen_mesh(
        &mut self,
        clip: &Rectangle,
        line_height: f32,
        msg_spacing: f32,
        baseline: f32,
        timestamp_width: f32,
        nick_colors: &[Color],
        timestamp_color: Color,
        text_color: Color,
        hi_bg_color: Color,
        debug_render: bool,
        render_api: &RenderApi,
    ) -> Vec<DrawMesh> {
        match self {
            Self::Priv(m) => vec![m.gen_mesh(
                clip,
                line_height,
                msg_spacing,
                baseline,
                timestamp_width,
                nick_colors,
                timestamp_color,
                text_color,
                hi_bg_color,
                debug_render,
                render_api,
            )],
            Self::Date(m) => vec![m.gen_mesh(
                clip,
                line_height,
                baseline,
                // No timestamp_width
                nick_colors,
                timestamp_color,
                text_color,
                // No hi_bg_color since dates can't be highlighted
                debug_render,
                render_api,
            )],
            Self::File(m) => m.gen_mesh(
                clip,
                line_height,
                baseline,
                timestamp_width,
                nick_colors,
                timestamp_color,
                text_color,
                debug_render,
                render_api,
            ),
        }
    }

    fn is_date(&self) -> bool {
        match self {
            Self::Priv(_) => false,
            Self::Date(_) => true,
            Self::File(_) => false,
        }
    }

    fn select(&mut self) {
        match self {
            Self::Priv(m) => m.select(),
            Self::Date(_) => {}
            Self::File(m) => m.select(),
        }
    }

    fn get_privmsg_mut(&mut self) -> Option<&mut PrivMessage> {
        match self {
            Message::Priv(msg) => Some(msg),
            _ => None,
        }
    }

    fn get_filemsg_mut(&mut self) -> Option<&mut FileMessage> {
        match self {
            Message::File(msg) => Some(msg),
            _ => None,
        }
    }
}

fn select_nick_color(nick: &str, nick_colors: &[Color]) -> Color {
    let mut hasher = DefaultHasher::new();
    nick.hash(&mut hasher);
    let i = hasher.finish() as usize;
    let color = nick_colors[i % nick_colors.len()];
    color
}

pub struct MessageBuffer {
    /// From most recent to older
    msgs: Vec<Message>,
    date_msgs: HashMap<NaiveDate, Message>,
    pub line_width: f32,

    font_size: PropertyFloat32,
    timestamp_font_size: PropertyFloat32,
    timestamp_width: PropertyFloat32,
    line_height: PropertyFloat32,
    msg_spacing: PropertyFloat32,
    baseline: PropertyFloat32,
    timestamp_color: PropertyColor,
    text_color: PropertyColor,
    nick_colors: PropertyPtr,
    hi_bg_color: PropertyColor,
    debug: PropertyBool,

    window_scale: PropertyFloat32,
    /// Used to detect if the window scale was changed when drawing.
    /// If it does then we must reload the glyphs too.
    old_window_scale: f32,

    render_api: RenderApi,
    text_shaper: TextShaperPtr,
}

impl MessageBuffer {
    pub fn new(
        font_size: PropertyFloat32,
        timestamp_font_size: PropertyFloat32,
        timestamp_width: PropertyFloat32,
        line_height: PropertyFloat32,
        msg_spacing: PropertyFloat32,
        baseline: PropertyFloat32,
        timestamp_color: PropertyColor,
        text_color: PropertyColor,
        nick_colors: PropertyPtr,
        hi_bg_color: PropertyColor,
        debug: PropertyBool,
        window_scale: PropertyFloat32,
        render_api: RenderApi,
        text_shaper: TextShaperPtr,
    ) -> Self {
        let old_window_scale = window_scale.get();
        Self {
            msgs: vec![],
            date_msgs: HashMap::new(),
            line_width: 0.,

            font_size,
            timestamp_font_size,
            timestamp_width,
            line_height,
            msg_spacing,
            baseline,
            timestamp_color,
            text_color,
            nick_colors,
            hi_bg_color,
            debug,

            window_scale,
            old_window_scale,

            render_api,
            text_shaper,
        }
    }

    pub fn clear(&mut self) {
        self.msgs.clear();
        self.date_msgs.clear();
    }

    pub fn adjust_window_scale(&mut self) {
        let window_scale = self.window_scale.get();
        if self.old_window_scale == window_scale {
            return
        }

        self.adjust_params();
    }

    /// This will force a reload of everything
    pub fn adjust_params(&mut self) {
        let window_scale = self.window_scale.get();
        let font_size = self.font_size.get();
        let timestamp_font_size = self.timestamp_font_size.get();
        let timestamp_width = self.timestamp_width.get();

        for msg in &mut self.msgs {
            msg.adjust_params(
                font_size,
                timestamp_font_size,
                window_scale,
                self.line_width,
                timestamp_width,
                &self.text_shaper,
                &self.render_api,
            );
        }
    }

    /// For scrolling we want to be able to adjust and measure without
    /// explicitly rendering since it may be off screen.
    pub fn adjust_width(&mut self, line_width: f32) {
        if (line_width - self.line_width).abs() < f32::EPSILON {
            return
        }
        self.line_width = line_width;

        let timestamp_width = self.timestamp_width.get();

        for msg in &mut self.msgs {
            msg.adjust_width(line_width, timestamp_width);
        }
    }

    /// Clear all meshes and caches.
    pub fn clear_meshes(&mut self) {
        for msg in &mut self.msgs {
            msg.clear_mesh();
        }
    }

    pub async fn calc_total_height(&mut self) -> f32 {
        let line_height = self.line_height.get();
        let baseline = self.baseline.get();
        let msg_spacing = self.msg_spacing.get();
        let mut height = 0.;

        let msgs = self.msgs_with_date();
        let mut msgs = pin!(msgs);

        let mut is_first = true;

        while let Some(msg) = msgs.next().await {
            if is_first {
                is_first = false;
            } else {
                height += msg_spacing;
            }

            height += msg.height(line_height);
        }

        // For the very top item. This is the ascent
        if !is_first {
            height += line_height - baseline;
        }

        height
    }

    fn find_privmsg_mut(&mut self, msg_id: &MessageId) -> Option<&mut PrivMessage> {
        for msg in &mut self.msgs {
            let Some(privmsg) = msg.get_privmsg_mut() else { continue };
            if privmsg.id == *msg_id {
                return Some(privmsg)
            }
        }
        None
    }
    pub fn mark_confirmed(&mut self, msg_id: &MessageId) -> bool {
        let Some(privmsg) = self.find_privmsg_mut(msg_id) else { return false };

        assert_eq!(privmsg.confirmed, false);
        privmsg.confirmed = true;
        privmsg.clear_mesh();

        return true
    }

    pub fn insert_privmsg(
        &mut self,
        timest: Timestamp,
        msg_id: MessageId,
        nick: String,
        text: String,
    ) -> Option<&mut PrivMessage> {
        t!("insert_privmsg({timest}, {msg_id}, {nick}, {text})");
        let font_size = self.font_size.get();
        let timestamp_font_size = self.timestamp_font_size.get();
        let timestamp_width = self.timestamp_width.get();
        let window_scale = self.window_scale.get();

        let msg = PrivMessage::new(
            font_size,
            timestamp_font_size,
            window_scale,
            timest,
            msg_id,
            nick,
            text,
            self.line_width,
            timestamp_width,
            &self.text_shaper,
            &self.render_api,
        );

        if self.msgs.is_empty() {
            self.msgs.push(msg);
            return self.msgs.last_mut().unwrap().get_privmsg_mut()
        }

        // We only add lines inside pages.
        // Calling the appropriate draw() function after should preload any missing pages.
        // When a line is before the first page, it will get preloaded as a new page.
        let oldest_timest = self.oldest_timestamp().unwrap();
        if timest < oldest_timest {
            return None
        }

        // Timestamps go from most recent backwards

        let mut idx = None;
        for (i, msg) in enumerate_mut(&mut self.msgs) {
            if timest >= msg.timestamp() {
                idx = Some(i);
                break
            }
        }

        let idx = match idx {
            Some(idx) => idx,
            None => {
                let last_page_idx = 0;
                last_page_idx
            }
        };

        self.msgs.insert(idx, msg);
        return self.msgs[idx].get_privmsg_mut()
    }

    pub fn push_privmsg(
        &mut self,
        timest: Timestamp,
        msg_id: MessageId,
        nick: String,
        text: String,
    ) -> f32 {
        //t!("push_privmsg({timest}, {msg_id}, {nick}, {text})");
        let font_size = self.font_size.get();
        let timestamp_font_size = self.timestamp_font_size.get();
        let timestamp_width = self.timestamp_width.get();
        let window_scale = self.window_scale.get();

        let msg = PrivMessage::new(
            font_size,
            timestamp_font_size,
            window_scale,
            timest,
            msg_id,
            nick,
            text,
            self.line_width,
            timestamp_width,
            &self.text_shaper,
            &self.render_api,
        );

        let msg_height = msg.height(self.line_height.get());

        if self.msgs.is_empty() {
            self.msgs.push(msg);
            return msg_height
        }

        self.msgs.push(msg);
        msg_height
    }

    /// Generate caches and return meshes
    pub async fn gen_meshes(&mut self, rect: &Rectangle, scroll: f32) -> Vec<(f32, DrawMesh)> {
        let line_height = self.line_height.get();
        let msg_spacing = self.msg_spacing.get();
        let baseline = self.baseline.get();
        let timestamp_width = self.timestamp_width.get();
        let debug_render = self.debug.get();

        let timest_color = self.timestamp_color.get();
        let text_color = self.text_color.get();
        let nick_colors = self.read_nick_colors();
        let hi_bg_color = self.hi_bg_color.get();

        let render_api = self.render_api.clone();

        let msgs = self.msgs_with_date();
        let mut msgs = pin!(msgs);

        let mut meshes = vec![];
        let mut current_pos = 0.;
        while let Some(msg) = msgs.next().await {
            let mesh_height = msg.height(line_height);
            let msg_bottom = current_pos;
            let msg_top = current_pos + mesh_height;

            if msg_bottom > scroll + rect.h {
                break
            }
            if msg_top < scroll {
                current_pos += msg_spacing;
                current_pos += mesh_height;
                continue
            }

            for mesh in msg.gen_mesh(
                rect,
                line_height,
                msg_spacing,
                baseline,
                timestamp_width,
                &nick_colors,
                timest_color,
                text_color,
                hi_bg_color,
                debug_render,
                &render_api,
            ) {
                meshes.push((current_pos, mesh));
            }

            current_pos += msg_spacing;
            current_pos += mesh_height;
        }

        //t!("gen_meshes() returning {} meshes", meshes.len());
        meshes
    }

    pub fn insert_filemsg(
        &mut self,
        timest: Timestamp,
        msg_id: MessageId,
        status: FileMessageStatus,
        nick: String,
        file_url: Url,
    ) -> Option<&mut FileMessage> {
        t!("insert_filemsg({timest}, {msg_id}, {nick}, {file_url})");
        let font_size = self.font_size.get();
        let window_scale = self.window_scale.get();

        let msg = FileMessage::new(
            font_size,
            window_scale,
            file_url,
            status,
            timest,
            nick,
            &self.text_shaper,
            &self.render_api,
        );

        // Timestamps go from most recent backwards
        let mut idx = None;
        for (i, msg) in enumerate_mut(&mut self.msgs) {
            if timest >= msg.timestamp() {
                idx = Some(i);
                break
            }
        }

        let idx = match idx {
            Some(idx) => idx,
            None => {
                let last_page_idx = 0;
                last_page_idx
            }
        };

        self.msgs.insert(idx, msg);
        self.msgs[idx].get_filemsg_mut()
    }

    /// Gets around borrow checker with unsafe
    fn msgs_with_date(&mut self) -> impl Stream<Item = &mut Message> {
        let font_size = self.font_size.get();
        let window_scale = self.window_scale.get();
        AsyncIter::from(async_gen! {
            let mut last_date = None;

            for idx in 0..self.msgs.len() {
                let msg = &mut self.msgs[idx] as *mut Message;
                let msg = unsafe { &mut *msg };
                let timest = msg.timestamp();

                let older_date = Local.timestamp_millis_opt(timest as i64).unwrap().date_naive();

                if let Some(newer_date) = last_date {
                    if newer_date != older_date {
                        let datemsg = self.get_date_msg(newer_date, font_size, window_scale);
                        let datemsg = unsafe { &mut *(datemsg as *mut Message) };
                        //t!("Adding date: {idx} {datemsg:?}");
                        yield datemsg;
                    }
                }
                last_date = Some(older_date);

                //t!("{idx} {msg:?}");
                yield msg;
            }

            if let Some(date) = last_date {
                let datemsg = self.get_date_msg(date, font_size, window_scale);
                let datemsg = unsafe { &mut *(datemsg as *mut Message) };
                yield datemsg;
            }
        })
    }

    fn get_date_msg(&mut self, date: NaiveDate, font_size: f32, window_scale: f32) -> &mut Message {
        let dt = date.and_hms_opt(0, 0, 0).unwrap();
        let timest = Local.from_local_datetime(&dt).unwrap().timestamp_millis() as u64;

        if !self.date_msgs.contains_key(&date) {
            let datemsg = DateMessage::new(
                font_size,
                window_scale,
                timest,
                &self.text_shaper,
                &self.render_api,
            );
            self.date_msgs.insert(date, datemsg);
        }

        self.date_msgs.get_mut(&date).unwrap()
    }

    pub fn oldest_timestamp(&self) -> Option<Timestamp> {
        let last_msg = &self.msgs.last()?;
        Some(last_msg.timestamp())
    }

    fn read_nick_colors(&self) -> Vec<Color> {
        let mut colors = vec![];
        let mut color = [0f32; 4];
        for i in 0..self.nick_colors.get_len() {
            color[i % 4] = self.nick_colors.get_f32(i).expect("prop logic err");

            if i > 0 && i % 4 == 0 {
                let color = std::mem::take(&mut color);
                colors.push(color);
            }
        }
        colors
    }

    pub async fn select_line(&mut self, y: f32) {
        let line_height = self.line_height.get();
        let msg_spacing = self.msg_spacing.get();

        let msgs = self.msgs_with_date();
        let mut msgs = pin!(msgs);

        let mut current_pos = 0.;
        while let Some(msg) = msgs.next().await {
            let mesh_height = msg.height(line_height);
            let msg_bottom = current_pos;
            let msg_top = current_pos + mesh_height + msg_spacing;

            if msg_bottom <= y && y <= msg_top {
                // Do nothing
                if msg.is_date() {
                    break
                }

                msg.select();

                msg.clear_mesh();
                break
            }

            current_pos += msg_spacing;
            current_pos += mesh_height;
        }
    }

    pub async fn update_file(&mut self, data: &Vec<u8>) {
        let mut cur = Cursor::new(data);
        let hash = String::decode(&mut cur).unwrap();
        let status = String::decode(&mut cur).unwrap();

        let status = match status.as_str() {
            "downloading" => {
                let progress = f32::decode(&mut cur).unwrap();
                FileMessageStatus::Downloading { progress }
            }
            "downloaded" => {
                let path = String::decode(&mut cur).unwrap();
                FileMessageStatus::Downloaded { path }
            }
            "error" => {
                let msg = String::decode(&mut cur).unwrap();
                FileMessageStatus::Error { msg }
            }
            _ => FileMessageStatus::Initializing,
        };

        // TODO: keep a cache of file messages somewhere to avoid looping
        // over all messages
        for msg in &mut self.msgs {
            if let Some(filemsg) = msg.get_filemsg_mut() {
                if filemsg.file_url.host_str() == Some(&hash) {
                    filemsg.set_status(&status);
                    filemsg.adjust_width(self.line_width, self.timestamp_width.get());
                    filemsg.clear_mesh();
                }
            }
        }
    }
}
