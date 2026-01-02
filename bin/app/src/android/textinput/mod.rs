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

use async_channel::Sender as AsyncSender;
use miniquad::native::android::attach_jni_env;
use parking_lot::RwLock;
use std::sync::LazyLock;

mod jni;
mod state;

use state::GameTextInput;

/// Global GameTextInput instance for JNI bridge
///
/// Single global instance since only ONE editor is active at a time.
pub(self) static GAME_TEXT_INPUT: LazyLock<RwLock<Option<GameTextInput>>> =
    LazyLock::new(|| RwLock::new(None));

pub(self) fn init_game_text_input() {
    debug!("AndroidTextInput: Initializing GameTextInput");

    let env = unsafe { attach_jni_env() };
    let mut gti = GameTextInput::new(env, 0);
    // Store globally for JNI bridge access
    *GAME_TEXT_INPUT.write() = Some(gti);

    debug!("AndroidTextInput: GameTextInput initialized");
}

fn is_init() -> bool {
    GAME_TEXT_INPUT.read().is_some()
}

// Text input state exposed to the rest of the app
#[derive(Debug, Clone)]
pub struct AndroidTextInputState {
    pub text: String,
    pub select: (usize, usize),
    pub compose: Option<(usize, usize)>,
}

impl AndroidTextInputState {
    fn new() -> Self {
        Self { text: String::new(), select: (0, 0), compose: None }
    }
}

pub struct AndroidTextInput {
    state: AndroidTextInputState,
    sender: async_channel::Sender<AndroidTextInputState>,
    is_focus: bool,
}

impl AndroidTextInput {
    pub fn new(sender: async_channel::Sender<AndroidTextInputState>) -> Self {
        Self { state: AndroidTextInputState::new(), sender, is_focus: false }
    }

    pub fn show(&mut self) {
        if !is_init() {
            return;
        }
        if let Some(gti) = &mut *GAME_TEXT_INPUT.write() {
            gti.event_sender = Some(self.sender.clone());
            gti.set_state(&self.state);
            gti.show_ime(0);
        }
        self.is_focus = true;
    }

    pub fn hide(&mut self) {
        if !is_init() {
            return;
        }
        if let Some(gti) = &mut *GAME_TEXT_INPUT.write() {
            gti.event_sender = None;
            gti.hide_ime(0);
        }
        self.is_focus = false;
    }

    pub fn set_state(&mut self, state: AndroidTextInputState) {
        self.state = state;
        if let Some(gti) = &mut *GAME_TEXT_INPUT.write() {
            gti.set_state(&self.state);
        }
    }

    pub fn get_state(&self) -> &AndroidTextInputState {
        &self.state
    }
}
