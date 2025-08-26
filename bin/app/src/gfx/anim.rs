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
        Arc, RwLock,
    },
};

use super::{BufferId, DrawCall, GfxDrawCall, TextureId};

// This can be in instruction but also implement encodable
// maybe just remove trax?

/*
type FrameOpt = Arc<RwLock<Option<SequenceAnimationFrame>>>;

pub struct SequenceAnimBuffer {
    frames: Vec<FrameOpt>
}

impl SequenceAnimBuffer {
    pub fn new(len: usize) -> Self {
    }
}
*/

#[derive(Debug, Clone, SerialEncodable)]
pub struct SequenceAnimation {
    oneshot: bool,
    frames: Vec<SequenceAnimationFrame>,
}

impl SequenceAnimation {
    pub fn new(oneshot: bool, frames: Vec<SequenceAnimationFrame>) -> Self {
        //let frames = frames.into_iter().map(|f| Arc::new(RwLock::new(
        Self { oneshot, frames }
    }

    pub(super) fn compile(
        self: Self,
        textures: &HashMap<TextureId, miniquad::TextureId>,
        buffers: &HashMap<BufferId, miniquad::BufferId>,
    ) -> GfxSequenceAnimation {
        let mut frames = Vec::with_capacity(self.frames.len());
        for gfxframe in self.frames {
            let duration = std::time::Duration::from_millis(gfxframe.duration as u64);
            let dc = gfxframe.dc.compile(textures, buffers, 0).unwrap();
            frames.push(GfxGfxSequenceAnimationFrame { duration, dc });
        }
        GfxSequenceAnimation::new(self.oneshot, frames)
    }
}

#[derive(Debug, Clone)]
pub struct SequenceAnimationFrame {
    /// Duration of this frame in ms
    duration: u32,
    dc: DrawCall,
}

impl SequenceAnimationFrame {
    pub fn new(duration: u32, dc: DrawCall) -> Self {
        Self { duration, dc }
    }
}

/// We have to implement this manually due to macro autism.
/// Since it contains DrawCall that contains Instruction that can contain this.
impl Encodable for SequenceAnimationFrame {
    fn encode<S: Write>(&self, s: &mut S) -> std::result::Result<usize, std::io::Error> {
        let mut len = 0;
        len += self.duration.encode(s)?;
        len += self.dc.encode(s)?;
        Ok(len)
    }
}
#[async_trait]
impl AsyncEncodable for SequenceAnimationFrame {
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
pub(super) struct GfxSequenceAnimation {
    oneshot: bool,
    frames: Vec<GfxGfxSequenceAnimationFrame>,
    //incoming_frames: Vec<Arc<RwLock<Option<SequenceAnimationFrame>>>>,
    state: RefCell<GfxSequenceAnimationState>,
}

impl GfxSequenceAnimation {
    fn new(oneshot: bool, frames: Vec<GfxGfxSequenceAnimationFrame>) -> Self {
        Self {
            oneshot,
            frames,
            state: RefCell::new(GfxSequenceAnimationState { timer: None, current_idx: 0 }),
        }
    }

    pub fn tick(&self) -> GfxDrawCall {
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
struct GfxGfxSequenceAnimationFrame {
    duration: std::time::Duration,
    dc: GfxDrawCall,
}

#[derive(Debug, Clone)]
struct GfxSequenceAnimationState {
    /// Timer between frames
    timer: Option<std::time::Instant>,
    current_idx: usize,
}
