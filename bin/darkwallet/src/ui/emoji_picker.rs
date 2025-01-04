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

use async_trait::async_trait;
use image::ImageReader;
use rand::{rngs::OsRng, Rng};
use std::{
    io::Cursor,
    sync::{Arc, Mutex as SyncMutex, OnceLock, Weak},
};

use crate::{
    gfx::{
        GfxDrawCall, GfxDrawInstruction, GfxDrawMesh, GfxTextureId, ManagedTexturePtr, Rectangle,
        RenderApi,
    },
    mesh::{MeshBuilder, MeshInfo, COLOR_WHITE},
    prop::{PropertyFloat32, PropertyPtr, PropertyRect, PropertyStr, PropertyUint32, Role},
    scene::{Pimpl, SceneNodePtr, SceneNodeWeak},
    text::{self, GlyphPositionIter, TextShaper, TextShaperPtr},
    ExecutorPtr,
};

use super::{DrawUpdate, OnModify, UIObject};

macro_rules! d {
    ($($arg:tt)*) => {
        debug!(target: "ui::emoji_picker", $($arg)*);
    }
}

pub type EmojiPickerPtr = Arc<EmojiPicker>;

pub struct EmojiPicker {
    node: SceneNodeWeak,
    render_api: RenderApi,
    text_shaper: TextShaperPtr,
    tasks: OnceLock<Vec<smol::Task<()>>>,

    dc_key: u64,

    rect: PropertyRect,
    z_index: PropertyUint32,

    window_scale: PropertyFloat32,
    parent_rect: SyncMutex<Option<Rectangle>>,
}

impl EmojiPicker {
    pub async fn new(
        node: SceneNodeWeak,
        window_scale: PropertyFloat32,
        render_api: RenderApi,
        text_shaper: TextShaperPtr,
        ex: ExecutorPtr,
    ) -> Pimpl {
        d!("EmojiPicker::new()");

        let node_ref = &node.upgrade().unwrap();
        let rect = PropertyRect::wrap(node_ref, Role::Internal, "rect").unwrap();
        let z_index = PropertyUint32::wrap(node_ref, Role::Internal, "z_index", 0).unwrap();

        let node_name = node_ref.name.clone();
        let node_id = node_ref.id;

        let self_ = Arc::new(Self {
            node,
            render_api,
            text_shaper,
            tasks: OnceLock::new(),

            dc_key: OsRng.gen(),

            rect,
            z_index,

            window_scale,
            parent_rect: SyncMutex::new(None),
        });

        Pimpl::EmojiPicker(self_)
    }

    async fn redraw(self: Arc<Self>) {
        let Some(parent_rect) = self.parent_rect.lock().unwrap().clone() else { return };

        let Some(draw_update) = self.get_draw_calls(parent_rect).await else {
            error!(target: "ui::image", "Emoji picker failed to draw");
            return;
        };
        self.render_api.replace_draw_calls(draw_update.draw_calls);
        debug!(target: "ui::image", "replace draw calls done");
    }

    /*
    fn regen_mesh(&self) -> MeshInfo {
        let rect = self.rect.get();
        let uv = self.uv.get();
        let mesh_rect = Rectangle::from([0., 0., rect.w, rect.h]);
        let mut mesh = MeshBuilder::new();
        mesh.draw_box(&mesh_rect, COLOR_WHITE, &uv);
        mesh.alloc(&self.render_api)
    }
    */

    async fn get_draw_calls(&self, parent_rect: Rectangle) -> Option<DrawUpdate> {
        if let Err(e) = self.rect.eval(&parent_rect) {
            warn!(target: "ui::emoji_picker", "Rect eval failed: {e}");
            return None
        }
        let rect = self.rect.get();

        /*
        let mesh = self.regen_mesh();
        let texture = self.texture.lock().unwrap().clone().expect("Node missing texture_id!");

        let mesh = GfxDrawMesh {
            vertex_buffer: mesh.vertex_buffer,
            index_buffer: mesh.index_buffer,
            texture: Some(texture),
            num_elements: mesh.num_elements,
        };
        */

        Some(DrawUpdate {
            key: self.dc_key,
            draw_calls: vec![(
                self.dc_key,
                GfxDrawCall {
                    instrs: vec![
                        GfxDrawInstruction::Move(rect.pos()),
                        //GfxDrawInstruction::Draw(mesh),
                    ],
                    dcs: vec![],
                    z_index: self.z_index.get(),
                },
            )],
        })
    }
}

#[async_trait]
impl UIObject for EmojiPicker {
    fn z_index(&self) -> u32 {
        self.z_index.get()
    }

    async fn start(self: Arc<Self>, ex: ExecutorPtr) {
        let me = Arc::downgrade(&self);

        let node_ref = &self.node.upgrade().unwrap();
        let node_name = node_ref.name.clone();
        let node_id = node_ref.id;

        let mut on_modify = OnModify::new(ex, node_name, node_id, me.clone());
        on_modify.when_change(self.rect.prop(), Self::redraw);
        on_modify.when_change(self.z_index.prop(), Self::redraw);

        self.tasks.set(on_modify.tasks);
    }

    async fn draw(&self, parent_rect: Rectangle) -> Option<DrawUpdate> {
        debug!(target: "ui::image", "Image::draw()");
        *self.parent_rect.lock().unwrap() = Some(parent_rect);
        self.get_draw_calls(parent_rect).await
    }
}

impl Drop for EmojiPicker {
    fn drop(&mut self) {
        self.render_api.replace_draw_calls(vec![(self.dc_key, Default::default())]);
    }
}
