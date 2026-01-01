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

use miniquad::native::android::ndk_sys;
use std::ffi::{c_char, c_void, CStr};

use super::AndroidTextInputState;

// Opaque type from GameTextInput C API
#[repr(C)]
pub struct GameTextInput(c_void);

// Span type used by GameTextInput
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct GameTextInputSpan {
    pub start: i32,
    pub end: i32,
}

// State structure matching the C header (gametextinput.h:67-85)
#[repr(C)]
pub struct GameTextInputState {
    pub text_utf8: *const c_char,
    pub text_length: i32,
    pub select: GameTextInputSpan,
    pub compose: GameTextInputSpan,
}

impl GameTextInputState {
    pub fn to_owned(&self) -> AndroidTextInputState {
        let text = unsafe { CStr::from_ptr(self.text_utf8) }.to_str().unwrap().to_string();

        let select = (self.select.start as usize, self.select.end as usize);

        let compose = if self.compose.start >= 0 {
            assert!(self.compose.end >= 0);
            Some((self.compose.start as usize, self.compose.end as usize))
        } else {
            assert!(self.compose.end < 0);
            None
        };

        AndroidTextInputState { text, select, compose }
    }
}

// Callback type definitions (gametextinput.h:93-94, 221-222)
pub type GameTextInputGetStateCallback =
    unsafe extern "C" fn(*mut c_void, *const GameTextInputState);

pub type GameTextInputEventCallback = unsafe extern "C" fn(*mut c_void, *const GameTextInputState);

// FFI bindings to GameTextInput C library
extern "C" {
    // gametextinput.h:111
    pub fn GameTextInput_init(
        env: *mut ndk_sys::JNIEnv,
        max_string_size: u32,
    ) -> *mut GameTextInput;

    // gametextinput.h:140
    pub fn GameTextInput_destroy(state: *mut GameTextInput);

    // gametextinput.h:235-237
    pub fn GameTextInput_setEventCallback(
        state: *mut GameTextInput,
        callback: Option<GameTextInputEventCallback>,
        context: *mut c_void,
    );

    // gametextinput.h:161
    pub fn GameTextInput_showIme(state: *mut GameTextInput, flags: u32);

    // gametextinput.h:182
    pub fn GameTextInput_hideIme(state: *mut GameTextInput, flags: u32);

    // gametextinput.h:211-212
    pub fn GameTextInput_setState(state: *mut GameTextInput, state: *const GameTextInputState);

    // gametextinput.h:200-202
    pub fn GameTextInput_getState(
        state: *const GameTextInput,
        callback: GameTextInputGetStateCallback,
        context: *mut c_void,
    );

    // gametextinput.h:121-122
    pub fn GameTextInput_setInputConnection(
        state: *mut GameTextInput,
        input_connection: *mut c_void,
    );

    // gametextinput.h:132
    pub fn GameTextInput_processEvent(state: *mut GameTextInput, event_state: *mut c_void);
}
