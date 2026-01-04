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

use async_channel::Sender as AsyncSender;
use parking_lot::Mutex as SyncMutex;
use std::sync::Arc;

mod gametextinput;
mod jni;

use gametextinput::{GameTextInput, GAME_TEXT_INPUT};

macro_rules! t { ($($arg:tt)*) => { trace!(target: "android::textinput", $($arg)*); } }

// Text input state exposed to the rest of the app
#[derive(Debug, Clone, Default)]
pub struct AndroidTextInputState {
    pub text: String,
    pub select: (usize, usize),
    pub compose: Option<(usize, usize)>,
}

struct SharedState {
    state: AndroidTextInputState,
    /// Used so we know whether to also update the GameTextInput in Android.
    /// We should only do so when active.
    is_active: bool,
    sender: AsyncSender<AndroidTextInputState>,
}

impl SharedState {
    fn new(sender: AsyncSender<AndroidTextInputState>) -> Self {
        Self { state: Default::default(), is_active: false, sender }
    }
}

pub(self) type SharedStatePtr = Arc<SyncMutex<SharedState>>;

pub struct AndroidTextInput {
    state: SharedStatePtr,
}

impl AndroidTextInput {
    pub fn new(sender: AsyncSender<AndroidTextInputState>) -> Self {
        Self { state: Arc::new(SyncMutex::new(SharedState::new(sender))) }
    }

    pub fn show(&self) {
        t!("show IME");
        let gti = GAME_TEXT_INPUT.get().unwrap();
        gti.focus(self.state.clone());
        gti.show_ime(0);
    }

    pub fn hide(&self) {
        t!("hide IME");
        let gti = GAME_TEXT_INPUT.get().unwrap();
        gti.hide_ime(0);
    }

    pub fn set_state(&self, state: AndroidTextInputState) {
        t!("set_state({state:?})");
        // Always update our own state.
        let mut ours = self.state.lock();
        ours.state = state.clone();
        let is_active = ours.is_active;
        drop(ours);

        // Only update java state when this input is active
        if is_active {
            let gti = GAME_TEXT_INPUT.get().unwrap();
            gti.push_update(&state);
        }
    }

    pub fn set_select(&self, select_start: usize, select_end: usize) {
        //t!("set_select({select_start}, {select_end})");
        // Always update our own state.
        let mut ours = self.state.lock();
        let state = &mut ours.state;
        assert!(select_start <= state.text.len());
        assert!(select_end <= state.text.len());
        state.select = (select_start, select_end);
        let is_active = ours.is_active;
        drop(ours);

        // Only update java state when this input is active
        if is_active {
            let gti = GAME_TEXT_INPUT.get().unwrap();
            gti.set_select(select_start as i32, select_end as i32).unwrap();
        }
    }
}
