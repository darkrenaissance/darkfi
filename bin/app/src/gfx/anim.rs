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

use std::collections::HashMap;

use super::{BufferId, DrawCall, GfxDrawCall, TextureId};

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

#[derive(Debug, Clone)]
pub(super) struct GfxSeqAnim {
    oneshot: bool,
    frames: Vec<Option<GfxFrame>>,
    /// Timer between frames
    timer: std::time::Instant,
    current_idx: usize,
    pub(super) is_visible: bool,
}

impl GfxSeqAnim {
    pub fn new(frames_len: usize, oneshot: bool) -> Self {
        let frames = vec![None; frames_len];
        Self {
            oneshot,
            frames,
            timer: std::time::Instant::now(),
            current_idx: 0,
            is_visible: false,
        }
    }

    pub fn set(
        &mut self,
        frame_idx: usize,
        frame: Frame,
        textures: &HashMap<TextureId, miniquad::TextureId>,
        buffers: &HashMap<BufferId, miniquad::BufferId>,
    ) {
        assert!(frame_idx < self.frames.len());
        let duration = std::time::Duration::from_millis(frame.duration as u64);
        let dc = frame.dc.compile(textures, buffers, 0);
        self.frames[frame_idx] = Some(GfxFrame { duration, dc });
        //t!("got frame {frame_idx}");
    }

    fn curr_frame(&self) -> Option<&GfxFrame> {
        assert!(self.current_idx < self.frames.len());
        self.frames[self.current_idx].as_ref()
    }

    pub fn tick(&mut self) -> Option<GfxDrawCall> {
        //t!("tick");
        if self.curr_frame().is_none() {
            assert_eq!(self.current_idx, 0);
            return None
        };

        self.increment();

        let curr_frame = self.curr_frame().unwrap().clone();
        Some(curr_frame.dc)
    }

    fn increment(&mut self) {
        // One shot anims dont loop
        if self.oneshot && self.current_idx + 1 == self.frames.len() {
            return
        }

        let elapsed = self.timer.elapsed();
        let frame = self.curr_frame().unwrap();
        let curr_duration = frame.duration;

        if elapsed >= curr_duration {
            let next_idx = (self.current_idx + 1) % self.frames.len();
            // Only advance when the next frame is Some
            // Otherwise stay on the same frame
            if self.frames[next_idx].is_some() {
                self.current_idx = next_idx;
                // Reset the timer now we changed frame
                self.timer = std::time::Instant::now();
            }
        }
    }
}

#[derive(Debug, Clone)]
struct GfxFrame {
    duration: std::time::Duration,
    dc: GfxDrawCall,
}
