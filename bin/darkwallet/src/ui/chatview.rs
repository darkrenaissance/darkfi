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

use darkfi_serial::{deserialize, Decodable, Encodable, SerialDecodable, SerialEncodable};
use miniquad::TouchPhase;
use rand::{rngs::OsRng, Rng};
use std::{
    collections::BTreeMap,
    hash::{DefaultHasher, Hash, Hasher},
    sync::{Arc, Mutex as SyncMutex, Weak},
};

use crate::{
    error::Result,
    gfx2::{
        DrawCall, DrawInstruction, DrawMesh, GraphicsEventPublisherPtr, Point, Rectangle,
        RenderApi, RenderApiPtr, Vertex,
    },
    mesh::{Color, MeshBuilder, MeshInfo, COLOR_BLUE, COLOR_GREY, COLOR_WHITE},
    prop::{
        PropertyBool, PropertyColor, PropertyFloat32, PropertyPtr, PropertyStr, PropertyUint32,
    },
    pubsub::Subscription,
    scene::{Pimpl, SceneGraph, SceneGraphPtr2, SceneNodeId},
    text2::{self, Glyph, GlyphPositionIter, SpritePtr, TextShaper, TextShaperPtr},
    util::zip3,
};

use super::{eval_rect, get_parent_rect, read_rect, DrawUpdate, OnModify, Stoppable};

const DEBUG_RENDER: bool = false;

fn is_whitespace(s: &str) -> bool {
    s.chars().all(char::is_whitespace)
}

#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct ChatMsg {
    pub nick: String,
    pub text: String,
}

type Timestamp = u32;

#[derive(Clone)]
struct Message {
    timest: Timestamp,
    chatmsg: ChatMsg,
    glyphs: Vec<Glyph>,
}

const LINES_PER_PAGE: usize = 10;
const PRELOAD_PAGES: usize = 200;

#[derive(Clone)]
struct Page {
    msgs: Vec<Message>,
    atlas: text2::RenderedAtlas,
}

struct TouchInfo {
    start_scroll: f32,
    start_y: f32,
    start_instant: std::time::Instant,
    last_y: f32,
}

impl TouchInfo {
    fn new() -> Self {
        Self { start_scroll: 0., start_y: 0., start_instant: std::time::Instant::now(), last_y: 0. }
    }
}

pub type ChatViewPtr = Arc<ChatView>;

pub struct ChatView {
    node_id: SceneNodeId,
    tasks: Vec<smol::Task<()>>,
    sg: SceneGraphPtr2,
    render_api: RenderApiPtr,
    text_shaper: TextShaperPtr,
    tree: sled::Tree,

    pages: SyncMutex<Vec<Page>>,
    drawcalls: SyncMutex<Vec<DrawMesh>>,
    dc_key: u64,

    /// Used for detecting when scrolling view
    mouse_pos: SyncMutex<Point>,
    /// Touch scrolling
    touch_info: SyncMutex<TouchInfo>,

    rect: PropertyPtr,
    scroll: PropertyFloat32,
    font_size: PropertyFloat32,
    line_height: PropertyFloat32,
    baseline: PropertyFloat32,
    timestamp_color: PropertyColor,
    text_color: PropertyColor,
    nick_colors: PropertyPtr,
    z_index: PropertyUint32,
}

impl ChatView {
    pub async fn new(
        ex: Arc<smol::Executor<'static>>,
        sg: SceneGraphPtr2,
        node_id: SceneNodeId,
        render_api: RenderApiPtr,
        event_pub: GraphicsEventPublisherPtr,
        text_shaper: TextShaperPtr,
        tree: sled::Tree,
    ) -> Pimpl {
        debug!(target: "ui::chatview", "ChatView::new()");
        let scene_graph = sg.lock().await;
        let node = scene_graph.get_node(node_id).unwrap();
        let node_name = node.name.clone();

        let rect = node.get_property("rect").expect("ChatView::rect");
        let scroll = PropertyFloat32::wrap(node, "scroll", 0).unwrap();
        let font_size = PropertyFloat32::wrap(node, "font_size", 0).unwrap();
        let line_height = PropertyFloat32::wrap(node, "line_height", 0).unwrap();
        let baseline = PropertyFloat32::wrap(node, "baseline", 0).unwrap();
        let timestamp_color = PropertyColor::wrap(node, "timestamp_color").unwrap();
        let text_color = PropertyColor::wrap(node, "text_color").unwrap();
        let nick_colors = node.get_property("nick_colors").expect("ChatView::nick_colors");
        let z_index = PropertyUint32::wrap(node, "z_index", 0).unwrap();

        drop(scene_graph);

        let self_ = Arc::new_cyclic(|me: &Weak<Self>| {
            let ev_sub = event_pub.subscribe_mouse_wheel();
            let me2 = me.clone();
            let mouse_wheel_task = ex.spawn(async move {
                loop {
                    Self::process_mouse_wheel(&me2, &ev_sub).await;
                }
            });

            let ev_sub = event_pub.subscribe_mouse_move();
            let me2 = me.clone();
            let mouse_move_task = ex.spawn(async move {
                loop {
                    Self::process_mouse_move(&me2, &ev_sub).await;
                }
            });

            let ev_sub = event_pub.subscribe_touch();
            let me2 = me.clone();
            let touch_task = ex.spawn(async move {
                loop {
                    Self::process_touch(&me2, &ev_sub).await;
                }
            });

            let mut on_modify = OnModify::new(ex, node_name, node_id, me.clone());

            //on_modify.when_change(scroll.prop(), Self::scrollview);

            async fn redraw(self_: Arc<ChatView>) {
                self_.redraw().await;
            }
            on_modify.when_change(rect.clone(), redraw);

            let mut tasks = vec![mouse_wheel_task, mouse_move_task, touch_task];
            tasks.append(&mut on_modify.tasks);

            Self {
                node_id,
                tasks,
                sg,
                render_api,
                text_shaper,
                tree,

                pages: SyncMutex::new(Vec::new()),
                drawcalls: SyncMutex::new(Vec::new()),
                dc_key: OsRng.gen(),

                mouse_pos: SyncMutex::new(Point::from([0., 0.])),
                touch_info: SyncMutex::new(TouchInfo::new()),

                rect,
                scroll,
                font_size,
                line_height,
                baseline,
                timestamp_color,
                text_color,
                nick_colors,
                z_index,
            }
        });

        self_.populate().await;

        Pimpl::ChatView(self_)
    }

    async fn process_mouse_wheel(me: &Weak<Self>, ev_sub: &Subscription<(f32, f32)>) {
        let Ok((wheel_x, wheel_y)) = ev_sub.receive().await else {
            debug!(target: "ui::chatview", "Event relayer closed");
            return
        };

        let Some(self_) = me.upgrade() else {
            // Should not happen
            panic!("self destroyed before mouse_wheel_task was stopped!");
        };

        self_.handle_mouse_wheel(wheel_x, wheel_y).await;
    }

    async fn process_mouse_move(me: &Weak<Self>, ev_sub: &Subscription<(f32, f32)>) {
        let Ok((mouse_x, mouse_y)) = ev_sub.receive().await else {
            debug!(target: "ui::chatview", "Event relayer closed");
            return
        };

        let Some(self_) = me.upgrade() else {
            // Should not happen
            panic!("self destroyed before mouse_move_task was stopped!");
        };

        self_.handle_mouse_move(mouse_x, mouse_y).await;
    }

    async fn process_touch(me: &Weak<Self>, ev_sub: &Subscription<(TouchPhase, u64, f32, f32)>) {
        let Ok((phase, id, touch_x, touch_y)) = ev_sub.receive().await else {
            debug!(target: "ui::chatview", "Event relayer closed");
            return
        };

        let Some(self_) = me.upgrade() else {
            // Should not happen
            panic!("self destroyed before touch_task was stopped!");
        };

        self_.handle_touch(phase, id, touch_x, touch_y).await;
    }

    async fn handle_mouse_wheel(&self, wheel_x: f32, wheel_y: f32) {
        debug!(target: "ui::chatview", "handle_mouse_wheel({wheel_x}, {wheel_y})");

        let Some(rect) = self.get_cached_world_rect().await else { return };

        let mouse_pos = self.mouse_pos.lock().unwrap().clone();
        if !rect.contains(&mouse_pos) {
            debug!(target: "ui::chatview", "not inside rect");
            return
        }
        debug!(target: "ui::chatview", "inside rect");
        let scroll = self.scroll.get();
        self.scroll.set(scroll + wheel_y * 50.);
        self.scrollview().await;
    }

    async fn handle_mouse_move(&self, mouse_x: f32, mouse_y: f32) {
        //debug!(target: "ui::chatview", "handle_mouse_move({mouse_x}, {mouse_y})");
        let mut mouse_pos = self.mouse_pos.lock().unwrap();
        mouse_pos.x = mouse_x;
        mouse_pos.y = mouse_y;
    }

    async fn handle_touch(&self, phase: TouchPhase, id: u64, touch_x: f32, touch_y: f32) {
        // Ignore multi-touch
        if id != 0 {
            return
        }
        // Simulate mouse events
        match phase {
            TouchPhase::Started => {
                let mut touch_info = self.touch_info.lock().unwrap();
                touch_info.start_scroll = self.scroll.get();
                touch_info.start_y = touch_y;
                touch_info.start_instant = std::time::Instant::now();
                touch_info.last_y = touch_y;
            }
            TouchPhase::Moved => {
                let (start_scroll, start_y) = {
                    let mut touch_info = self.touch_info.lock().unwrap();
                    touch_info.last_y = touch_y;
                    (touch_info.start_scroll, touch_info.start_y)
                };

                let dist = touch_y - start_y;
                // TODO the line selected should be fixed and move exactly that distance
                // No use of multipliers
                // TODO we are maybe doing too many updates so make a widget to 'slow down'
                // how often we move to fixed intervals.
                // draw a poly shape and eval each line segment.
                self.scroll.set(start_scroll + dist);
                self.scrollview().await;
            }
            TouchPhase::Ended => {
                // Now calculate scroll acceleration
            }
            TouchPhase::Cancelled => {}
        }
    }

    /// Beware of this method. Here be dragons.
    /// Possibly racy so we limit it just to mouse stuff (for now).
    fn cached_rect(&self) -> Option<Rectangle> {
        let Ok(rect) = read_rect(self.rect.clone()) else {
            error!(target: "ui::chatview", "cached_rect is None");
            return None
        };
        Some(rect)
    }
    async fn get_parent_rect(&self) -> Option<Rectangle> {
        let sg = self.sg.lock().await;
        let node = sg.get_node(self.node_id).unwrap();
        let Some(parent_rect) = get_parent_rect(&sg, node) else {
            return None;
        };
        drop(sg);
        Some(parent_rect)
    }
    async fn get_cached_world_rect(&self) -> Option<Rectangle> {
        // NBD if it's slightly wrong
        let mut rect = self.cached_rect()?;

        // If layers can be nested and we use offsets for (x, y)
        // then this will be incorrect for nested layers.
        // For now we don't allow nesting of layers.
        let parent_rect = self.get_parent_rect().await?;

        // Offset rect which is now in world coords
        rect.x += parent_rect.x;
        rect.y += parent_rect.y;

        Some(rect)
    }

    async fn populate(&self) {
        let mut pages = vec![];
        let mut msgs = vec![];

        for entry in self.tree.iter().rev() {
            let Ok((k, v)) = entry else { break };
            assert_eq!(k.len(), 4);
            let key_bytes: [u8; 4] = k.as_ref().try_into().unwrap();
            let timest = Timestamp::from_be_bytes(key_bytes);
            let chatmsg: ChatMsg = deserialize(&v).unwrap();
            //println!("{k:?} {chatmsg:?}");

            let timestr = timest.to_string();
            // left pad with zeros
            let mut timestr = format!("{:0>4}", timestr);
            timestr.insert(2, ':');

            let text = format!("{} {} {}", timestr, chatmsg.nick, chatmsg.text);
            let glyphs = self.text_shaper.shape(text, self.font_size.get()).await;

            msgs.push(Message { timest, chatmsg, glyphs });

            if msgs.len() >= LINES_PER_PAGE {
                let mut atlas = text2::Atlas::new(&self.render_api);
                for msg in &msgs {
                    atlas.push(&msg.glyphs);
                }
                let Ok(atlas) = atlas.make().await else {
                    // what else should I do here?
                    panic!("unable to make atlas!");
                };

                let page = Page { msgs: std::mem::take(&mut msgs), atlas };
                pages.push(page);

                if pages.len() >= PRELOAD_PAGES {
                    break
                }
            }
        }
        debug!(target: "ui::chatview", "populated {} pages", pages.len());
        *self.pages.lock().unwrap() = pages;
    }

    /// Basically a version of redraw() where regen_mesh() is never called.
    /// Instead we use the cached version.
    async fn scrollview(&self) {
        debug!(target: "ui::chatview", "scrollview()");
        let sg = self.sg.lock().await;
        let node = sg.get_node(self.node_id).unwrap();

        let Some(parent_rect) = get_parent_rect(&sg, node) else {
            return;
        };

        if let Err(err) = eval_rect(self.rect.clone(), &parent_rect) {
            panic!("Node {:?} bad rect property: {}", node, err);
        }

        let Ok(mut rect) = read_rect(self.rect.clone()) else {
            panic!("Node {:?} bad rect property", node);
        };

        //let mut drawcalls = self.regen_mesh(rect.clone()).await;

        debug!(target: "ui::chatview", "chatview rect = {:?}", rect);

        // Apply scroll and scissor
        // We use the scissor for scrolling
        // Because we use the scissor, our actual rect is now rect instead of parent_rect
        let off_x = 0.;
        // This calc decides whether scroll is in terms of pages or pixels
        let off_y = (self.scroll.get() + rect.h) / rect.h;
        let scale_x = 1. / rect.w;
        let scale_y = 1. / rect.h;
        let model = glam::Mat4::from_translation(glam::Vec3::new(off_x, off_y, 0.)) *
            glam::Mat4::from_scale(glam::Vec3::new(scale_x, scale_y, 1.));

        let mut instrs =
            vec![DrawInstruction::ApplyViewport(rect), DrawInstruction::ApplyMatrix(model)];
        let drawcalls = self.drawcalls.lock().unwrap().clone();
        let mut drawcalls: Vec<_> =
            drawcalls.into_iter().map(|dc| DrawInstruction::Draw(dc)).collect();
        instrs.append(&mut drawcalls);

        let draw_calls =
            vec![(self.dc_key, DrawCall { instrs, dcs: vec![], z_index: self.z_index.get() })];

        self.render_api.replace_draw_calls(draw_calls).await;

        debug!(target: "ui::chatview", "scrollview done");
    }

    async fn redraw(&self) {
        debug!(target: "ui::chatview", "redraw()");
        let sg = self.sg.lock().await;
        let node = sg.get_node(self.node_id).unwrap();

        let Some(parent_rect) = get_parent_rect(&sg, node) else {
            return;
        };

        let Some(draw_update) = self.draw(&sg, &parent_rect).await else {
            error!(target: "ui::chatview", "ChatView {:?} failed to draw", node);
            return;
        };
        self.render_api.replace_draw_calls(draw_update.draw_calls).await;
        debug!(target: "ui::chatview", "replace draw calls done");
        for buffer_id in draw_update.freed_buffers {
            self.render_api.delete_buffer(buffer_id);
        }
        for texture_id in draw_update.freed_textures {
            self.render_api.delete_texture(texture_id);
        }
    }

    async fn regen_mesh(&self, mut clip: Rectangle) -> Vec<DrawMesh> {
        let font_size = self.font_size.get();
        let line_height = self.line_height.get();
        let baseline = self.baseline.get();
        // Draw time and nick, then go over each word. If word crosses end of line
        // then apply a line break before the word and continue.
        let pages = self.pages.lock().unwrap().clone();

        let mut draws = vec![];
        let color = COLOR_WHITE;

        let timestamp_color = self.timestamp_color.get();
        let text_color = self.text_color.get();
        let nick_colors = self.read_nick_colors();

        // This is a little hack to nudge the bottom line up slightly, otherwise
        // chars like p will cross the bottom.
        let descent = baseline / 2.;

        // Pages start at the bottom.
        let mut current_idx = 0;
        'pageloop: for page in pages {
            let mut mesh = MeshBuilder::new();

            for msg in page.msgs {
                let glyphs = msg.glyphs;

                let nick_color = self.select_nick_color(&msg.chatmsg.nick, &nick_colors);

                // Keep track of the 'section'
                // Section 0 is the timestamp
                // Section 1 is the nickname (colorized)
                // Finally is just the message itself
                let mut section = 2;

                let mut lines = text2::wrap(clip.w, font_size, &glyphs);
                // We are drawing bottom up but line wrap gives us lines in normal order
                lines.reverse();
                let last_idx = lines.len() - 1;
                for (i, line) in lines.into_iter().enumerate() {
                    let off_y = descent + baseline + current_idx as f32 * line_height;

                    //if px_height > clip.h {
                    //    break 'pageloop;
                    //}

                    if i == last_idx {
                        section = 0;
                    }

                    // Render line
                    let mut glyph_pos_iter = GlyphPositionIter::new(font_size, &line, baseline);
                    for (mut glyph_rect, glyph) in glyph_pos_iter.zip(line.iter()) {
                        let uv_rect =
                            page.atlas.fetch_uv(glyph.glyph_id).expect("missing glyph UV rect");
                        glyph_rect.y -= off_y;

                        let color = match section {
                            0 => timestamp_color,
                            1 => nick_color,
                            _ => text_color,
                        };

                        mesh.draw_box(&glyph_rect, color, uv_rect);

                        if section < 2 && is_whitespace(&glyph.substr) {
                            section += 1;
                        }
                    }

                    current_idx += 1;
                }
            }

            let mesh = mesh.alloc(&self.render_api).await.unwrap();

            draws.push(DrawMesh {
                vertex_buffer: mesh.vertex_buffer,
                index_buffer: mesh.index_buffer,
                texture: Some(page.atlas.texture_id),
                num_elements: mesh.num_elements,
            });
        }

        if DEBUG_RENDER {
            let mut debug_mesh = MeshBuilder::new();
            debug_mesh.draw_outline(
                &Rectangle { x: 0., y: -clip.h, w: clip.w, h: clip.h },
                COLOR_BLUE,
                2.,
            );
            let mesh = debug_mesh.alloc(&self.render_api).await.unwrap();
            draws.push(DrawMesh {
                vertex_buffer: mesh.vertex_buffer,
                index_buffer: mesh.index_buffer,
                texture: None,
                num_elements: mesh.num_elements,
            });
        }

        draws
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

    fn select_nick_color(&self, nick: &str, nick_colors: &[Color]) -> Color {
        let mut hasher = DefaultHasher::new();
        nick.hash(&mut hasher);
        let i = hasher.finish() as usize;
        let color = nick_colors[i % nick_colors.len()];
        color
    }

    pub async fn draw(&self, sg: &SceneGraph, parent_rect: &Rectangle) -> Option<DrawUpdate> {
        debug!(target: "ui::chatview", "ChatView::draw()");
        // Only used for debug messages
        let node = sg.get_node(self.node_id).unwrap();

        if let Err(err) = eval_rect(self.rect.clone(), parent_rect) {
            panic!("Node {:?} bad rect property: {}", node, err);
        }

        let Ok(mut rect) = read_rect(self.rect.clone()) else {
            panic!("Node {:?} bad rect property", node);
        };

        // TODO: Do we need this? Because of the viewport clipping
        rect.x += parent_rect.x;
        rect.y += parent_rect.y;

        let drawcalls = self.regen_mesh(rect.clone()).await;
        let old_drawcalls =
            std::mem::replace(&mut *self.drawcalls.lock().unwrap(), drawcalls.clone());
        let mut drawcalls: Vec<_> =
            drawcalls.into_iter().map(|dc| DrawInstruction::Draw(dc)).collect();

        let mut freed_textures = vec![];
        let mut freed_buffers = vec![];
        for old_dc in old_drawcalls {
            freed_buffers.push(old_dc.vertex_buffer);
            freed_buffers.push(old_dc.index_buffer);
            if let Some(texture_id) = old_dc.texture {
                freed_textures.push(texture_id);
            }
        }

        debug!(target: "ui::chatview", "chatview rect = {:?}", rect);

        // Apply scroll and scissor
        // We use the scissor for scrolling
        // Because we use the scissor, our actual rect is now rect instead of parent_rect
        let off_x = 0.;
        // This calc decides whether scroll is in terms of pages or pixels
        let off_y = (self.scroll.get() + rect.h) / rect.h;
        let scale_x = 1. / rect.w;
        let scale_y = 1. / rect.h;
        let model = glam::Mat4::from_translation(glam::Vec3::new(off_x, off_y, 0.)) *
            glam::Mat4::from_scale(glam::Vec3::new(scale_x, scale_y, 1.));

        let mut instrs =
            vec![DrawInstruction::ApplyViewport(rect), DrawInstruction::ApplyMatrix(model)];
        //let mut instrs = vec![DrawInstruction::ApplyMatrix(model)];
        instrs.append(&mut drawcalls);

        Some(DrawUpdate {
            key: self.dc_key,
            draw_calls: vec![(
                self.dc_key,
                DrawCall { instrs, dcs: vec![], z_index: self.z_index.get() },
            )],
            freed_textures,
            freed_buffers,
        })
    }
}
