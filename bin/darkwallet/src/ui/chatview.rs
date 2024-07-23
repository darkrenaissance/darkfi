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
use rand::{rngs::OsRng, Rng};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex as SyncMutex, Weak},
};

use crate::{
    error::Result,
    gfx2::{
        DrawCall, DrawInstruction, DrawMesh, GraphicsEventPublisherPtr, Point, Rectangle,
        RenderApi, RenderApiPtr, Vertex,
    },
    mesh::{Color, MeshBuilder, MeshInfo, COLOR_BLUE, COLOR_WHITE},
    prop::{
        PropertyBool, PropertyColor, PropertyFloat32, PropertyPtr, PropertyStr, PropertyUint32,
    },
    pubsub::Subscription,
    scene::{Pimpl, SceneGraph, SceneGraphPtr2, SceneNodeId},
    text2::{self, Glyph, GlyphPositionIter, SpritePtr, TextShaper, TextShaperPtr},
    util::zip3,
};

use super::{eval_rect, get_parent_rect, read_rect, DrawUpdate, OnModify, Stoppable};

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

pub type ChatViewPtr = Arc<ChatView>;

pub struct ChatView {
    node_id: SceneNodeId,
    render_api: RenderApiPtr,
    text_shaper: TextShaperPtr,
    tree: sled::Tree,

    pages: SyncMutex<Vec<Page>>,
    dc_key: u64,

    rect: PropertyPtr,
    scroll: PropertyFloat32,
    font_size: PropertyFloat32,
    line_height: PropertyFloat32,
    z_index: PropertyUint32,
}

impl ChatView {
    pub async fn new(
        sg: SceneGraphPtr2,
        node_id: SceneNodeId,
        render_api: RenderApiPtr,
        text_shaper: TextShaperPtr,
        tree: sled::Tree,
    ) -> Pimpl {
        let scene_graph = sg.lock().await;
        let node = scene_graph.get_node(node_id).unwrap();

        let rect = node.get_property("rect").expect("ChatView::rect");
        let scroll = PropertyFloat32::wrap(node, "scroll", 0).unwrap();
        let font_size = PropertyFloat32::wrap(node, "font_size", 0).unwrap();
        let line_height = PropertyFloat32::wrap(node, "line_height", 0).unwrap();
        let z_index = PropertyUint32::wrap(node, "z_index", 0).unwrap();

        drop(scene_graph);

        let self_ = Arc::new_cyclic(|me: &Weak<Self>| Self {
            node_id,
            render_api,
            text_shaper,
            tree,

            pages: SyncMutex::new(Vec::new()),
            dc_key: OsRng.gen(),

            rect,
            scroll,
            font_size,
            line_height,
            z_index,
        });

        self_.populate().await;

        Pimpl::ChatView(self_)
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

    async fn regen_mesh(&self, mut clip: Rectangle) -> Vec<DrawInstruction> {
        let font_size = self.font_size.get();
        let line_height = self.line_height.get();
        // Draw time and nick, then go over each word. If word crosses end of line
        // then apply a line break before the word and continue.
        let pages = self.pages.lock().unwrap().clone();

        let mut draws = vec![];
        let color = COLOR_WHITE;

        // Pages start at the bottom.
        let mut height = 0;
        'pageloop: for page in pages {
            let mut mesh = MeshBuilder::new();

            for msg in page.msgs {
                let glyphs = msg.glyphs;

                let mut lines = text2::wrap(clip.w, font_size, &glyphs);
                // We are drawing bottom up but line wrap gives us lines in normal order
                lines.reverse();
                for line in lines {
                    let px_height = height as f32 * line_height;

                    if px_height > clip.h {
                        break 'pageloop;
                    }

                    // Render line
                    let mut glyph_pos_iter =
                        GlyphPositionIter::new(font_size, &line, clip.h - px_height);
                    for (mut glyph_rect, glyph) in glyph_pos_iter.zip(line.iter()) {
                        let uv_rect =
                            page.atlas.fetch_uv(glyph.glyph_id).expect("missing glyph UV rect");
                        mesh.draw_box(&glyph_rect, color, uv_rect);
                    }

                    height += 1;
                }
            }

            let mesh = mesh.alloc(&self.render_api).await.unwrap();

            draws.push(DrawInstruction::Draw(DrawMesh {
                vertex_buffer: mesh.vertex_buffer,
                index_buffer: mesh.index_buffer,
                texture: Some(page.atlas.texture_id),
                num_elements: mesh.num_elements,
            }));
        }

        draws
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

        rect.x += parent_rect.x;
        rect.y += parent_rect.y;

        let mut drawcalls = self.regen_mesh(rect.clone()).await;
        // TODO: delete old buffers
        let mut freed_textures = vec![];
        let mut freed_buffers = vec![];

        // Apply scroll and scissor
        // We use the scissor for scrolling
        let off_x = rect.x / parent_rect.w;
        let off_y = rect.y / parent_rect.h;
        let scale_x = 1. / parent_rect.w;
        let scale_y = 1. / parent_rect.h;
        let model = glam::Mat4::from_translation(glam::Vec3::new(off_x, off_y, 0.)) *
            glam::Mat4::from_scale(glam::Vec3::new(scale_x, scale_y, 1.));

        let mut instrs = vec![DrawInstruction::ApplyMatrix(model)];
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
