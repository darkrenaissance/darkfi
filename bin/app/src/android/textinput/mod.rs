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

use miniquad::native::android::{self, ndk_sys};
use parking_lot::Mutex as SyncMutex;
use std::{
    collections::HashMap,
    ffi::{c_char, c_void, CString},
    sync::LazyLock,
};

mod ffi;
use ffi::{
    GameTextInput, GameTextInputSpan, GameTextInputState, GameTextInput_destroy,
    GameTextInput_getState, GameTextInput_hideIme, GameTextInput_init,
    GameTextInput_setEventCallback, GameTextInput_setState, GameTextInput_showIme,
};

// Text input state exposed to the rest of the app
#[derive(Debug, Clone)]
pub struct AndroidTextInputState {
    pub text: String,
    pub select: (usize, usize),
    pub compose: Option<(usize, usize)>,
}

struct Globals {
    next_id: usize,
    senders: HashMap<usize, async_channel::Sender<AndroidTextInputState>>,
}

static GLOBALS: LazyLock<SyncMutex<Globals>> =
    LazyLock::new(|| SyncMutex::new(Globals { next_id: 0, senders: HashMap::new() }));

// Callback implementation - sends state update event
extern "C" fn game_text_input_callback(ctx: *mut c_void, state: *const GameTextInputState) {
    // Ensures we can use the void* pointer to store a usize
    assert_eq!(std::mem::size_of::<usize>(), std::mem::size_of::<*mut c_void>());
    // ctx is the usize id we passed as void* pointer
    let id = ctx as usize;
    let text_state = unsafe { &(*state) }.to_owned();

    let globals = GLOBALS.lock();
    if let Some(sender) = globals.senders.get(&id) {
        let _ = sender.try_send(text_state);
    }
}

pub struct AndroidTextInput {
    id: usize,
    state: *mut GameTextInput,
}

impl AndroidTextInput {
    pub fn new(sender: async_channel::Sender<AndroidTextInputState>) -> Self {
        let id = {
            let mut globals = GLOBALS.lock();
            let id = globals.next_id;
            globals.next_id += 1;
            globals.senders.insert(id, sender);
            id
        };

        let state = unsafe {
            let env = android::attach_jni_env();
            let state = GameTextInput_init(env, 0);
            // Ensures we can use the void* pointer to store a usize
            assert_eq!(std::mem::size_of::<usize>(), std::mem::size_of::<*mut c_void>());
            GameTextInput_setEventCallback(
                state,
                Some(game_text_input_callback),
                id as *mut c_void,
            );
            state
        };

        Self { id, state }
    }

    pub fn show_ime(&self) {
        unsafe {
            GameTextInput_showIme(self.state, 0);
        }
    }

    pub fn hide_ime(&self) {
        unsafe {
            GameTextInput_hideIme(self.state, 0);
        }
    }

    pub fn set_state(&self, state: &AndroidTextInputState) {
        let ctext = CString::new(state.text.as_str()).unwrap();

        let select = GameTextInputSpan { start: state.select.0 as i32, end: state.select.1 as i32 };

        let compose = match state.compose {
            Some((start, end)) => GameTextInputSpan { start: start as i32, end: end as i32 },
            None => GameTextInputSpan { start: -1, end: -1 },
        };

        let gt_state = GameTextInputState {
            text_utf8: ctext.as_ptr(),
            text_length: state.text.len() as i32,
            select,
            compose,
        };

        unsafe {
            GameTextInput_setState(self.state, &gt_state);
        }
    }

    pub fn get_state(&self) -> AndroidTextInputState {
        let mut state =
            AndroidTextInputState { text: String::new(), select: (0, 0), compose: None };

        // This is guaranteed by GameTextInput_getState() to be called sync
        // so what we are doing is legit here.
        extern "C" fn callback(ctx: *mut c_void, game_state: *const GameTextInputState) {
            let state = unsafe { &mut *(ctx as *mut AndroidTextInputState) };
            *state = unsafe { &(*game_state) }.to_owned();
        }

        unsafe {
            GameTextInput_getState(
                self.state,
                callback,
                &mut state as *mut AndroidTextInputState as *mut c_void,
            );
        }

        state
    }
}

impl Drop for AndroidTextInput {
    fn drop(&mut self) {
        unsafe {
            GameTextInput_destroy(self.state);
        }
        GLOBALS.lock().senders.remove(&self.id);
    }
}
