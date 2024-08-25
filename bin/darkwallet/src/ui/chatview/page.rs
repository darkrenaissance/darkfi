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
    mesh::{Color, MeshBuilder, COLOR_BLUE, COLOR_GREEN},
    prop::{PropertyBool, PropertyColor, PropertyFloat32, PropertyPtr, PropertyUint32, Role},
    pubsub::Subscription,
    scene::{Pimpl, SceneGraph, SceneGraphPtr2, SceneNodeId},
    text::{self, Glyph, GlyphPositionIter, TextShaper, TextShaperPtr},
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
pub(super) struct Message {
    font_size: f32,

    timestamp: Timestamp,
    id: MessageId,
    nick: String,
    text: String,

    unwrapped_glyphs: Vec<Glyph>,
    wrapped_lines: Vec<Vec<Glyph>>,
    line_width: f32,
}

impl Message {
    pub(super) async fn new(
        font_size: f32,

        timestamp: Timestamp,
        id: MessageId,
        nick: String,
        text: String,

        text_shaper: &TextShaper,
    ) -> Self {
        let dt = Local.timestamp_opt(timestamp as i64, 0).unwrap();
        let timestr = dt.format("%H:%M").to_string();

        let linetext = format!("{} {} {}", timestr, nick, text);
        let unwrapped_glyphs = text_shaper.shape(linetext, font_size).await;

        Self {
            font_size,
            timestamp,
            id,
            nick,
            text,
            unwrapped_glyphs,
            wrapped_lines: vec![],
            line_width: 0.,
        }
    }

    fn adjust_line_width(&mut self, line_width: f32) {
        if (line_width - self.line_width).abs() < f32::EPSILON {
            return;
        }

        // Invalidate wrapped_glyphs and recalc
        self.wrapped_lines = text::wrap(line_width, self.font_size, &self.unwrapped_glyphs);
        self.line_width = line_width;
    }
}

enum PageVisibility {
    Null,
    Invisible,
    EnterTop,
    Visible,
    ExitBottom,
}

pub(super) struct Page {
    msgs: Vec<Message>,
    atlas: text::RenderedAtlas,
    mesh_cache: Option<DrawMesh>,
    visible: PageVisibility,
}

impl Page {
    pub(super) async fn new(msgs: Vec<Message>, render_api: &RenderApi) -> Self {
        let mut atlas = text::Atlas::new(render_api);
        for msg in &msgs {
            atlas.push(&msg.unwrapped_glyphs);
        }
        let atlas = atlas.make().await.expect("unable to make atlas!");

        Self { msgs, atlas, mesh_cache: None, visible: PageVisibility::Null }
    }

    fn clear_mesh(&mut self) -> Option<DrawMesh> {
        std::mem::replace(&mut self.mesh_cache, None)
    }

    async fn split(mut msgs: Vec<Message>, render_api: &RenderApi) -> Vec<Self> {
        msgs.sort_unstable_by_key(|msg| msg.timestamp);
        msgs.reverse();

        let chunk_size = if msgs.len() > PAGE_SIZE {
            // Round up so we don't get a weird page with a single item
            msgs.len() / 2 + 1
        } else {
            PAGE_SIZE
        };

        // Replace single page with N pages each with chunk_size messages
        let mut new_pages = vec![];
        for page_msgs in msgs.chunks(chunk_size).map(|m| m.to_vec()) {
            debug!(target: "ui::chatview", "PAGE ==========================");
            for msg in &page_msgs {
                debug!(target: "ui::chatview", "{} {:?}", msg.timestamp, msg.text);
            }
            debug!(target: "ui::chatview", "===============================");

            let new_page = Page::new(page_msgs, render_api).await;
            new_pages.push(new_page);
        }
        assert!(!new_pages.is_empty());
        assert!(new_pages.len() <= 2);
        new_pages
    }

    fn wrapped_lines_count(&self) -> usize {
        let mut count = 0;
        for msg in &self.msgs {
            count += msg.wrapped_lines.len();
        }
        count
    }

    async fn gen_mesh(
        &mut self,
        render_api: &RenderApi,
        clip: &Rectangle,
        font_size: f32,
        line_height: f32,
        baseline: f32,
        nick_colors: &[Color],
        timestamp_color: Color,
        text_color: Color,
        debug_render: bool,
    ) -> DrawMesh {
        if let Some(mesh) = &self.mesh_cache {
            return mesh.clone()
        }

        let mut line_idx = 0;
        let mut mesh = MeshBuilder::new();

        for msg in &self.msgs {
            let nick_color = select_nick_color(&msg.nick, nick_colors);

            let last_idx = msg.wrapped_lines.len() - 1;
            for (i, line) in msg.wrapped_lines.iter().rev().enumerate() {
                let off_y = (line_idx + 1) as f32 * line_height;
                let is_last_line = i == last_idx;

                // debug draw baseline
                if debug_render {
                    let y = baseline - off_y;
                    mesh.draw_filled_box(
                        &Rectangle { x: 0., y: y - 1., w: clip.w, h: 1. },
                        COLOR_BLUE,
                    );
                }

                self.render_line(
                    &mut mesh,
                    line,
                    off_y,
                    is_last_line,
                    font_size,
                    baseline,
                    timestamp_color,
                    nick_color,
                    text_color,
                    debug_render,
                );

                line_idx += 1;
            }
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
        font_size: f32,
        baseline: f32,
        timestamp_color: Color,
        nick_color: Color,
        text_color: Color,
        debug_render: bool,
    ) {
        // Keep track of the 'section'
        // Section 0 is the timestamp
        // Section 1 is the nickname (colorized)
        // Finally is just the message itself
        let mut section = 2;
        if is_last {
            section = 0;
        }

        let glyph_pos_iter = GlyphPositionIter::new(font_size, line, baseline);
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
}

fn select_nick_color(nick: &str, nick_colors: &[Color]) -> Color {
    let mut hasher = DefaultHasher::new();
    nick.hash(&mut hasher);
    let i = hasher.finish() as usize;
    let color = nick_colors[i % nick_colors.len()];
    color
}

struct FreedData {
    buffers: Vec<BufferId>,
    textures: Vec<TextureId>,
}

impl FreedData {
    fn add(&mut self, mesh: DrawMesh) {
        self.buffers.push(mesh.vertex_buffer);
        self.buffers.push(mesh.index_buffer);
        if let Some(texture_id) = mesh.texture {
            self.textures.push(texture_id);
        }
    }
}

pub struct PageManager {
    pages: Vec<Page>,
    freed: FreedData,
    render_api: RenderApiPtr,
    text_shaper: TextShaperPtr,
}

impl PageManager {
    pub(super) fn new(render_api: RenderApiPtr, text_shaper: TextShaperPtr) -> Self {
        Self {
            pages: vec![],
            freed: FreedData { buffers: vec![], textures: vec![] },
            render_api,
            text_shaper,
        }
    }

    pub(super) fn push(&mut self, page: Page) {
        self.pages.push(page);
    }

    /// For scrolling we want to be able to adjust and measure without
    /// explicitly rendering since it may be off screen.
    pub(super) fn adjust_line_width(&mut self, line_width: f32) {
        for page in &mut self.pages {
            for msg in &mut page.msgs {
                msg.adjust_line_width(line_width);
            }
        }
    }

    pub(super) fn calc_total_height(&self, line_height: f32, baseline: f32) -> f32 {
        let descent = line_height - baseline;
        let mut height = descent;

        for page in &self.pages {
            let lines_count = page.wrapped_lines_count();
            height += lines_count as f32 * line_height;
        }

        height
    }

    pub(super) async fn insert_line(
        &mut self,
        font_size: f32,
        timest: Timestamp,
        message_id: MessageId,
        nick: String,
        text: String,
    ) {
        let msg = Message::new(font_size, timest, message_id, nick, text, &self.text_shaper).await;

        // Now add message to page

        // Maybe we can write this code below better
        if self.pages.is_empty() {
            let page = Page::new(vec![msg], &self.render_api).await;
            self.pages.push(page);
            return;
        }

        let mut idx = None;
        for (i, page) in self.pages.iter_mut().enumerate() {
            let first_timest = page.msgs.last().unwrap().timestamp;
            let last_timest = page.msgs.first().unwrap().timestamp;

            if timest < first_timest {
                // It does not belong to any current page
                // Create page only if there's not enough pages for the screen rect
                // current_height < rect.h
                // Otherwise it just means the page wasn't loaded
                // Maybe we need to have a flag indicating there's pages not loaded in the DB still
            }

            //debug!(target: "ui::chatview", "page {i} [{first_timest}, {last_timest}]");
            if first_timest <= timest && timest <= last_timest {
                //debug!(target: "ui::chatview", "found page {i} [{first_timest}, {last_timest}]");
                idx = Some(i);
                break
            }
        }

        let Some(idx) = idx else {
            // Add to the end
            let page = Page::new(vec![msg], &self.render_api).await;
            self.pages.push(page);
            return
        };

        let old_pages_len = self.pages.len();
        // We now want to replace this page by 1 or 2 pages
        // Split pages into 3 parts: head, replaced page and tail
        let mut head = std::mem::replace(&mut self.pages, vec![]);
        let mut drain_iter = head.drain(idx..);
        // Drop the item at idx which will be replaced
        let old_page = drain_iter.next().unwrap();
        let mut tail: Vec<_> = drain_iter.collect();
        assert_eq!(old_pages_len, head.len() + 1 + tail.len());

        // Free texture and mesh before dropping page
        if let Some(mesh) = old_page.mesh_cache {
            self.freed.add(mesh);
        }

        let mut msgs = old_page.msgs;
        msgs.push(msg);
        let mut new_pages = Page::split(msgs, &self.render_api).await;

        self.pages.append(&mut head);
        self.pages.append(&mut new_pages);
        self.pages.append(&mut tail);
    }

    /// Clear all meshes and caches. Returns data that needs to be freed.
    pub(super) fn invalidate_caches(&mut self) {
        for page in &mut self.pages {
            if let Some(mesh) = page.clear_mesh() {
                self.freed.add(mesh);
            }
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
        let mut current_height = descent;
        for page in &mut self.pages {
            if current_height > scroll + rect.h {
                break
            }

            let mesh = page
                .gen_mesh(
                    &self.render_api,
                    rect,
                    font_size,
                    line_height,
                    baseline,
                    nick_colors,
                    timestamp_color,
                    text_color,
                    debug_render,
                )
                .await;

            let lines_count = page.wrapped_lines_count();
            let mesh_height = lines_count as f32 * line_height;

            meshes.push((mesh_height, mesh));

            current_height += mesh_height;
        }

        meshes
    }

    pub(super) fn last_timestamp(&self) -> Timestamp {
        self.pages.last().unwrap().msgs.last().unwrap().timestamp
    }
}
