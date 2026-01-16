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

use std::collections::{HashMap, HashSet};

use super::{
    anim::GfxSeqAnim, AnimId, BufferId, EpochIndex, GraphicsMethod, TextureId, DEBUG_GFXAPI,
};
use crate::prop::BatchGuardId;

macro_rules! t { ($($arg:tt)*) => { trace!(target: "gfx::prune", $($arg)*) }; }

struct PendingAnim {
    new_method: GraphicsMethod,
    updates: HashMap<usize, GraphicsMethod>,
}

/// This is used to process the method queue while the screen is off to avoid the queue
/// becoming congested and using up all the memory.
/// Will drop alloc/delete pairs, and merge draw calls together.
pub struct PruneMethodHeap {
    /// Newly allocated buffers while screen was off
    new_buf: HashMap<BufferId, GraphicsMethod>,
    /// Newly allocated textures while screen was off
    new_tex: HashMap<TextureId, GraphicsMethod>,
    /// Deleted objects
    del: Vec<GraphicsMethod>,

    new_anim: HashMap<AnimId, PendingAnim>,
    /// Existing anim updates
    anim_updates: HashMap<AnimId, HashMap<usize, GraphicsMethod>>,
    /// Existing anim deletes
    anim_deletes: HashSet<AnimId>,

    epoch: EpochIndex,

    pub textures: *const HashMap<TextureId, miniquad::TextureId>,
    pub buffers: *const HashMap<BufferId, miniquad::BufferId>,
    pub anims: *const HashMap<AnimId, GfxSeqAnim>,
    pub dropped_batches: *mut HashSet<BatchGuardId>,
}

impl PruneMethodHeap {
    pub fn new(epoch: EpochIndex) -> Self {
        Self {
            new_buf: HashMap::new(),
            new_tex: HashMap::new(),
            del: vec![],
            new_anim: HashMap::new(),
            anim_updates: HashMap::new(),
            anim_deletes: HashSet::new(),
            epoch,
            textures: std::ptr::null(),
            buffers: std::ptr::null(),
            anims: std::ptr::null(),
            dropped_batches: std::ptr::null_mut(),
        }
    }

    #[instrument(skip_all, target = "gfx::pruner")]
    pub fn drain(&mut self, method_recv: &async_channel::Receiver<(EpochIndex, GraphicsMethod)>) {
        // Process as many methods as we can
        while let Ok((epoch, method)) = method_recv.try_recv() {
            if epoch < self.epoch {
                // Discard old rubbish
                t!(
                    "Discard method with old epoch: {epoch} curr: {} [method={method:?}]",
                    self.epoch
                );
                continue
            }
            assert_eq!(epoch, self.epoch);
            self.process_method(method);
        }
    }

    fn process_method(&mut self, mut method: GraphicsMethod) {
        match &method {
            GraphicsMethod::NewTexture((_, _, _, _, gtex_id, _)) => {
                if DEBUG_GFXAPI {
                    t!("Prune method: new_texture(..., {gtex_id})");
                }
                self.new_tex.insert(*gtex_id, std::mem::take(&mut method));
            }
            GraphicsMethod::DeleteTexture((gtex_id, _)) => {
                if DEBUG_GFXAPI {
                    t!("Prune method: delete_texture(..., {gtex_id})");
                }
                if self.new_tex.remove(&gtex_id).is_none() {
                    if !self.textures().contains_key(&gtex_id) {
                        panic!("delete_texture missing ID {gtex_id} in pruner")
                    }
                    let method = std::mem::take(&mut method);
                    self.del.push(method);
                } else if DEBUG_GFXAPI {
                    t!("Discard ellided texture {gtex_id}");
                }
            }
            GraphicsMethod::NewVertexBuffer((_, gbuff_id, _)) => {
                if DEBUG_GFXAPI {
                    t!("Prune method: new_vertex_buffer(..., {gbuff_id})");
                }
                self.new_buf.insert(*gbuff_id, std::mem::take(&mut method));
            }
            GraphicsMethod::NewIndexBuffer((_, gbuff_id, _)) => {
                if DEBUG_GFXAPI {
                    t!("Prune method: new_index_buffer(..., {gbuff_id})");
                }
                self.new_buf.insert(*gbuff_id, std::mem::take(&mut method));
            }
            GraphicsMethod::DeleteBuffer((gbuff_id, _, _)) => {
                if DEBUG_GFXAPI {
                    t!("Prune method: delete_buffer(..., {gbuff_id})");
                }
                if self.new_buf.remove(&gbuff_id).is_none() {
                    if !self.buffers().contains_key(&gbuff_id) {
                        panic!("delete_buffer missing ID {gbuff_id} in pruner")
                    }
                    let method = std::mem::take(&mut method);
                    self.del.push(method);
                } else if DEBUG_GFXAPI {
                    t!("Discard ellided buffer {gbuff_id}");
                }
            }
            GraphicsMethod::NewSeqAnim { id, .. } => {
                self.new_anim.insert(
                    *id,
                    PendingAnim {
                        updates: HashMap::new(),
                        new_method: std::mem::take(&mut method),
                    },
                );
            }

            GraphicsMethod::UpdateSeqAnim { id, frame_idx, .. } => {
                if let Some(pending) = self.new_anim.get_mut(id) {
                    pending.updates.insert(*frame_idx, method);
                } else if self.anims().contains_key(id) {
                    self.anim_updates.entry(*id).or_default().insert(*frame_idx, method);
                } else {
                    panic!("UpdateSeqAnim for unknown anim {id}");
                }
            }

            GraphicsMethod::DeleteSeqAnim((id, _)) => {
                if self.new_anim.remove(id).is_some() {
                } else if self.anims().contains_key(id) {
                    self.anim_deletes.insert(*id);
                    self.anim_updates.remove(id);
                } else {
                    panic!("DeleteSeqAnim for unknown anim {id}");
                }
            }
            GraphicsMethod::ReplaceGfxDrawCalls { .. } => {}
            // Discard batches since we will apply everything all at once anyway
            // once the screen is switched on.
            GraphicsMethod::StartBatch { batch_id, tag } => {
                t!("Pruner drop start batch {batch_id} debug={tag:?}");
                if !self.dropped_batches().insert(*batch_id) {
                    panic!("dropped batch {batch_id} already exits!");
                }
            }
            GraphicsMethod::EndBatch { batch_id, timest: _ } => {
                t!("Pruner drop end batch {batch_id}");
                // Should have already been dropped previously
                assert!(self.dropped_batches().contains(batch_id));
            }
            GraphicsMethod::Noop => panic!("noop"),
        }
    }

    fn textures(&self) -> &HashMap<TextureId, miniquad::TextureId> {
        assert!(!self.textures.is_null());
        unsafe { &*self.textures }
    }
    fn buffers(&self) -> &HashMap<BufferId, miniquad::BufferId> {
        assert!(!self.buffers.is_null());
        unsafe { &*self.buffers }
    }
    fn anims(&self) -> &HashMap<AnimId, GfxSeqAnim> {
        assert!(!self.anims.is_null());
        unsafe { &*self.anims }
    }
    fn dropped_batches(&mut self) -> &mut HashSet<BatchGuardId> {
        assert!(!self.dropped_batches.is_null());
        unsafe { &mut *self.dropped_batches }
    }

    /// Collect everything now the screen is on
    pub fn recv_all(&mut self) -> Vec<GraphicsMethod> {
        // Inhale that smoke deep
        let mut meth = Vec::with_capacity(
            self.new_buf.len() +
                self.new_tex.len() +
                self.del.len() +
                self.new_anim.len() +
                self.anim_updates.len() +
                self.anim_deletes.len(),
        );

        self.drain_resources(&mut meth);
        self.drain_anims(&mut meth);

        meth
    }

    fn drain_resources(&mut self, meth: &mut Vec<GraphicsMethod>) {
        let new_buf = std::mem::take(&mut self.new_buf);
        let new_tex = std::mem::take(&mut self.new_tex);
        meth.extend(new_buf.into_values());
        meth.extend(new_tex.into_values());
        meth.append(&mut self.del);
    }

    fn drain_anims(&mut self, meth: &mut Vec<GraphicsMethod>) {
        self.drain_pending_anims(meth);
        self.drain_live_anims(meth);
    }

    fn drain_pending_anims(&mut self, meth: &mut Vec<GraphicsMethod>) {
        for (_id, pending) in std::mem::take(&mut self.new_anim) {
            meth.push(pending.new_method);

            let mut updates: Vec<_> = pending.updates.into_iter().collect();
            updates.sort_by_key(|(idx, _)| *idx);
            for (_, update) in updates {
                meth.push(update);
            }
        }
    }

    fn drain_live_anims(&mut self, meth: &mut Vec<GraphicsMethod>) {
        let mut updates: Vec<_> = self
            .anim_updates
            .drain()
            .flat_map(|(anim_id, frame_updates)| {
                let mut sorted: Vec<_> = frame_updates.into_iter().collect();
                sorted.sort_by_key(|(idx, _)| *idx);
                sorted.into_iter().map(move |(idx, update)| (anim_id, idx, update))
            })
            .collect();
        updates.sort_by_key(|(anim_id, idx, _)| (*anim_id, *idx));
        for (_, _, update) in updates {
            meth.push(update);
        }

        for id in std::mem::take(&mut self.anim_deletes) {
            meth.push(GraphicsMethod::DeleteSeqAnim((id, None)));
        }
    }
}
