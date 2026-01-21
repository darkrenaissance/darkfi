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

use async_trait::async_trait;
use parking_lot::Mutex as SyncMutex;
use rand::{rngs::OsRng, Rng};
use std::sync::Arc;
use tracing::instrument;

use crate::{
    gfx::{gfxtag, DrawCall, DrawInstruction, DrawMesh, Point, Rectangle, RenderApi, Renderer},
    prop::{
        BatchGuardPtr, PropertyAtomicGuard, PropertyBool, PropertyFloat32, PropertyRect,
        PropertyUint32, Role,
    },
    scene::{Pimpl, SceneNodeWeak},
    ExecutorPtr,
};

use super::{DrawUpdate, OnModify, UIObject};

pub mod shape;
use shape::VectorShape;

pub type VectorArtPtr = Arc<VectorArt>;

pub struct VectorArt {
    node: SceneNodeWeak,
    renderer: Renderer,
    tasks: SyncMutex<Vec<smol::Task<()>>>,

    shape: VectorShape,
    dc_key: u64,

    is_visible: PropertyBool,
    rect: PropertyRect,
    scale: PropertyFloat32,
    z_index: PropertyUint32,
    priority: PropertyUint32,

    parent_rect: SyncMutex<Option<Rectangle>>,
}

impl VectorArt {
    pub async fn new(node: SceneNodeWeak, shape: VectorShape, renderer: Renderer) -> Pimpl {
        let node_ref = &node.upgrade().unwrap();
        let is_visible = PropertyBool::wrap(node_ref, Role::Internal, "is_visible", 0).unwrap();
        let rect = PropertyRect::wrap(node_ref, Role::Internal, "rect").unwrap();
        let scale = PropertyFloat32::wrap(node_ref, Role::Internal, "scale", 0).unwrap();
        let z_index = PropertyUint32::wrap(node_ref, Role::Internal, "z_index", 0).unwrap();
        let priority = PropertyUint32::wrap(node_ref, Role::Internal, "priority", 0).unwrap();

        let self_ = Arc::new(Self {
            node,
            renderer,
            tasks: SyncMutex::new(vec![]),

            shape,
            dc_key: OsRng.gen(),

            is_visible,
            rect,
            scale,
            z_index,
            priority,

            parent_rect: SyncMutex::new(None),
        });

        Pimpl::VectorArt(self_)
    }

    #[instrument(target = "ui::vector_art")]
    async fn redraw(self: Arc<Self>, batch: BatchGuardPtr) {
        let Some(parent_rect) = self.parent_rect.lock().clone() else {
            warn!(target: "ui:vector_art", "Skip draw since parent rect is empty");
            return
        };

        let atom = &mut batch.spawn();
        let Some(draw_update) = self.get_draw_calls(atom, parent_rect) else {
            error!(target: "ui:vector_art", "Mesh failed to draw");
            return
        };
        self.renderer.replace_draw_calls(Some(batch.id), draw_update.draw_calls);
    }

    fn get_draw_instrs(&self) -> Vec<DrawInstruction> {
        if !self.is_visible.get() {
            //t!("Skipping draw for invisible node");
            return vec![]
        }

        let rect = self.rect.get();
        let scale = self.scale.get();
        let mut verts = self.shape.eval(rect.w, rect.h).expect("bad shape");
        let indices = self.shape.indices.clone();
        let num_elements = self.shape.indices.len() as i32;

        // Apply scaling
        for v in &mut verts {
            v.pos[0] *= scale;
            v.pos[1] *= scale;
        }

        //debug!(target: "ui::vector_art", "vec_draw_instrs {verts:?} | {indices:?} | {num_elements}");
        let vertex_buffer = self.renderer.new_vertex_buffer(verts, gfxtag!("vectorart"));
        let index_buffer = self.renderer.new_index_buffer(indices, gfxtag!("vectorart"));
        let mesh = DrawMesh { vertex_buffer, index_buffer, textures: None, num_elements };

        vec![DrawInstruction::Move(rect.pos()), DrawInstruction::Draw(mesh)]
    }

    fn get_draw_calls(
        &self,
        atom: &mut PropertyAtomicGuard,
        parent_rect: Rectangle,
    ) -> Option<DrawUpdate> {
        if let Err(e) = self.rect.eval(atom, &parent_rect) {
            warn!(target: "ui::vector_art", "Rect eval failure: {e}");
            return None
        }
        let instrs = self.get_draw_instrs();
        Some(DrawUpdate {
            key: self.dc_key,
            draw_calls: vec![(
                self.dc_key,
                DrawCall::new(instrs, vec![], self.z_index.get(), "vecart"),
            )],
        })
    }
}

#[async_trait]
impl UIObject for VectorArt {
    fn priority(&self) -> u32 {
        self.priority.get()
    }

    async fn start(self: Arc<Self>, ex: ExecutorPtr) {
        let me = Arc::downgrade(&self);

        let mut on_modify = OnModify::new(ex, self.node.clone(), me.clone());
        on_modify.when_change(self.is_visible.prop(), Self::redraw);
        on_modify.when_change(self.rect.prop(), Self::redraw);
        on_modify.when_change(self.scale.prop(), Self::redraw);
        on_modify.when_change(self.z_index.prop(), Self::redraw);

        *self.tasks.lock() = on_modify.tasks;
    }

    fn stop(&self) {
        self.tasks.lock().clear();
        *self.parent_rect.lock() = None;
    }

    #[instrument(target = "ui::vector_art")]
    async fn draw(
        &self,
        parent_rect: Rectangle,
        atom: &mut PropertyAtomicGuard,
    ) -> Option<DrawUpdate> {
        *self.parent_rect.lock() = Some(parent_rect);
        self.get_draw_calls(atom, parent_rect)
    }
}

impl Drop for VectorArt {
    fn drop(&mut self) {
        let atom = self.renderer.make_guard(gfxtag!("VectorArt::drop"));
        self.renderer
            .replace_draw_calls(Some(atom.batch_id), vec![(self.dc_key, Default::default())]);
    }
}

impl std::fmt::Debug for VectorArt {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self.node.upgrade().unwrap())
    }
}
