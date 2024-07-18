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

#[derive(Debug, SerialEncodable, SerialDecodable)]
pub struct ChatLine {
    pub nick: String,
    pub text: String,
}

type Timestamp = u32;

struct RenderLine {
    timest: Timestamp,
    chatline: ChatLine,
    glyphs: Vec<Glyph>,
}

pub type ChatViewPtr = Arc<ChatView>;

pub struct ChatView {
    node_id: SceneNodeId,
    render_api: RenderApiPtr,
    text_shaper: TextShaperPtr,
    tree: sled::Tree,

    lines: SyncMutex<Vec<RenderLine>>,
    drawcalls: SyncMutex<Option<Vec<DrawInstruction>>>,
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

            lines: SyncMutex::new(Vec::new()),
            drawcalls: SyncMutex::new(None),
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
        let mut lines = vec![];

        for entry in self.tree.iter().rev() {
            let Ok((k, v)) = entry else { break };
            assert_eq!(k.len(), 4);
            let key_bytes: [u8; 4] = k.as_ref().try_into().unwrap();
            let timest = Timestamp::from_be_bytes(key_bytes);
            let chatline: ChatLine = deserialize(&v).unwrap();
            //println!("{k:?} {chatline:?}");

            let timestr = timest.to_string();
            // left pad with zeros
            let mut timestr = format!("{:0>4}", timestr);
            timestr.insert(2, ':');

            let text = format!("{} {} {}", timestr, chatline.nick, chatline.text);
            let glyphs = self.text_shaper.shape(text, self.font_size.get()).await;

            lines.push(RenderLine { timest, chatline, glyphs });
        }
        *self.lines.lock().unwrap() = lines;
    }

    async fn regen_mesh(&self, mut clip: Rectangle) -> Vec<DrawInstruction> {
        // Draw time and nick, then go over each word. If word crosses end of line
        // then apply a line break before the word and continue.
        vec![]
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

        let drawcalls = self.drawcalls.lock().unwrap().clone();
        let mut drawcalls = match drawcalls {
            Some(drawcalls) => drawcalls,
            None => {
                let drawcalls = self.regen_mesh(rect.clone()).await;
                *self.drawcalls.lock().unwrap() = Some(drawcalls.clone());
                drawcalls
            }
        };

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
        })
    }
}
