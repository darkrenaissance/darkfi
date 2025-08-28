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
use darkfi_serial::{
    AsyncEncodable, AsyncWrite, Encodable, FutAsyncWriteExt, SerialEncodable, VarInt,
};
use parking_lot::RwLock;
use std::{
    cell::RefCell,
    collections::HashMap,
    io::Write,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
};

use super::{BufferId, DrawCall, GfxDrawCall, TextureId};

macro_rules! t { ($($arg:tt)*) => { trace!(target: "gfx::anim", $($arg)*); } }

#[derive(Debug, Clone)]
pub struct SeqAnim {
    oneshot: bool,
    frames: Vec<Option<Frame>>,
    recv_frames: async_channel::Receiver<(usize, Frame)>,
    state: State,
}

impl SeqAnim {
    pub fn new(
        oneshot: bool,
        frames: Vec<Option<Frame>>,
        recv_frames: async_channel::Receiver<(usize, Frame)>,
        state: State,
    ) -> Self {
        Self { oneshot, frames, recv_frames, state }
    }

    pub(super) fn compile(
        mut self: Self,
        textures: &HashMap<TextureId, miniquad::TextureId>,
        buffers: &HashMap<BufferId, miniquad::BufferId>,
    ) -> GfxSeqAnim {
        let mut frames = Vec::with_capacity(self.frames.len());
        for frame in self.frames {
            let Some(frame) = frame else {
                frames.push(None);
                continue
            };
            let duration = std::time::Duration::from_millis(frame.duration as u64);
            let dc = frame.dc.compile(textures, buffers, 0).unwrap();
            frames.push(Some(GfxFrame { duration, dc }));
        }
        GfxSeqAnim::new(self.oneshot, frames, self.recv_frames, self.state)
    }
}

impl Encodable for SeqAnim {
    fn encode<S: Write>(&self, s: &mut S) -> std::result::Result<usize, std::io::Error> {
        let mut len = 0;
        len += self.oneshot.encode(s)?;
        // Write frames array
        /*
        len += VarInt(self.frames.len() as u64).encode(s)?;
        for frame in &self.frames {
            let frame = frame.read();
            frame.encode(s)?;
        }
        */
        Ok(len)
    }
}
#[async_trait]
impl AsyncEncodable for SeqAnim {
    async fn encode_async<W: AsyncWrite + Unpin + Send>(
        &self,
        w: &mut W,
    ) -> std::io::Result<usize> {
        let mut len = 0;
        len += self.oneshot.encode_async(w).await?;
        // Write frames array
        /*
        len += VarInt(self.frames.len() as u64).encode_async(w).await?;
        for frame in &self.frames {
            let frame = frame.read().clone();
            frame.encode_async(w).await?;
        }
        */
        Ok(len)
    }
}

#[derive(Debug, Clone)]
pub struct Frame {
    /// Duration of this frame in ms
    duration: u32,
    dc: DrawCall,
}

impl Frame {
    pub fn new(duration: u32, dc: DrawCall) -> Self {
        Self { duration, dc }
    }
}

/// We have to implement this manually due to macro autism.
/// Since it contains DrawCall that contains Instruction that can contain this.
impl Encodable for Frame {
    fn encode<S: Write>(&self, s: &mut S) -> std::result::Result<usize, std::io::Error> {
        let mut len = 0;
        len += self.duration.encode(s)?;
        len += self.dc.encode(s)?;
        Ok(len)
    }
}
#[async_trait]
impl AsyncEncodable for Frame {
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

#[derive(Debug)]
struct InternalState {
    /// Timer between frames
    timer: std::time::Instant,
    current_idx: usize,
}

type InternalStatePtr = Arc<RefCell<InternalState>>;

#[derive(Debug, Clone)]
pub struct State(InternalStatePtr);

impl State {
    pub fn new() -> Self {
        Self(Arc::new(RefCell::new(InternalState {
            timer: std::time::Instant::now(),
            current_idx: 0,
        })))
    }
}

unsafe impl Send for State {}
unsafe impl Sync for State {}

#[derive(Debug, Clone)]
pub(super) struct GfxSeqAnim {
    oneshot: bool,
    frames: Vec<Option<GfxFrame>>,
    /// Stream frames in
    recv_frames: async_channel::Receiver<(usize, Frame)>,
    state: State,
}

impl GfxSeqAnim {
    fn new(
        oneshot: bool,
        frames: Vec<Option<GfxFrame>>,
        recv_frames: async_channel::Receiver<(usize, Frame)>,
        state: State,
    ) -> Self {
        Self { oneshot, frames, recv_frames, state }
    }

    pub fn tick(
        &mut self,
        textures: &HashMap<TextureId, miniquad::TextureId>,
        buffers: &HashMap<BufferId, miniquad::BufferId>,
    ) -> Option<GfxDrawCall> {
        t!("tick");
        while let Ok((frame_idx, frame)) = self.recv_frames.try_recv() {
            let duration = std::time::Duration::from_millis(frame.duration as u64);
            let dc = frame.dc.compile(textures, buffers, 0).unwrap();
            self.frames[frame_idx] = Some(GfxFrame { duration, dc });
            t!("got frame {frame_idx}");
            for i in 0..self.frames.len() {
                if self.frames[i].is_none() {
                    t!("frame {i} is none");
                }
            }
        }

        let mut state = self.state.0.borrow_mut();

        let elapsed = state.timer.elapsed();
        assert!(state.current_idx < self.frames.len());
        let frame = &self.frames[state.current_idx];
        let Some(frame) = frame else {
            assert_eq!(state.current_idx, 0);
            return None
        };

        let curr_duration = frame.duration;
        if elapsed >= curr_duration {
            let next_idx = (state.current_idx + 1) % self.frames.len();
            // Only advance when the next frame is Some
            // Otherwise stay on the same frame
            if self.frames[next_idx].is_some() {
                state.current_idx = next_idx;
                // Reset the timer now we changed frame
                state.timer = std::time::Instant::now();
            }
        }

        let curr_frame = self.frames[state.current_idx].clone().unwrap();
        Some(curr_frame.dc)
    }
}

#[derive(Debug, Clone)]
struct GfxFrame {
    duration: std::time::Duration,
    dc: GfxDrawCall,
}
