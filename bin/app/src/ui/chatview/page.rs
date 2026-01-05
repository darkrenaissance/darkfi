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

use super::{MessageId, Timestamp};
use crate::{
    gfx::{gfxtag, DrawInstruction, ManagedTexturePtr, Point, Rectangle, RenderApi},
    mesh::{Color, MeshBuilder, COLOR_CYAN, COLOR_GREEN, COLOR_RED, COLOR_WHITE},
    prop::{PropertyBool, PropertyColor, PropertyFloat32, PropertyPtr},
    text,
    util::enumerate_mut,
};

macro_rules! t { ($($arg:tt)*) => { trace!(target: "ui::chatview::message_buffer", $($arg)*); } }

//const PAGE_SIZE: usize = 10;
//const PRELOAD_PAGES: usize = 10;

const UNCONF_COLOR: [f32; 4] = [0.4, 0.4, 0.4, 1.];

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

    mesh_cache: Option<Vec<DrawInstruction>>,
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
    ) -> Message {
        if nick == "NOTICE" {
            font_size *= 0.8;
        }

        Message::Priv(Self {
            font_size,
            timestamp_font_size,
            window_scale,
            timestamp,
            id,
            nick,
            text,
            confirmed: true,
            is_selected: false,
            mesh_cache: None,
        })
    }

    fn gen_timestr(timestamp: Timestamp) -> String {
        let dt = Local.timestamp_millis_opt(timestamp as i64).unwrap();
        let timestr = dt.format("%H:%M").to_string();
        timestr
    }

    fn height(&self, line_height: f32) -> f32 {
        // TODO: calculate based on actual layout height
        // For now, assume a single line
        line_height
    }

    async fn gen_mesh(
        &mut self,
        clip: &Rectangle,
        line_height: f32,
        msg_spacing: f32,
        timestamp_width: f32,
        nick_colors: &[Color],
        timestamp_color: Color,
        text_color: Color,
        hi_bg_color: Color,
        render_api: &RenderApi,
    ) -> Vec<DrawInstruction> {
        if let Some(instrs) = &self.mesh_cache {
            return instrs.clone()
        }

        let mut all_instrs = vec![DrawInstruction::Move(Point::new(0., -line_height))];

        // Draw selection background if selected
        if self.is_selected {
            let height = self.height(line_height) + msg_spacing;
            let mut mesh = MeshBuilder::new(gfxtag!("chatview_privmsg_sel"));
            mesh.draw_filled_box(
                &Rectangle { x: 0., y: -height, w: clip.w, h: height },
                hi_bg_color,
            );
            all_instrs
                .push(DrawInstruction::Draw(mesh.alloc(render_api).draw_with_textures(vec![])));
        }

        let mut txt_ctx = text::TEXT_CTX.get().await;

        // Timestamp layout
        let timestr = Self::gen_timestr(self.timestamp);
        let timestamp_layout = txt_ctx.make_layout(
            &timestr,
            timestamp_color,
            self.timestamp_font_size,
            line_height / self.timestamp_font_size,
            self.window_scale,
            None,
            &[],
        );

        // Message text layout
        let linetext = if self.nick == "NOTICE" {
            self.text.clone()
        } else {
            format!("{} {}", self.nick, self.text)
        };

        let nick_color = select_nick_color(&self.nick, nick_colors);

        let text_layout = if self.nick == "NOTICE" {
            txt_ctx.make_layout(
                &linetext,
                text_color,
                self.font_size,
                line_height / self.font_size,
                self.window_scale,
                Some(clip.w - timestamp_width),
                &[],
            )
        } else {
            let body_color = if self.confirmed { text_color } else { UNCONF_COLOR };
            let nick_end = self.nick.len() + 1;
            txt_ctx.make_layout2(
                &linetext,
                body_color,
                self.font_size,
                line_height / self.font_size,
                self.window_scale,
                Some(clip.w - timestamp_width),
                &[],
                &[(0..nick_end, nick_color)],
            )
        };

        // Render timestamp
        let timestamp_instrs =
            text::render_layout(&timestamp_layout, render_api, gfxtag!("chatview_privmsg_ts"));
        all_instrs.extend(timestamp_instrs);

        // Render message text offset by timestamp_width
        all_instrs.push(DrawInstruction::Move(Point::new(timestamp_width, 0.)));
        let text_instrs =
            text::render_layout(&text_layout, render_api, gfxtag!("chatview_privmsg_text"));
        all_instrs.extend(text_instrs);

        self.mesh_cache = Some(all_instrs.clone());
        all_instrs
    }

    fn adjust_params(&mut self, font_size: f32, timestamp_font_size: f32, window_scale: f32) {
        let font_size = if self.nick == "NOTICE" { font_size * 0.8 } else { font_size };
        self.font_size = font_size;
        self.timestamp_font_size = timestamp_font_size;
        self.window_scale = window_scale;
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
    mesh_cache: Option<Vec<DrawInstruction>>,
}

impl DateMessage {
    pub fn new(font_size: f32, window_scale: f32, timestamp: Timestamp) -> Message {
        let timestamp = Self::timest_to_midnight(timestamp);
        Message::Date(Self { font_size, window_scale, timestamp, mesh_cache: None })
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

    fn adjust_params(&mut self, font_size: f32, window_scale: f32) {
        self.font_size = font_size;
        self.window_scale = window_scale;
        self.mesh_cache = None;
    }

    fn clear_mesh(&mut self) {
        self.mesh_cache = None;
    }

    async fn gen_mesh(
        &mut self,
        line_height: f32,
        timestamp_color: Color,
        render_api: &RenderApi,
    ) -> Vec<DrawInstruction> {
        // Return cached mesh if available
        if let Some(cache) = &self.mesh_cache {
            return cache.clone()
        }

        let datestr = Self::datestr(self.timestamp);

        let mut txt_ctx = text::TEXT_CTX.get().await;
        let layout = txt_ctx.make_layout(
            &datestr,
            timestamp_color,
            self.font_size,
            line_height / self.font_size,
            self.window_scale,
            None,
            &[],
        );
        drop(txt_ctx);

        let mut txt_instrs = text::render_layout(&layout, render_api, gfxtag!("chatview_datemsg"));
        let mut instrs = Vec::with_capacity(1 + txt_instrs.len());
        instrs.push(DrawInstruction::Move(Point::new(0., -line_height)));
        instrs.append(&mut txt_instrs);

        // Cache the instructions
        self.mesh_cache = Some(instrs.clone());
        instrs
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

    mesh_cache: Option<Vec<DrawInstruction>>,
}

impl FileMessage {
    // This is not portable across devices and will break
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
    ) -> Message {
        Message::File(Self {
            font_size,
            window_scale,
            max_width: 0.,
            file_url,
            status,
            imgbuf: Arc::new(SyncMutex::new(None)),
            timestamp,
            mesh_cache: None,
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

    fn adjust_params(&mut self, font_size: f32, window_scale: f32) {
        self.font_size = font_size;
        self.window_scale = window_scale;
        self.mesh_cache = None;
    }

    fn clear_mesh(&mut self) {
        self.mesh_cache = None;
    }

    fn get_img_size(&self, imgbuf: &ImageBuffer<Rgba<u8>, Vec<u8>>) -> (f32, f32) {
        let img_w = imgbuf.width() as f32;
        let img_h = imgbuf.height() as f32;

        let width_scale = (self.max_width - Self::GLOW_SIZE) / img_w;
        let height_scale = Self::IMG_MAX_HEIGHT / img_h;

        let scale = width_scale.min(height_scale);
        (img_w * scale, img_h * scale)
    }

    async fn gen_mesh(
        &mut self,
        clip: &Rectangle,
        line_height: f32,
        timestamp_width: f32,
        timestamp_color: Color,
        render_api: &RenderApi,
    ) -> Vec<DrawInstruction> {
        if let Some(instrs) = &self.mesh_cache {
            return instrs.clone()
        }

        self.max_width = clip.w - timestamp_width;

        // Extract image size while holding lock, then drop it
        let mut img_size = None;
        if let Some(img) = &*self.imgbuf.lock() {
            img_size = Some(self.get_img_size(img));
        }

        // Lock is dropped here, safe to await now
        if let Some((img_w, img_h)) = img_size {
            let mesh_rect =
                Rectangle::from([timestamp_width, -img_h - Self::MARGIN_BOTTOM, img_w, img_h]);
            let texture = self.load_texture(render_api);
            let mut mesh_gradient = MeshBuilder::new(gfxtag!("file_gradient"));
            let glow_color = [timestamp_color[0], timestamp_color[1], timestamp_color[2], 0.5];
            mesh_gradient.draw_box_shadow(&mesh_rect, glow_color, Self::GLOW_SIZE);

            let mesh_gradient = mesh_gradient.alloc(render_api);
            let mut instrs = vec![DrawInstruction::Draw(mesh_gradient.draw_untextured())];

            let mut mesh_img = MeshBuilder::new(gfxtag!("file_img"));
            let uv_rect = Rectangle::from([0., 0., 1., 1.]);
            mesh_img.draw_box(&mesh_rect, COLOR_WHITE, &uv_rect);
            let mesh_img = mesh_img.alloc(render_api);
            instrs.push(DrawInstruction::Draw(mesh_img.draw_with_textures(vec![texture])));

            self.mesh_cache = Some(instrs.clone());
            // Image is downloaded so return
            return instrs;
        }

        // Image is not downloaded yet

        let mut all_instrs = vec![];

        // Draw background box

        let color = match self.status {
            FileMessageStatus::Initializing => timestamp_color,
            FileMessageStatus::Downloading { .. } => COLOR_CYAN,
            FileMessageStatus::Downloaded { .. } => COLOR_GREEN,
            FileMessageStatus::Error { .. } => COLOR_RED,
        };
        let box_height = 2. * line_height + Self::BOX_PADDING_TOP + Self::BOX_PADDING_BOTTOM;

        let mut mesh = MeshBuilder::new(gfxtag!("chatview_filemsg_box"));
        let box_width = self.max_width + Self::BOX_PADDING_X * 2.;
        let mesh_rect = Rectangle::new(
            timestamp_width,
            -box_height + Self::MARGIN_BOTTOM,
            box_width,
            box_height,
        );
        mesh.draw_outline(&mesh_rect, color, 1.);

        let glow_color = [color[0], color[1], color[2], 0.3];
        mesh.draw_box_shadow(&mesh_rect, glow_color, Self::GLOW_SIZE);
        let mesh = mesh.alloc(render_api);

        all_instrs.push(DrawInstruction::Draw(mesh.draw_untextured()));

        let file_strs = Self::filestr(&self.file_url, &self.status);

        let mut layouts = Vec::with_capacity(file_strs.len());
        let mut txt_ctx = text::TEXT_CTX.get().await;
        for file_str in &file_strs {
            let layout = txt_ctx.make_layout(
                file_str,
                color,
                self.font_size,
                line_height / self.font_size,
                self.window_scale,
                Some(self.max_width),
                &[],
            );
            layouts.push(layout);
        }
        drop(txt_ctx);

        all_instrs
            .push(DrawInstruction::Move(Point::new(timestamp_width + Self::BOX_PADDING_X, 0.)));
        for layout in layouts {
            all_instrs.push(DrawInstruction::Move(Point::new(0., -line_height)));

            let instrs =
                text::render_layout(&layout, render_api, gfxtag!("chatview_filemsg_text"));
            all_instrs.extend(instrs);
        }

        self.mesh_cache = Some(all_instrs.clone());
        all_instrs
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
        // If image is downloaded, return image height plus margins
        if let Some(buf) = &*imgbuf {
            let img_height = self.get_img_size(buf).1;
            return img_height + Self::MARGIN_TOP + Self::MARGIN_BOTTOM;
        }
        drop(imgbuf);

        // No image yet, so calculate height for text box
        // filestr() always returns 2 lines: [file_hash, status_string]
        2. * line_height
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

    fn adjust_params(&mut self, font_size: f32, timestamp_font_size: f32, window_scale: f32) {
        match self {
            Self::Priv(m) => m.adjust_params(font_size, timestamp_font_size, window_scale),
            Self::Date(m) => m.adjust_params(font_size, window_scale),
            Self::File(m) => m.adjust_params(font_size, window_scale),
        }
    }

    fn clear_mesh(&mut self) {
        match self {
            Self::Priv(m) => m.clear_mesh(),
            Self::Date(m) => m.clear_mesh(),
            Self::File(m) => m.clear_mesh(),
        }
    }

    async fn gen_mesh(
        &mut self,
        clip: &Rectangle,
        line_height: f32,
        msg_spacing: f32,
        timestamp_width: f32,
        nick_colors: &[Color],
        timestamp_color: Color,
        text_color: Color,
        hi_bg_color: Color,
        render_api: &RenderApi,
    ) -> Vec<DrawInstruction> {
        match self {
            Self::Priv(m) => {
                m.gen_mesh(
                    clip,
                    line_height,
                    msg_spacing,
                    timestamp_width,
                    nick_colors,
                    timestamp_color,
                    text_color,
                    hi_bg_color,
                    render_api,
                )
                .await
            }
            Self::Date(m) => m.gen_mesh(line_height, timestamp_color, render_api).await,
            Self::File(m) => {
                m.gen_mesh(clip, line_height, timestamp_width, timestamp_color, render_api).await
            }
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
            msg.adjust_params(font_size, timestamp_font_size, window_scale);
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
        );

        let msg_height = msg.height(self.line_height.get());

        if self.msgs.is_empty() {
            self.msgs.push(msg);
            return msg_height
        }

        self.msgs.push(msg);
        msg_height
    }

    /// Generate caches and return draw instructions
    pub async fn gen_meshes(
        &mut self,
        rect: &Rectangle,
        scroll: f32,
    ) -> Vec<(f32, Vec<DrawInstruction>)> {
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

            let instrs = msg
                .gen_mesh(
                    rect,
                    line_height,
                    msg_spacing,
                    timestamp_width,
                    &nick_colors,
                    timest_color,
                    text_color,
                    hi_bg_color,
                    &render_api,
                )
                .await;

            meshes.push((current_pos, instrs));

            current_pos += msg_spacing;
            current_pos += mesh_height;
        }
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

        let msg = FileMessage::new(font_size, window_scale, file_url, status, timest);

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
            let datemsg = DateMessage::new(font_size, window_scale, timest);
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
                    filemsg.clear_mesh();
                }
            }
        }
    }
}
