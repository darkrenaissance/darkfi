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

use miniquad::KeyCode;
use std::{collections::HashMap, time::Instant};

#[derive(Debug, Clone, Eq, Hash, PartialEq)]
pub enum PressedKey {
    Char(char),
    Key(KeyCode),
}

/// On key press (repeat=false), we immediately process the event.
/// Then there's a delay (repeat=true) and then for every step time
/// while key press events are being sent, we allow an event.
/// This ensures smooth typing in the editbox.
pub struct PressedKeysSmoothRepeat {
    /// When holding keys, we track from start and last sent time.
    /// This is useful for initial delay and smooth scrolling.
    pressed_keys: HashMap<PressedKey, RepeatingKeyTimer>,
    /// Initial delay before allowing keys
    start_delay: u32,
    /// Minimum time between repeated keys
    step_time: u32,
}

impl PressedKeysSmoothRepeat {
    pub fn new(start_delay: u32, step_time: u32) -> Self {
        Self { pressed_keys: HashMap::new(), start_delay, step_time }
    }

    pub fn clear(&mut self) {
        self.pressed_keys.clear()
    }

    pub fn key_down(&mut self, key: PressedKey, repeat: bool) -> u32 {
        trace!(target: "PressedKeysSmoothRepeat", "key_down({:?}, {})", key, repeat);

        let is_initial_keypress = !repeat;
        if is_initial_keypress {
            trace!(target: "PressedKeysSmoothRepeat", "remove key {:?}", key);
            self.pressed_keys.remove(&key);
            return 1
        }

        // Insert key if not exists
        if !self.pressed_keys.contains_key(&key) {
            trace!(target: "PressedKeysSmoothRepeat", "insert key {:?}", key);
            self.pressed_keys.insert(key.clone(), RepeatingKeyTimer::new());
        }

        let repeater = self.pressed_keys.get_mut(&key).expect("repeat map");
        let actions = repeater.update(self.start_delay, self.step_time);
        // This is a temporary workaround due to a miniquad issue.
        // See https://github.com/not-fl3/miniquad/issues/517
        std::cmp::min(1, actions)
    }

    /*
    fn key_up(&mut self, key: &PressedKey) {
        //trace!(target: "PressedKeysSmoothRepeat", "key_up({:?})", key);
        assert!(self.pressed_keys.contains_key(key));
        self.pressed_keys.remove(key).expect("key was pressed");
    }
    */
}

struct RepeatingKeyTimer {
    start: Instant,
    actions: u32,
}

impl RepeatingKeyTimer {
    fn new() -> Self {
        Self { start: Instant::now(), actions: 0 }
    }

    fn update(&mut self, start_delay: u32, step_time: u32) -> u32 {
        let elapsed = self.start.elapsed().as_millis();
        trace!(target: "RepeatingKeyTimer", "update() elapsed={}, actions={}",
               elapsed, self.actions);
        if elapsed < start_delay as u128 {
            return 0
        }
        let total_actions = ((elapsed - start_delay as u128) / step_time as u128) as u32;
        let remaining_actions = total_actions - self.actions;
        self.actions = total_actions;
        remaining_actions
    }
}
