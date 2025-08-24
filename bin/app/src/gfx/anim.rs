/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
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
use darkfi_serial::{AsyncEncodable, AsyncWrite, Encodable, FutAsyncWriteExt, SerialEncodable};
use std::{
    cell::RefCell,
    collections::HashMap,
    io::Write,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
};

use super::{DrawCall, GfxBufferId, GfxDrawCall, GfxTextureId};

// This can be in instruction but also implement encodable
// maybe just remove trax?

#[derive(Debug, Clone, SerialEncodable)]
pub struct GfxSequenceAnimation {
    oneshot: bool,
    frames: Vec<GfxSequenceAnimationFrame>,
}

impl GfxSequenceAnimation {
    pub fn new(oneshot: bool, frames: Vec<GfxSequenceAnimationFrame>) -> Self {
        Self { oneshot, frames }
    }

    pub(super) fn compile(
        self: Self,
        textures: &HashMap<GfxTextureId, miniquad::TextureId>,
        buffers: &HashMap<GfxBufferId, miniquad::BufferId>,
    ) -> SequenceAnimation {
        let mut frames = Vec::with_capacity(self.frames.len());
        for gfxframe in self.frames {
            let duration = std::time::Duration::from_millis(gfxframe.duration as u64);
            let dc = gfxframe.dc.compile(textures, buffers, 0).unwrap();
            frames.push(SequenceAnimationFrame { duration, dc });
        }
        SequenceAnimation::new(self.oneshot, frames)
    }
}

#[derive(Debug, Clone)]
pub struct GfxSequenceAnimationFrame {
    /// Duration of this frame in ms
    duration: u32,
    dc: GfxDrawCall,
}

impl GfxSequenceAnimationFrame {
    pub fn new(duration: u32, dc: GfxDrawCall) -> Self {
        Self { duration, dc }
    }
}

/// We have to implement this manually due to macro autism.
/// Since it contains GfxDrawCall that contains Instruction that can contain this.
impl Encodable for GfxSequenceAnimationFrame {
    fn encode<S: Write>(&self, s: &mut S) -> std::result::Result<usize, std::io::Error> {
        let mut len = 0;
        len += self.duration.encode(s)?;
        len += self.dc.encode(s)?;
        Ok(len)
    }
}
#[async_trait]
impl AsyncEncodable for GfxSequenceAnimationFrame {
    async fn encode_async<W: AsyncWrite + Unpin + Send>(
        &self,
        w: &mut W,
    ) -> std::io::Result<usize> {
        let mut len = 0;
        len += self.duration.encode_async(w).await?;
        len += self.dc.encode_async(w).await?;
        Ok(len)
    }
}

#[derive(Debug, Clone)]
pub(super) struct SequenceAnimation {
    oneshot: bool,
    frames: Vec<SequenceAnimationFrame>,
    state: RefCell<SequenceAnimationState>,
}

impl SequenceAnimation {
    fn new(oneshot: bool, frames: Vec<SequenceAnimationFrame>) -> Self {
        Self {
            oneshot,
            frames,
            state: RefCell::new(SequenceAnimationState { timer: None, current_idx: 0 }),
        }
    }

    pub fn tick(&self) -> DrawCall {
        let mut state = self.state.borrow_mut();

        let elapsed = state.timer.get_or_insert_with(|| std::time::Instant::now()).elapsed();
        assert!(state.current_idx < self.frames.len());
        if elapsed >= self.frames[state.current_idx].duration {
            state.current_idx = (state.current_idx + 1) % self.frames.len();
        }

        self.frames[state.current_idx].dc.clone()
    }
}

#[derive(Debug, Clone)]
struct SequenceAnimationFrame {
    duration: std::time::Duration,
    dc: DrawCall,
}

#[derive(Debug, Clone)]
struct SequenceAnimationState {
    /// Timer between frames
    timer: Option<std::time::Instant>,
    current_idx: usize,
}
