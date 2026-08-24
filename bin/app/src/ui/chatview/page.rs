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
use darkfi_serial::{Encodable, FutAsyncWriteExt, SerialDecodable, SerialEncodable};
use futures::stream::{Stream, StreamExt};
use image::{ImageBuffer, ImageReader, Rgba};
use miniquad::{MouseButton, TextureFormat, TouchPhase};
use parking_lot::Mutex as SyncMutex;
use regex::Regex;
use std::{
    collections::HashMap,
    hash::{DefaultHasher, Hash, Hasher},
    io::Cursor,
    ops::Range,
    pin::pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, LazyLock,
    },
};
use url::Url;

use super::{MessageId, Timestamp};
use crate::{
    gfx::{gfxtag, DrawInstruction, ManagedTexturePtr, Point, Rectangle, RenderApi, Renderer},
    mesh::{Color, MeshBuilder, COLOR_CYAN, COLOR_GREEN, COLOR_RED, COLOR_WHITE},
    prop::{PropertyColor, PropertyFloat32, PropertyPtr},
    scene::SceneNodeWeak,
    text,
    ui::UIObject,
    util::enumerate_mut,
};

macro_rules! t { ($($arg:tt)*) => { trace!(target: "ui::chatview::message_buffer", $($arg)*); } }

//const PAGE_SIZE: usize = 10;
//const PRELOAD_PAGES: usize = 10;

const UNCONF_COLOR: [f32; 4] = [0.4, 0.4, 0.4, 1.];

static URL_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"https?://[^\s]+|fud://[^\s]+|www\.[^\s]+").unwrap());

fn url_color_ranges(text: &str, offset: usize, color: Color) -> Vec<(Range<usize>, Color)> {
    URL_REGEX.find_iter(text).map(|m| (m.start() + offset..m.end() + offset, color)).collect()
}

/// IRC CTCP ACTION framing prefix, e.g. `\x01ACTION waves\x01`.
const CTCP_ACTION_PREFIX: &str = "\u{1}ACTION ";

/// If `text` is a CTCP ACTION-framed message body, return the stripped
/// action text. A single trailing `\x01` delimiter is stripped when present
/// but is not required, since bodies truncated by length limits may lose it.
fn parse_ctcp_action(text: &str) -> Option<&str> {
    let body = text.strip_prefix(CTCP_ACTION_PREFIX)?;
    let body = body.strip_suffix('\u{1}').unwrap_or(body);
    Some(body)
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

    /// Whether this is an IRC-style CTCP ACTION message (`/me`).
    /// Detected in `PrivMessage::new`; `text` holds the stripped action text.
    is_action: bool,

    is_selected: bool,

    mesh_cache: Option<Vec<DrawInstruction>>,
    txt_layout: Option<text::TextLayout>,

    /// Bounding rects of this message's URL runs in message-local coordinates,
    /// each tagged with its URL string. Populated in `gen_mesh`, used by
    /// `handle_mouse_btn_up` for click hit-testing. Cleared in `clear_mesh`.
    url_click_rects: Vec<(String, Rectangle)>,
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

        let (is_action, text) = match parse_ctcp_action(&text) {
            Some(action) => (true, action.to_string()),
            None => (false, text),
        };

        Message::Priv(Self {
            font_size,
            timestamp_font_size,
            window_scale,
            timestamp,
            id,
            nick,
            text,
            confirmed: true,
            is_action,
            is_selected: false,
            mesh_cache: None,
            txt_layout: None,
            url_click_rects: vec![],
        })
    }

    fn gen_timestr(timestamp: Timestamp) -> String {
        let dt = Local.timestamp_millis_opt(timestamp as i64).unwrap();
        let timestr = dt.format("%H:%M").to_string();
        timestr
    }

    fn height(&self, _line_height: f32) -> f32 {
        self.txt_layout.as_ref().unwrap().height()
    }

    /// The full rendered line text: NOTICE renders the body alone, normal
    /// messages render "<nick> <body>", and actions render "* <nick> <body>".
    fn line_text(&self) -> String {
        if self.nick == "NOTICE" {
            return self.text.clone()
        }
        if self.is_action {
            return format!("* {} {}", self.nick, self.text)
        }
        format!("{} {}", self.nick, self.text)
    }

    /// Byte offset of the body within the rendered line text. This is also
    /// the end of the nick-colored prefix.
    fn body_offset(&self) -> usize {
        if self.nick == "NOTICE" {
            return 0
        }
        if self.is_action {
            // "* " + nick + " "
            return self.nick.len() + 3
        }
        self.nick.len() + 1
    }

    fn cache_txt_layout(
        &mut self,
        clip: &Rectangle,
        line_height: f32,
        timestamp_width: f32,
        nick_colors: &[Color],
        text_color: Color,
        action_text_color: Color,
        url_text_color: Color,
    ) {
        if self.txt_layout.is_some() {
            return
        }

        let linetext = self.line_text();

        let nick_color = select_nick_color(&self.nick, nick_colors);

        let is_notice = self.nick == "NOTICE";
        let body_offset = self.body_offset();
        let url_ranges = url_color_ranges(&self.text, body_offset, url_text_color);

        let txt_layout = if is_notice {
            text::make_layout2(
                &linetext,
                text_color,
                self.font_size,
                line_height / self.font_size,
                self.window_scale,
                Some(clip.w - timestamp_width),
                &[],
                &url_ranges,
                "start",
                "normal",
            )
        } else {
            let body_color = if self.is_action {
                if self.confirmed {
                    action_text_color
                } else {
                    UNCONF_COLOR
                }
            } else if self.confirmed {
                text_color
            } else {
                UNCONF_COLOR
            };
            let mut foreground_colors = vec![(0..body_offset, nick_color)];
            foreground_colors.extend(url_ranges);
            text::make_layout2(
                &linetext,
                body_color,
                self.font_size,
                line_height / self.font_size,
                self.window_scale,
                Some(clip.w - timestamp_width),
                &[],
                &foreground_colors,
                "start",
                "normal",
            )
        };
        self.txt_layout = Some(txt_layout);
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
        action_text_color: Color,
        url_text_color: Color,
        url_bg_color: Color,
        url_bg_border_size: f32,
        url_bg_border_color: Color,
        hi_bg_color: Color,
        renderer: &Renderer,
    ) -> Vec<DrawInstruction> {
        if let Some(instrs) = &self.mesh_cache {
            assert!(self.txt_layout.is_some());
            return instrs.clone()
        }

        // Timestamp layout
        let timestr = Self::gen_timestr(self.timestamp);
        let timestamp_layout = text::make_layout(
            &timestr,
            timestamp_color,
            self.timestamp_font_size,
            line_height / self.timestamp_font_size,
            self.window_scale,
            None,
            &[],
        );

        self.cache_txt_layout(
            clip,
            line_height,
            timestamp_width,
            nick_colors,
            text_color,
            action_text_color,
            url_text_color,
        );

        let mut all_instrs = vec![];

        // Draw selection background if selected
        if self.is_selected {
            let height = self.height(line_height) + msg_spacing;
            let mut mesh = MeshBuilder::new(gfxtag!("chatview_privmsg_sel"));
            mesh.draw_filled_box(&Rectangle { x: 0., y: 0., w: clip.w, h: height }, hi_bg_color);
            all_instrs.push(DrawInstruction::Draw(mesh.alloc(renderer).draw_untextured()));
        }

        // Render timestamp
        let timestamp_instrs =
            text::render_layout(&timestamp_layout, renderer, gfxtag!("chatview_privmsg_ts"));
        all_instrs.extend(timestamp_instrs);

        // Render message text offset by timestamp_width
        all_instrs.push(DrawInstruction::Move(Point::new(timestamp_width, 0.)));

        // Draw URL background (and optional border) behind the URL runs, under the
        // glyphs. render_backgrounds matches glyph runs by their style brush, so
        // only the URL-colored runs (brush == url_text_color) get a box — never the
        // nick or surrounding body text.
        if URL_REGEX.is_match(&self.text) {
            let bg_instrs = text::render_backgrounds(
                self.txt_layout.as_ref().unwrap(),
                url_text_color,
                url_bg_color,
                url_bg_border_color,
                url_bg_border_size,
                renderer,
                gfxtag!("chatview_privmsg_urlbg"),
            );
            all_instrs.extend(bg_instrs);
        }

        // Record this message's URL hit-rectangles for click detection.
        self.url_click_rects = self.compute_url_click_rects(timestamp_width, url_text_color);

        let text_instrs = text::render_layout(
            self.txt_layout.as_ref().unwrap(),
            renderer,
            gfxtag!("chatview_privmsg_text"),
        );
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
        self.txt_layout = None;
        self.url_click_rects.clear();
    }

    fn select(&mut self) {
        self.is_selected = true;
    }

    fn deselect(&mut self) {
        self.is_selected = false;
    }

    fn is_selected(&self) -> bool {
        self.is_selected
    }

    /// Build the URL hit-rectangles for this message, in message-local
    /// virtual coordinates. Each URL-colored glyph run (`style().brush ==
    /// url_text_color`) becomes a rect `(timestamp_width + run.offset,
    /// run.baseline - ascent, run.advance, ascent + descent)`. The run is
    /// tagged with its URL string by intersecting its (coarse) font-run
    /// `text_range()` with the message's URL byte ranges in `linetext`,
    /// so wrapped URLs and multiple URLs are handled correctly.
    fn compute_url_click_rects(
        &self,
        timestamp_width: f32,
        url_text_color: Color,
    ) -> Vec<(String, Rectangle)> {
        let mut rects = vec![];
        let Some(layout) = self.txt_layout.as_ref() else { return rects };

        let linetext = self.line_text();
        let body_offset = self.body_offset();

        // URL byte ranges within linetext (the color value is unused here).
        let url_ranges: Vec<Range<usize>> =
            url_color_ranges(&self.text, body_offset, url_text_color)
                .into_iter()
                .map(|(r, _)| r)
                .collect();
        if url_ranges.is_empty() {
            return rects
        }

        // Layout coordinates are physical so divide by the scale to get
        // the virtual units the hit test positions use.
        let scale = layout.scale();
        for line in layout.lines() {
            for item in line.items() {
                let parley::PositionedLayoutItem::GlyphRun(glyph_run) = item else { continue };
                if glyph_run.style().brush != url_text_color {
                    continue
                }

                // Map this run to its URL via the (coarse) font-run range intersected
                // with the URL byte ranges.
                let font_range = glyph_run.run().text_range();
                let Some(url_range) = url_ranges
                    .iter()
                    .find(|r| r.start < font_range.end && r.end > font_range.start)
                else {
                    continue
                };
                let url_str = linetext[url_range.clone()].to_string();

                let metrics = glyph_run.run().metrics();
                let x = timestamp_width + glyph_run.offset() / scale;
                let y = (glyph_run.baseline() - metrics.ascent) / scale;
                let w = glyph_run.advance() / scale;
                let h = (metrics.ascent + metrics.descent) / scale;
                rects.push((url_str, Rectangle::new(x, y, w, h)));
            }
        }

        rects
    }

    fn url_at_local(&self, pos: Point) -> Option<&str> {
        for (url, rect) in &self.url_click_rects {
            if rect.contains(pos) {
                return Some(url.as_str())
            }
        }
        None
    }

    async fn handle_mouse_btn_up(&self, btn: MouseButton, mouse_pos: Point) -> bool {
        if btn != MouseButton::Left {
            return false
        }
        let Some(url) = self.url_at_local(mouse_pos) else { return false };
        info!(target: "ui::chatview", "URL clicked: {url}");

        #[cfg(target_os = "android")]
        crate::android::open_url(url);

        #[cfg(not(target_os = "android"))]
        let _ = open::that(url);

        true
    }

    async fn handle_touch(&self, phase: TouchPhase, touch_pos: Point) -> bool {
        if phase != TouchPhase::Ended {
            return false
        }
        let Some(url) = self.url_at_local(touch_pos) else { return false };
        info!(target: "ui::chatview", "URL tapped: {url}");

        #[cfg(target_os = "android")]
        crate::android::open_url(url);

        #[cfg(not(target_os = "android"))]
        let _ = open::that(url);

        true
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
        renderer: &Renderer,
    ) -> Vec<DrawInstruction> {
        // Return cached mesh if available
        if let Some(cache) = &self.mesh_cache {
            return cache.clone()
        }

        let datestr = Self::datestr(self.timestamp);

        let layout = text::make_layout(
            &datestr,
            timestamp_color,
            self.font_size,
            line_height / self.font_size,
            self.window_scale,
            None,
            &[],
        );

        let instrs = text::render_layout(&layout, renderer, gfxtag!("chatview_datemsg"));
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

#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub enum FileMessageStatus {
    Initializing,
    Idle,
    Downloading { progress: f32 },
    Downloaded { path: String },
    Error { msg: String, progress: f32 },
}

type GenericImageBuffer = ImageBuffer<Rgba<u8>, Vec<u8>>;

pub struct FileMessage {
    chatview_node: SceneNodeWeak,

    font_size: f32,
    window_scale: f32,
    max_width: f32,

    file_url: Url,
    pub status: FileMessageStatus,
    imgbuf: Arc<SyncMutex<Option<GenericImageBuffer>>>,
    timestamp: Timestamp,

    active_rect: Option<Rectangle>,
    mouse_btn_held: AtomicBool,

    mesh_cache: Option<Vec<DrawInstruction>>,
}

impl FileMessage {
    // This is not portable across devices and will break
    const GLOW_SIZE: f32 = 20.;
    const MARGIN_TOP: f32 = 4.;
    const MARGIN_BOTTOM: f32 = 10.;
    const BOX_PADDING_Y: f32 = 12.;
    const BOX_PADDING_X: f32 = 15.;
    const IMG_MAX_HEIGHT: f32 = 500.;

    pub fn new(
        chatview_node: SceneNodeWeak,

        font_size: f32,
        window_scale: f32,

        file_url: Url,
        status: FileMessageStatus,
        timestamp: Timestamp,
    ) -> Message {
        Message::File(Self {
            chatview_node,
            font_size,
            window_scale,
            max_width: 0.,
            file_url,
            status,
            imgbuf: Arc::new(SyncMutex::new(None)),
            timestamp,
            active_rect: None,
            mouse_btn_held: AtomicBool::new(false),
            mesh_cache: None,
        })
    }

    fn filestr(file_url: &Url, status: &FileMessageStatus) -> Vec<String> {
        let status_str = match status {
            FileMessageStatus::Initializing => "starting fud".to_string(),
            FileMessageStatus::Idle => "tap to download".to_string(),
            FileMessageStatus::Downloading { progress } => format!("downloading [{progress:.1}%]"),
            FileMessageStatus::Downloaded { .. } => "downloaded".to_string(),
            FileMessageStatus::Error { msg, progress } => {
                if *progress > 0. {
                    format!("{} [{progress:.1}%]", msg.to_lowercase())
                } else {
                    msg.to_lowercase()
                }
            }
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

        let width_scale = self.max_width / img_w;
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
        renderer: &Renderer,
    ) -> Vec<DrawInstruction> {
        if let Some(instrs) = &self.mesh_cache {
            return instrs.clone()
        }

        self.max_width = clip.w - timestamp_width - Self::GLOW_SIZE;

        // Extract image size while holding lock, then drop it
        let mut img_size = None;
        if let Some(img) = &*self.imgbuf.lock() {
            img_size = Some(self.get_img_size(img));
        }

        // Lock is dropped here, safe to await now
        if let Some((img_w, img_h)) = img_size {
            let mesh_rect = Rectangle::from([timestamp_width, Self::MARGIN_TOP, img_w, img_h]);
            let texture = self.load_texture(renderer);
            let mut mesh_gradient = MeshBuilder::new(gfxtag!("file_gradient"));
            let glow_color = [timestamp_color[0], timestamp_color[1], timestamp_color[2], 0.5];
            mesh_gradient.draw_box_shadow(&mesh_rect, glow_color, Self::GLOW_SIZE);
            self.active_rect = Some(mesh_rect);

            let mesh_gradient = mesh_gradient.alloc(renderer);
            let mut instrs = vec![DrawInstruction::Draw(mesh_gradient.draw_untextured())];

            let mut mesh_img = MeshBuilder::new(gfxtag!("file_img"));
            let uv_rect = Rectangle::from([0., 0., 1., 1.]);
            mesh_img.draw_box(&mesh_rect, COLOR_WHITE, &uv_rect);
            let mesh_img = mesh_img.alloc(renderer);
            instrs.push(DrawInstruction::Draw(mesh_img.draw_with_textures(vec![texture])));

            self.mesh_cache = Some(instrs.clone());
            // Image is downloaded so return
            return instrs;
        }

        // File is not an image, or the image is not downloaded yet

        let mut all_instrs = vec![];

        let color = match self.status {
            FileMessageStatus::Initializing => timestamp_color,
            FileMessageStatus::Idle => timestamp_color,
            FileMessageStatus::Downloading { .. } => COLOR_CYAN,
            FileMessageStatus::Downloaded { .. } => COLOR_GREEN,
            FileMessageStatus::Error { .. } => COLOR_RED,
        };

        // Compute text

        let file_strs = Self::filestr(&self.file_url, &self.status);
        let mut layouts = Vec::with_capacity(file_strs.len());
        let mut text_width = 0.;
        for file_str in &file_strs {
            let layout = text::make_layout(
                file_str,
                color,
                self.font_size,
                line_height / self.font_size,
                self.window_scale,
                Some(self.max_width),
                &[],
            );
            if layout.width() > text_width {
                text_width = layout.width();
            }
            layouts.push(layout);
        }

        // Draw background box

        let box_height = 2. * line_height + Self::BOX_PADDING_Y * 2.;

        let mut mesh = MeshBuilder::new(gfxtag!("chatview_filemsg_box"));
        let box_width = if text_width > self.max_width { self.max_width } else { text_width } +
            Self::BOX_PADDING_X * 2.;
        let mesh_rect = Rectangle::new(timestamp_width, Self::MARGIN_TOP, box_width, box_height);
        mesh.draw_outline(&mesh_rect, color, 1.);
        self.active_rect = Some(mesh_rect);

        let glow_color = [color[0], color[1], color[2], 0.3];
        mesh.draw_box_shadow(&mesh_rect, glow_color, Self::GLOW_SIZE);
        let mesh = mesh.alloc(renderer);

        all_instrs.push(DrawInstruction::Draw(mesh.draw_untextured()));

        // Draw text

        all_instrs.push(DrawInstruction::Move(Point::new(
            timestamp_width + Self::BOX_PADDING_X,
            Self::MARGIN_TOP + Self::BOX_PADDING_Y,
        )));
        for layout in layouts {
            let instrs = text::render_layout(&layout, renderer, gfxtag!("chatview_filemsg_text"));
            all_instrs.extend(instrs);
            all_instrs.push(DrawInstruction::Move(Point::new(0., line_height)));
        }

        self.mesh_cache = Some(all_instrs.clone());
        all_instrs
    }

    fn load_img(&self) -> Option<ImageBuffer<Rgba<u8>, Vec<u8>>> {
        if let FileMessageStatus::Downloaded { path } = &self.status {
            let data = Arc::new(SyncMutex::new(vec![]));
            let data2 = data.clone();
            miniquad::fs::load_file(path.as_str(), move |res| match res {
                Ok(res) => *data2.lock() = res,
                Err(_) => {}
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

    fn load_texture(&self, renderer: &Renderer) -> ManagedTexturePtr {
        let imgbuf = self.imgbuf.lock();
        let img = imgbuf.as_ref().unwrap();

        let width = img.width() as u16;
        let height = img.height() as u16;
        let bmp = img.as_raw().clone();
        drop(imgbuf);

        renderer.new_texture(width, height, bmp, TextureFormat::RGBA8, gfxtag!("file_img_texture"))
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
        2. * line_height + Self::BOX_PADDING_Y * 2. + Self::MARGIN_TOP + Self::MARGIN_BOTTOM
    }

    fn select(&mut self) {}

    async fn download(&self) {
        let node_ref = self.chatview_node.upgrade().unwrap();
        let mut data = vec![];
        self.file_url.encode(&mut data).unwrap();
        let _ = node_ref.trigger("file_download_request", data).await;
    }
}

#[async_trait]
impl UIObject for FileMessage {
    fn priority(&self) -> u32 {
        1
    }

    async fn handle_mouse_btn_down(&self, btn: MouseButton, mouse_pos: Point) -> bool {
        if btn != MouseButton::Left {
            return false
        }
        if self.active_rect.is_none() {
            return false
        }
        let rect = self.active_rect.unwrap();
        if !rect.contains(mouse_pos) {
            return false
        }

        self.mouse_btn_held.store(true, Ordering::Relaxed);
        true
    }

    async fn handle_mouse_btn_up(&self, btn: MouseButton, mouse_pos: Point) -> bool {
        if btn != MouseButton::Left {
            return false
        }

        // Did we start the click inside this FileMessage?
        let btn_held = self.mouse_btn_held.swap(false, Ordering::Relaxed);
        if !btn_held {
            return false
        }

        if self.active_rect.is_none() {
            return false
        }
        let rect = self.active_rect.unwrap();
        if !rect.contains(mouse_pos) {
            return false
        }

        match self.status {
            FileMessageStatus::Idle | FileMessageStatus::Error { .. } => {
                self.download().await;
            }
            _ => {}
        }
        true
    }

    async fn handle_touch(&self, phase: TouchPhase, _id: u64, touch_pos: Point) -> bool {
        if phase != TouchPhase::Ended {
            return false
        }
        if self.active_rect.is_none() {
            return false
        }
        let rect = self.active_rect.unwrap();
        if !rect.contains(touch_pos) {
            return false
        }

        match self.status {
            FileMessageStatus::Idle | FileMessageStatus::Error { .. } => {
                self.download().await;
            }
            _ => {}
        }
        true
    }
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

    /// If `local_pos` (message-local coords) is on a URL, return it.
    fn url_hit(&self, local_pos: Point) -> Option<String> {
        match self {
            Self::Priv(m) => m.url_at_local(local_pos).map(|s| s.to_string()),
            _ => None,
        }
    }

    fn cache_txt_layout(
        &mut self,
        clip: &Rectangle,
        line_height: f32,
        timestamp_width: f32,
        nick_colors: &[Color],
        text_color: Color,
        action_text_color: Color,
        url_text_color: Color,
    ) {
        match self {
            Self::Priv(m) => {
                m.cache_txt_layout(
                    clip,
                    line_height,
                    timestamp_width,
                    nick_colors,
                    text_color,
                    action_text_color,
                    url_text_color,
                );
            }
            Self::Date(_) => {}
            Self::File(_) => {}
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
        action_text_color: Color,
        url_text_color: Color,
        url_bg_color: Color,
        url_bg_border_size: f32,
        url_bg_border_color: Color,
        hi_bg_color: Color,
        renderer: &Renderer,
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
                    action_text_color,
                    url_text_color,
                    url_bg_color,
                    url_bg_border_size,
                    url_bg_border_color,
                    hi_bg_color,
                    renderer,
                )
                .await
            }
            Self::Date(m) => m.gen_mesh(line_height, timestamp_color, renderer).await,
            Self::File(m) => {
                m.gen_mesh(clip, line_height, timestamp_width, timestamp_color, renderer).await
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

    fn deselect(&mut self) {
        match self {
            Self::Priv(m) => m.deselect(),
            Self::Date(_) => {}
            Self::File(_) => {}
        }
    }

    fn is_selected(&self) -> bool {
        match self {
            Self::Priv(m) => m.is_selected(),
            _ => false,
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

#[async_trait]
impl UIObject for Message {
    fn priority(&self) -> u32 {
        1
    }
    async fn handle_mouse_btn_down(&self, btn: MouseButton, mouse_pos: Point) -> bool {
        match self {
            Self::Priv(_) => false,
            Self::Date(_) => false,
            Self::File(m) => m.handle_mouse_btn_down(btn, mouse_pos).await,
        }
    }
    async fn handle_mouse_btn_up(&self, btn: MouseButton, mouse_pos: Point) -> bool {
        match self {
            Self::Priv(m) => m.handle_mouse_btn_up(btn, mouse_pos).await,
            Self::Date(_) => false,
            Self::File(m) => m.handle_mouse_btn_up(btn, mouse_pos).await,
        }
    }
    async fn handle_touch(&self, phase: TouchPhase, id: u64, touch_pos: Point) -> bool {
        match self {
            Self::Priv(m) => m.handle_touch(phase, touch_pos).await,
            Self::Date(_) => false,
            Self::File(m) => m.handle_touch(phase, id, touch_pos).await,
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

    font_size: PropertyFloat32,
    timestamp_font_size: PropertyFloat32,
    timestamp_width: PropertyFloat32,
    line_height: PropertyFloat32,
    msg_spacing: PropertyFloat32,
    baseline: PropertyFloat32,
    timestamp_color: PropertyColor,
    text_color: PropertyColor,
    action_text_color: PropertyColor,
    url_text_color: PropertyColor,
    url_bg_color: PropertyColor,
    url_bg_border_size: PropertyFloat32,
    url_bg_border_color: PropertyColor,
    nick_colors: PropertyPtr,
    hi_bg_color: PropertyColor,

    window_scale: PropertyFloat32,
    /// Used to detect if the window scale was changed when drawing.
    /// If it does then we must reload the glyphs too.
    old_window_scale: f32,

    renderer: Renderer,
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
        action_text_color: PropertyColor,
        url_text_color: PropertyColor,
        url_bg_color: PropertyColor,
        url_bg_border_size: PropertyFloat32,
        url_bg_border_color: PropertyColor,
        nick_colors: PropertyPtr,
        hi_bg_color: PropertyColor,
        window_scale: PropertyFloat32,
        renderer: Renderer,
    ) -> Self {
        let old_window_scale = window_scale.get();
        Self {
            msgs: vec![],
            date_msgs: HashMap::new(),

            font_size,
            timestamp_font_size,
            timestamp_width,
            line_height,
            msg_spacing,
            baseline,
            timestamp_color,
            text_color,
            action_text_color,
            url_text_color,
            url_bg_color,
            url_bg_border_size,
            url_bg_border_color,
            nick_colors,
            hi_bg_color,

            window_scale,
            old_window_scale,

            renderer,
        }
    }

    pub fn clear(&mut self) {
        self.msgs.clear();
        self.date_msgs.clear();
    }

    /// Returns whether the scale changed (and params were re-adjusted).
    pub fn adjust_window_scale(&mut self) -> bool {
        let window_scale = self.window_scale.get();
        if self.old_window_scale == window_scale {
            return false
        }

        self.adjust_params();
        true
    }

    /// This will force a reload of everything
    pub fn adjust_params(&mut self) {
        let window_scale = self.window_scale.get();
        let font_size = self.font_size.get();
        let timestamp_font_size = self.timestamp_font_size.get();

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

    pub async fn calc_total_height(&mut self, rect: &Rectangle) -> f32 {
        let line_height = self.line_height.get();
        let baseline = self.baseline.get();
        let timestamp_width = self.timestamp_width.get();
        let msg_spacing = self.msg_spacing.get();
        let text_color = self.text_color.get();
        let action_text_color = self.action_text_color.get();
        let url_text_color = self.url_text_color.get();
        let nick_colors = self.read_nick_colors();
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

            msg.cache_txt_layout(
                &rect,
                line_height,
                timestamp_width,
                &nick_colors,
                text_color,
                action_text_color,
                url_text_color,
            );

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

    pub async fn insert_privmsg(
        &mut self,
        timest: Timestamp,
        msg_id: MessageId,
        nick: String,
        text: String,
        rect: Rectangle,
    ) -> Option<&mut PrivMessage> {
        t!("insert_privmsg({timest}, {msg_id}, {nick}, {text})");
        let line_height = self.line_height.get();
        let font_size = self.font_size.get();
        let timestamp_font_size = self.timestamp_font_size.get();
        let timestamp_width = self.timestamp_width.get();
        let window_scale = self.window_scale.get();
        let text_color = self.text_color.get();
        let action_text_color = self.action_text_color.get();
        let url_text_color = self.url_text_color.get();
        let nick_colors = self.read_nick_colors();

        let mut msg = PrivMessage::new(
            font_size,
            timestamp_font_size,
            window_scale,
            timest,
            msg_id,
            nick,
            text,
        );

        msg.cache_txt_layout(
            &rect,
            line_height,
            timestamp_width,
            &nick_colors,
            text_color,
            action_text_color,
            url_text_color,
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

    pub async fn push_privmsg(
        &mut self,
        timest: Timestamp,
        msg_id: MessageId,
        nick: String,
        text: String,
        rect: &Rectangle,
    ) -> f32 {
        //t!("push_privmsg({timest}, {msg_id}, {nick}, {text})");
        let line_height = self.line_height.get();
        let font_size = self.font_size.get();
        let timestamp_font_size = self.timestamp_font_size.get();
        let timestamp_width = self.timestamp_width.get();
        let window_scale = self.window_scale.get();
        let text_color = self.text_color.get();
        let action_text_color = self.action_text_color.get();
        let url_text_color = self.url_text_color.get();
        let nick_colors = self.read_nick_colors();

        let mut msg = PrivMessage::new(
            font_size,
            timestamp_font_size,
            window_scale,
            timest,
            msg_id,
            nick,
            text,
        );

        msg.cache_txt_layout(
            rect,
            line_height,
            timestamp_width,
            &nick_colors,
            text_color,
            action_text_color,
            url_text_color,
        );

        let msg_height = msg.height(self.line_height.get());
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
        let timestamp_width = self.timestamp_width.get();

        let timest_color = self.timestamp_color.get();
        let text_color = self.text_color.get();
        let action_text_color = self.action_text_color.get();
        let url_text_color = self.url_text_color.get();
        let url_bg_color = self.url_bg_color.get();
        let url_bg_border_size = self.url_bg_border_size.get();
        let url_bg_border_color = self.url_bg_border_color.get();
        let nick_colors = self.read_nick_colors();
        let hi_bg_color = self.hi_bg_color.get();

        let renderer = self.renderer.clone();

        let msgs = self.msgs_with_date();
        let mut msgs = pin!(msgs);

        let mut meshes = vec![];
        let mut current_pos = 0.;
        while let Some(msg) = msgs.next().await {
            let instrs = msg
                .gen_mesh(
                    rect,
                    line_height,
                    msg_spacing,
                    timestamp_width,
                    &nick_colors,
                    timest_color,
                    text_color,
                    action_text_color,
                    url_text_color,
                    url_bg_color,
                    url_bg_border_size,
                    url_bg_border_color,
                    hi_bg_color,
                    &renderer,
                )
                .await;

            let mesh_height = msg.height(line_height);
            current_pos += msg_spacing + mesh_height;

            let msg_top = current_pos;
            let msg_bottom = current_pos - mesh_height;

            if msg_bottom > scroll + rect.h {
                break
            }
            if msg_top < scroll {
                continue
            }

            meshes.push((current_pos, instrs));
        }
        meshes
    }

    pub fn insert_filemsg(
        &mut self,
        chatview_node: SceneNodeWeak,
        timest: Timestamp,
        msg_id: MessageId,
        nick: String,
        file_url: Url,
    ) -> Option<&mut FileMessage> {
        t!("insert_filemsg({timest}, {msg_id}, {nick}, {file_url})");
        let font_size = self.font_size.get();
        let window_scale = self.window_scale.get();

        let msg = FileMessage::new(
            chatview_node,
            font_size,
            window_scale,
            file_url,
            FileMessageStatus::Initializing,
            timest,
        );

        // Timestamps go from most recent backwards
        let mut idx = None;
        for (i, msg) in enumerate_mut(&mut self.msgs) {
            if timest >= msg.timestamp() {
                idx = Some(i);
                break
            }
        }

        let idx = idx.unwrap_or_default();

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

    pub async fn get_line(&mut self, rect: &Rectangle, y: f32) -> Option<(&mut Message, f32)> {
        let line_height = self.line_height.get();
        let msg_spacing = self.msg_spacing.get();
        let timestamp_width = self.timestamp_width.get();
        let text_color = self.text_color.get();
        let action_text_color = self.action_text_color.get();
        let url_text_color = self.url_text_color.get();
        let nick_colors = self.read_nick_colors();

        let msgs = self.msgs_with_date();
        let mut msgs = pin!(msgs);

        let mut current_pos = 0.;
        while let Some(msg) = msgs.next().await {
            // Messages can have their layout cache cleared at any time
            // (e.g. by select/deselect), so make sure it exists before
            // measuring, same as in calc_total_height().
            msg.cache_txt_layout(
                rect,
                line_height,
                timestamp_width,
                &nick_colors,
                text_color,
                action_text_color,
                url_text_color,
            );
            let mesh_height = msg.height(line_height);
            let msg_bottom = current_pos;
            let msg_top = current_pos + mesh_height + msg_spacing;

            if msg_bottom <= y && y <= msg_top {
                return Some((msg, msg_top))
            }

            current_pos += msg_spacing;
            current_pos += mesh_height;
        }

        None
    }

    pub async fn url_at(&mut self, rect: &Rectangle, x: f32, y: f32) -> Option<String> {
        let (msg, msg_top) = self.get_line(rect, y).await?;
        msg.url_hit(Point::new(x, msg_top - y))
    }

    pub async fn select_line(&mut self, rect: &Rectangle, y: f32) {
        if let Some((msg, _)) = self.get_line(rect, y).await {
            // Do nothing
            if msg.is_date() {
                return
            }

            msg.select();

            msg.clear_mesh();
        }
    }

    pub async fn deselect_line(&mut self, rect: &Rectangle, y: f32) {
        if let Some((msg, _)) = self.get_line(rect, y).await {
            if msg.is_date() {
                return
            }

            msg.deselect();

            msg.clear_mesh();
        }
    }

    pub async fn is_line_selected(&mut self, rect: &Rectangle, y: f32) -> bool {
        if let Some((msg, _)) = self.get_line(rect, y).await {
            if msg.is_date() {
                return false
            }
            return msg.is_selected()
        }
        false
    }

    /// Whether any message is currently selected.
    pub fn has_selection(&self) -> bool {
        self.msgs.iter().any(|msg| msg.is_selected())
    }

    /// Deselect every selected message.
    pub fn unselect_all(&mut self) {
        for msg in &mut self.msgs {
            if msg.is_selected() {
                msg.deselect();
                msg.clear_mesh();
            }
        }
    }

    /// Concatenated text of all selected messages, joined by newlines, in
    /// display order. NOTICE messages contribute their body; privmsgs
    /// contribute "<nick> <text>".
    pub fn selected_text(&self) -> String {
        let mut lines = vec![];
        for msg in &self.msgs {
            if let Message::Priv(p) = msg {
                if p.is_selected {
                    if p.nick == "NOTICE" {
                        lines.push(p.text.clone());
                    } else {
                        lines.push(format!("{} {}", p.nick, p.text));
                    }
                }
            }
        }
        lines.join("\n")
    }

    pub fn update_file_status(&mut self, url: &Url, status: &FileMessageStatus) {
        for msg in &mut self.msgs {
            if let Some(filemsg) = msg.get_filemsg_mut() {
                if filemsg.file_url == *url {
                    filemsg.set_status(status);
                    filemsg.clear_mesh();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT_COLOR: Color = [1., 1., 1., 1.];
    const ACTION_COLOR: Color = [0.5, 0.25, 0.75, 1.];
    const URL_COLOR: Color = [0., 0.94, 1., 1.];
    const NICK_COLORS: &[Color] = &[[1., 0., 0., 1.]];

    fn make_priv(text: &str) -> Message {
        PrivMessage::new(14., 10., 1., 0, MessageId([0; 32]), "alice".to_string(), text.to_string())
    }

    #[test]
    fn ctcp_action_fully_framed() {
        assert_eq!(parse_ctcp_action("\u{1}ACTION waves\u{1}"), Some("waves"));
    }

    #[test]
    fn ctcp_action_missing_trailing_delimiter() {
        assert_eq!(parse_ctcp_action("\u{1}ACTION waves"), Some("waves"));
    }

    #[test]
    fn ctcp_action_empty_body() {
        assert_eq!(parse_ctcp_action("\u{1}ACTION \u{1}"), Some(""));
        assert_eq!(parse_ctcp_action("\u{1}ACTION "), Some(""));
    }

    #[test]
    fn ctcp_action_not_an_action() {
        assert_eq!(parse_ctcp_action("waves"), None);
        assert_eq!(parse_ctcp_action("waves\u{1}"), None);
        assert_eq!(parse_ctcp_action("see \u{1}ACTION waves\u{1} here"), None);
        assert_eq!(parse_ctcp_action("\u{1}action waves\u{1}"), None);
        assert_eq!(parse_ctcp_action("\u{1}PING\u{1}"), None);
    }

    #[test]
    fn privmsg_action_detection() {
        let Message::Priv(m) = make_priv("\u{1}ACTION waves\u{1}") else { panic!() };
        assert!(m.is_action);
        assert_eq!(m.text, "waves");

        let Message::Priv(m) = make_priv("\u{1}ACTION waves") else { panic!() };
        assert!(m.is_action);
        assert_eq!(m.text, "waves");

        let Message::Priv(m) = make_priv("/me waves") else { panic!() };
        assert!(!m.is_action);
        assert_eq!(m.text, "/me waves");
    }

    #[test]
    fn action_line_text_and_body_offset() {
        let Message::Priv(m) = make_priv("\u{1}ACTION waves\u{1}") else { panic!() };
        assert_eq!(m.line_text(), "* alice waves");
        assert_eq!(m.body_offset(), "alice".len() + 3);

        let Message::Priv(m) = make_priv("waves") else { panic!() };
        assert!(!m.is_action);
        assert_eq!(m.line_text(), "alice waves");
        assert_eq!(m.body_offset(), "alice".len() + 1);
    }

    #[test]
    fn action_layout_colors() {
        let mut msg = make_priv("\u{1}ACTION waves\u{1}");
        let Message::Priv(m) = &mut msg else { panic!() };
        m.cache_txt_layout(
            &Rectangle::new(0., 0., 1000., 100.),
            20.,
            50.,
            NICK_COLORS,
            TEXT_COLOR,
            ACTION_COLOR,
            URL_COLOR,
        );

        // GlyphRun items are split per style, so brushes identify the
        // colored ranges exactly: the "* <nick> " prefix uses the nick
        // color and the action text uses action_text_color. No other
        // brush may appear in a confirmed action line.
        let layout = m.txt_layout.as_ref().unwrap();
        let mut nick_brushes = 0;
        let mut action_brushes = 0;
        for line in layout.lines() {
            for item in line.items() {
                let parley::PositionedLayoutItem::GlyphRun(run) = item else { continue };
                let brush = run.style().brush;
                if brush == NICK_COLORS[0] {
                    nick_brushes += 1;
                } else if brush == ACTION_COLOR {
                    action_brushes += 1;
                } else {
                    panic!("unexpected brush {brush:?}");
                }
            }
        }
        assert!(nick_brushes > 0);
        assert!(action_brushes > 0);
    }

    #[test]
    fn action_layout_url_color() {
        let mut msg = make_priv("\u{1}ACTION see https://example.com now\u{1}");
        let Message::Priv(m) = &mut msg else { panic!() };
        m.cache_txt_layout(
            &Rectangle::new(0., 0., 1000., 100.),
            20.,
            50.,
            NICK_COLORS,
            TEXT_COLOR,
            ACTION_COLOR,
            URL_COLOR,
        );

        let layout = m.txt_layout.as_ref().unwrap();
        let mut url_brushes = 0;
        let mut action_brushes = 0;
        for line in layout.lines() {
            for item in line.items() {
                let parley::PositionedLayoutItem::GlyphRun(run) = item else { continue };
                if run.style().brush == URL_COLOR {
                    url_brushes += 1;
                } else if run.style().brush == ACTION_COLOR {
                    action_brushes += 1;
                }
            }
        }
        assert!(url_brushes > 0);
        assert!(action_brushes > 0);
    }
}
