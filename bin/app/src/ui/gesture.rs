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
use darkfi_serial::serialize;
use miniquad::TouchPhase;
use std::sync::{Arc, Mutex as SyncMutex};

use crate::{
    gfx::Point,
    prop::{PropertyUint32, Role},
    scene::{Pimpl, SceneNodeWeak},
};

use super::UIObject;

macro_rules! d { ($($arg:tt)*) => { debug!(target: "ui::gesture", $($arg)*); } }
macro_rules! t { ($($arg:tt)*) => { trace!(target: "ui::gesture", $($arg)*); } }

/// Maximum number of simultaneous touch events.
/// Put 3 here because any more is ridiculous.
const MAX_TOUCH: usize = 3;

#[derive(Clone)]
struct GestureState {
    start: [Option<Point>; MAX_TOUCH],
    curr: [Option<Point>; MAX_TOUCH],
}

pub type GesturePtr = Arc<Gesture>;

pub struct Gesture {
    node: SceneNodeWeak,
    priority: PropertyUint32,
    state: SyncMutex<GestureState>,
}

impl Gesture {
    pub async fn new(node: SceneNodeWeak) -> Pimpl {
        t!("Gesture::new()");

        let node_ref = &node.upgrade().unwrap();
        let priority = PropertyUint32::wrap(node_ref, Role::Internal, "priority", 0).unwrap();

        let state = GestureState { start: [None; MAX_TOUCH], curr: [None; MAX_TOUCH] };

        let self_ = Arc::new(Self { node, priority, state: SyncMutex::new(state) });

        Pimpl::Gesture(self_)
    }

    fn handle_update(&self, state: GestureState) -> Option<f32> {
        let Some(start_1) = state.start[0] else { return None };
        let curr_1 = state.curr[0].unwrap();

        let Some(start_2) = state.start[1] else { return None };
        let curr_2 = state.curr[1].unwrap();

        let start_dist_sq = start_1.dist_sq(start_2);
        let curr_dist_sq = curr_1.dist_sq(curr_2);
        let r = (curr_dist_sq / start_dist_sq).sqrt();

        Some(r)
    }
}

#[async_trait]
impl UIObject for Gesture {
    fn priority(&self) -> u32 {
        self.priority.get()
    }

    async fn handle_touch(&self, phase: TouchPhase, id: u64, touch_pos: Point) -> bool {
        //t!("handle_touch({phase:?}, {id}, {touch_pos:?})");
        let id = id as usize;
        if id >= MAX_TOUCH {
            return false
        }

        match phase {
            TouchPhase::Started => {
                let mut state = self.state.lock().unwrap();
                state.start[id] = Some(touch_pos);
                state.curr[id] = Some(touch_pos);
                false
            }
            TouchPhase::Moved => {
                let state = {
                    let mut state = self.state.lock().unwrap();
                    state.curr[id] = Some(touch_pos);
                    state.clone()
                };

                if let Some(update) = self.handle_update(state) {
                    let node = self.node.upgrade().unwrap();
                    d!("Gesture invoked: {update}");
                    node.trigger("gesture", serialize(&update)).await.unwrap();
                }

                false
            }
            TouchPhase::Ended | TouchPhase::Cancelled => {
                let mut state = self.state.lock().unwrap();
                state.start = [None; MAX_TOUCH];
                state.curr = [None; MAX_TOUCH];
                false
            }
        }
    }
}
