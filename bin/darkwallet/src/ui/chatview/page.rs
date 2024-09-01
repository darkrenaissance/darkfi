/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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
use chrono::{Local, TimeZone};
use miniquad::{BufferId, TextureId};
use std::{
    hash::{DefaultHasher, Hash, Hasher},
    io::Cursor,
    sync::{atomic::Ordering, Arc, Mutex as SyncMutex, Weak},
};

use crate::{
    gfx::{
        DrawCall, DrawInstruction, DrawMesh, GraphicsEventPublisherPtr, Point, Rectangle,
        RenderApi, RenderApiPtr,
    },
    mesh::{Color, MeshBuilder, COLOR_BLUE, COLOR_GREEN, COLOR_PINK},
    prop::{PropertyBool, PropertyColor, PropertyFloat32, PropertyPtr, PropertyUint32, Role},
    pubsub::Subscription,
    scene::{Pimpl, SceneGraph, SceneGraphPtr2, SceneNodeId},
    text::{self, glyph_str, Glyph, GlyphPositionIter, TextShaper, TextShaperPtr},
    util::enumerate_mut,
    ExecutorPtr,
};

const PAGE_SIZE: usize = 10;
const PRELOAD_PAGES: usize = 10;

type Timestamp = u64;
type MessageId = [u8; 32];

fn is_whitespace(s: &str) -> bool {
    s.chars().all(char::is_whitespace)
}

#[derive(Clone)]
pub(super) struct PrivMessage {
    font_size: f32,

    timestamp: Timestamp,
    id: MessageId,
    nick: String,
    text: String,

    unwrapped_glyphs: Vec<Glyph>,
    wrapped_lines: Vec<Vec<Glyph>>,

    atlas: text::RenderedAtlas,
    mesh_cache: Option<DrawMesh>,
}

impl PrivMessage {
    pub(super) async fn new(
        font_size: f32,

        timestamp: Timestamp,
        id: MessageId,
        nick: String,
        text: String,

        line_width: f32,

        text_shaper: &TextShaper,
        render_api: &RenderApi,
    ) -> Message {
        let dt = Local.timestamp_millis_opt(timestamp as i64).unwrap();
        let timestr = dt.format("%H:%M").to_string();

        let linetext = format!("{} {} {}", timestr, nick, text);
        let unwrapped_glyphs = text_shaper.shape(linetext, font_size).await;

        let mut atlas = text::Atlas::new(render_api);
        atlas.push(&unwrapped_glyphs);
        let atlas = atlas.make().await.expect("unable to make atlas");

        let mut self_ = Self {
            font_size,
            timestamp,
            id,
            nick,
            text,
            unwrapped_glyphs,
            wrapped_lines: vec![],
            atlas,
            mesh_cache: None,
        };
        self_.adjust_width(line_width);
        Message::Priv(self_)
    }

    fn height(&self, line_height: f32) -> f32 {
        self.wrapped_lines.len() as f32 * line_height
    }

    async fn gen_mesh(
        &mut self,
        clip: &Rectangle,
        line_height: f32,
        baseline: f32,
        nick_colors: &[Color],
        timestamp_color: Color,
        text_color: Color,
        debug_render: bool,
        render_api: &RenderApi,
    ) -> DrawMesh {
        //debug!(target: "ui::chatview", "gen_mesh({})", glyph_str(&self.unwrapped_glyphs));
        let mut mesh = MeshBuilder::new();

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
                off_y,
                is_last_line,
                baseline,
                timestamp_color,
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

        let mesh = mesh.alloc(render_api).await.unwrap();
        let mesh = mesh.draw_with_texture(self.atlas.texture_id);
        self.mesh_cache = Some(mesh.clone());

        mesh
    }

    fn render_line(
        &self,
        mesh: &mut MeshBuilder,
        line: &Vec<Glyph>,
        off_y: f32,
        is_last: bool,
        baseline: f32,
        timestamp_color: Color,
        nick_color: Color,
        text_color: Color,
        debug_render: bool,
    ) {
        //debug!(target: "ui::chatview", "render_line({})", glyph_str(line));
        // Keep track of the 'section'
        // Section 0 is the timestamp
        // Section 1 is the nickname (colorized)
        // Finally is just the message itself
        let mut section = 2;
        if is_last {
            section = 0;
        }

        let glyph_pos_iter = GlyphPositionIter::new(self.font_size, line, baseline);
        for (mut glyph_rect, glyph) in glyph_pos_iter.zip(line.iter()) {
            let uv_rect = self.atlas.fetch_uv(glyph.glyph_id).expect("missing glyph UV rect");
            glyph_rect.y -= off_y;

            let color = match section {
                0 => timestamp_color,
                1 => nick_color,
                _ => text_color,
            };

            //if debug_render {
            //    mesh.draw_outline(&glyph_rect, COLOR_BLUE, 2.);
            //}

            mesh.draw_box(&glyph_rect, color, uv_rect);

            if is_last && section < 2 && is_whitespace(&glyph.substr) {
                section += 1;
            }
        }
    }

    fn clear_mesh(&mut self) -> Option<DrawMesh> {
        std::mem::replace(&mut self.mesh_cache, None)
    }

    fn adjust_width(&mut self, line_width: f32) {
        // Invalidate wrapped_glyphs and recalc
        self.wrapped_lines = text::wrap(line_width, self.font_size, &self.unwrapped_glyphs);
    }
}

#[derive(Clone)]
pub(super) struct DateMessage {
    font_size: f32,
    timestamp: Timestamp,
    glyphs: Vec<Glyph>,

    atlas: text::RenderedAtlas,
    mesh_cache: Option<DrawMesh>,
}

impl DateMessage {
    pub(super) async fn new(
        font_size: f32,
        timestamp: Timestamp,

        text_shaper: &TextShaper,
        render_api: &RenderApi,
    ) -> Message {
        let dt = Local.timestamp_millis_opt(timestamp as i64).unwrap();
        let datestr = dt.format("%a %-d %b %Y").to_string();

        let dt2 = dt.date_naive().and_hms_opt(0, 0, 0).unwrap();
        assert_eq!(dt.date_naive(), dt2.date());
        let timestamp = Local.from_local_datetime(&dt2).unwrap().timestamp_millis() as u64;

        let glyphs = text_shaper.shape(datestr, font_size).await;

        let mut atlas = text::Atlas::new(render_api);
        atlas.push(&glyphs);
        let atlas = atlas.make().await.expect("unable to make atlas");

        Message::Date(Self { font_size, timestamp, glyphs, atlas, mesh_cache: None })
    }

    fn clear_mesh(&mut self) -> Option<DrawMesh> {
        None
    }

    fn adjust_width(&mut self, line_width: f32) {
        // Do nothing
    }

    async fn gen_mesh(
        &mut self,
        clip: &Rectangle,
        line_height: f32,
        baseline: f32,
        nick_colors: &[Color],
        timestamp_color: Color,
        text_color: Color,
        debug_render: bool,
        render_api: &RenderApi,
    ) -> DrawMesh {
        let mut mesh = MeshBuilder::new();

        let glyph_pos_iter = GlyphPositionIter::new(self.font_size, &self.glyphs, baseline);
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

        let mesh = mesh.alloc(render_api).await.unwrap();
        let mesh = mesh.draw_with_texture(self.atlas.texture_id);
        self.mesh_cache = Some(mesh.clone());

        mesh
    }
}

/// Easier than fucking around with traits nonsense
enum Message {
    Priv(PrivMessage),
    Date(DateMessage),
}

impl Message {
    fn timestamp(&self) -> u64 {
        match self {
            Self::Priv(m) => m.timestamp,
            Self::Date(m) => m.timestamp,
        }
    }

    fn height(&self, line_height: f32) -> f32 {
        match self {
            Self::Priv(m) => m.height(line_height),
            Self::Date(_) => line_height,
        }
    }

    fn clear_mesh(&mut self) -> Option<DrawMesh> {
        match self {
            Self::Priv(m) => m.clear_mesh(),
            Self::Date(m) => m.clear_mesh(),
        }
    }

    fn adjust_width(&mut self, line_width: f32) {
        match self {
            Self::Priv(m) => m.adjust_width(line_width),
            Self::Date(_) => {}
        }
    }

    async fn gen_mesh(
        &mut self,
        clip: &Rectangle,
        line_height: f32,
        baseline: f32,
        nick_colors: &[Color],
        timestamp_color: Color,
        text_color: Color,
        debug_render: bool,
        render_api: &RenderApi,
    ) -> DrawMesh {
        match self {
            Self::Priv(m) => {
                m.gen_mesh(
                    clip,
                    line_height,
                    baseline,
                    nick_colors,
                    timestamp_color,
                    text_color,
                    debug_render,
                    render_api,
                )
                .await
            }
            Self::Date(m) => {
                m.gen_mesh(
                    clip,
                    line_height,
                    baseline,
                    nick_colors,
                    timestamp_color,
                    text_color,
                    debug_render,
                    render_api,
                )
                .await
            }
        }
    }

    fn is_date(&self) -> bool {
        match self {
            Self::Priv(m) => false,
            Self::Date(_) => true,
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

#[derive(Default)]
pub(super) struct FreedData {
    pub(super) buffers: Vec<BufferId>,
    pub(super) textures: Vec<TextureId>,
}

impl FreedData {
    fn add_mesh(&mut self, mesh: DrawMesh) {
        self.buffers.push(mesh.vertex_buffer);
        self.buffers.push(mesh.index_buffer);
    }
    fn add_texture(&mut self, texture_id: TextureId) {
        self.textures.push(texture_id);
    }
}

pub struct MessageBuffer {
    /// From most recent to older
    msgs: Vec<Message>,
    pub(super) freed: FreedData,
    pub(super) line_width: f32,
    render_api: RenderApiPtr,
    text_shaper: TextShaperPtr,
}

impl MessageBuffer {
    pub(super) fn new(render_api: RenderApiPtr, text_shaper: TextShaperPtr) -> Self {
        Self { msgs: vec![], freed: Default::default(), line_width: 0., render_api, text_shaper }
    }

    pub(super) fn push(&mut self, msg: Message) {
        self.msgs.push(msg);
    }

    /// For scrolling we want to be able to adjust and measure without
    /// explicitly rendering since it may be off screen.
    pub(super) fn adjust_width(&mut self, line_width: f32) {
        if (line_width - self.line_width).abs() < f32::EPSILON {
            return;
        }
        self.line_width = line_width;

        for msg in &mut self.msgs {
            msg.adjust_width(line_width);
        }

        self.clear_meshes();
    }

    /// Clear all meshes and caches. Returns data that needs to be freed.
    fn clear_meshes(&mut self) {
        for msg in &mut self.msgs {
            if let Some(mesh) = msg.clear_mesh() {
                self.freed.add_mesh(mesh);
            }
        }
    }

    pub(super) fn calc_total_height(&self, line_height: f32, baseline: f32) -> f32 {
        let mut height = 0.;

        for msg in &self.msgs {
            height += msg.height(line_height);
        }

        height
    }

    pub(super) async fn insert_privmsg(
        &mut self,
        font_size: f32,
        timest: Timestamp,
        message_id: MessageId,
        nick: String,
        text: String,
    ) {
        let msg = PrivMessage::new(
            font_size,
            timest,
            message_id,
            nick,
            text,
            self.line_width,
            &self.text_shaper,
            &self.render_api,
        )
        .await;

        if self.msgs.is_empty() {
            let datemsg =
                DateMessage::new(font_size, timest, &self.text_shaper, &self.render_api).await;
            assert!(datemsg.timestamp() <= timest);

            self.msgs.push(msg);
            self.msgs.push(datemsg);
            return
        }

        // We only add lines inside pages.
        // Calling the appropriate draw() function after should preload any missing pages.
        // When a line is before the first page, it will get preloaded as a new page.
        let oldest_timest = self.oldest_timestamp().unwrap();
        if timest < oldest_timest {
            return;
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

        self.insert_msg(msg, idx, font_size).await;
    }

    pub(super) async fn push_privmsg(
        &mut self,
        font_size: f32,
        timest: Timestamp,
        message_id: MessageId,
        nick: String,
        text: String,
    ) {
        let msg = PrivMessage::new(
            font_size,
            timest,
            message_id,
            nick,
            text,
            self.line_width,
            &self.text_shaper,
            &self.render_api,
        )
        .await;

        if self.msgs.is_empty() {
            let datemsg =
                DateMessage::new(font_size, timest, &self.text_shaper, &self.render_api).await;
            assert!(datemsg.timestamp() <= timest);

            self.msgs.push(msg);
            self.msgs.push(datemsg);
            return
        }

        let idx = self.msgs.len() - 1;
        self.insert_msg(msg, idx, font_size).await;
    }

    async fn insert_msg(&mut self, msg: Message, idx: usize, font_size: f32) {
        let timest = msg.timestamp();
        let msg_is_date = msg.is_date();

        // Do we cross from a new day?
        let prev_msg = &self.msgs[idx];
        let prev_timest = prev_msg.timestamp();
        let prev_is_date = prev_msg.is_date();
        let prev_date = Local.timestamp_millis_opt(prev_timest as i64).unwrap().date_naive();
        let curr_date = Local.timestamp_millis_opt(timest as i64).unwrap().date_naive();

        self.msgs.insert(idx, msg);

        if !prev_is_date && !msg_is_date && prev_date != curr_date {
            let datemsg =
                DateMessage::new(font_size, timest, &self.text_shaper, &self.render_api).await;
            assert!(datemsg.timestamp() <= timest);
            self.msgs.insert(idx + 1, datemsg);
        }
    }

    /// Generate caches and return meshes
    pub(super) async fn gen_meshes(
        &mut self,
        rect: &Rectangle,
        scroll: f32,
        font_size: f32,
        line_height: f32,
        baseline: f32,
        nick_colors: &[Color],
        timestamp_color: Color,
        text_color: Color,
        debug_render: bool,
    ) -> Vec<(f32, DrawMesh)> {
        let mut meshes = vec![];

        let descent = line_height - baseline;
        let mut current_pos = descent;
        for msg in &mut self.msgs {
            let mesh_height = msg.height(line_height);
            let msg_bottom = current_pos;
            let msg_top = current_pos + mesh_height;

            if msg_bottom > scroll + rect.h {
                break
            }
            if msg_top < scroll {
                current_pos += mesh_height;
                continue
            }

            let mesh = msg
                .gen_mesh(
                    rect,
                    line_height,
                    baseline,
                    nick_colors,
                    timestamp_color,
                    text_color,
                    debug_render,
                    &self.render_api,
                )
                .await;

            meshes.push((current_pos, mesh));
            current_pos += mesh_height;
        }

        //debug!("gen_meshes() returning {} meshes", meshes.len());
        meshes
    }

    pub(super) fn oldest_timestamp(&self) -> Option<Timestamp> {
        let last_msg = &self.msgs.last()?;
        Some(last_msg.timestamp())
    }

    pub(super) fn latest_timestamp(&self) -> Option<Timestamp> {
        let first_msg = &self.msgs.first()?;
        Some(first_msg.timestamp())
    }
}
