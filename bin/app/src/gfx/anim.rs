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

use std::cell::RefCell;

use super::DrawCall;

pub(super) trait AbstractAnimation: std::fmt::Debug {
    fn tick(&self) -> DrawCall;
}

#[derive(Debug)]
struct SequenceAnimation {
    oneshot: bool,
    frames: Vec<SequenceAnimationFrame>,
    state: RefCell<SequenceAnimationState>,
}
#[derive(Debug)]
struct SequenceAnimationFrame {
    dc: DrawCall,
    duration: std::time::Duration,
}
#[derive(Debug)]
struct SequenceAnimationState {
    /// Timer between frames
    timer: Option<std::time::Instant>,
    current_idx: usize,
}

impl AbstractAnimation for SequenceAnimation {
    fn tick(&self) -> DrawCall {
        let mut state = self.state.borrow_mut();

        let elapsed = state.timer.get_or_insert_with(|| std::time::Instant::now()).elapsed();
        assert!(state.current_idx < self.frames.len());
        if elapsed >= self.frames[state.current_idx].duration {
            state.current_idx = (state.current_idx + 1) % self.frames.len();
        }

        self.frames[state.current_idx].dc.clone()
    }
}
