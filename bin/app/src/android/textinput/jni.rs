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

use super::gametextinput::{GameTextInput, GAME_TEXT_INPUT};

/// Set the InputConnection for GameTextInput (called from Java)
///
/// This follows the official Android GameTextInput integration pattern:
/// https://developer.android.com/games/agdk/add-support-for-text-input
///
/// Called from MainActivity when the InputConnection is created. It passes
/// the Java InputConnection object to the native GameTextInput library.
#[no_mangle]
pub extern "C" fn Java_darkfi_darkfi_1app_MainActivity_setInputConnectionNative(
    _env: *mut ndk_sys::JNIEnv,
    _class: ndk_sys::jclass,
    input_connection: ndk_sys::jobject,
) {
    debug!(target: "android::textinput::jni", "Setting input connection");
    // Initialize GameTextInput on first call
    let gti = GAME_TEXT_INPUT.get_or_init(|| GameTextInput::new());
    gti.set_input_connection(input_connection);
}

/// Process IME state event from Java Listener.stateChanged()
///
/// This follows the official Android GameTextInput integration pattern.
/// Called from the Java InputConnection's Listener whenever the IME sends
/// a state change (text typed, cursor moved, etc.).
#[no_mangle]
pub extern "C" fn Java_darkfi_darkfi_1app_MainActivity_onTextInputEventNative(
    _env: *mut ndk_sys::JNIEnv,
    _class: ndk_sys::jclass,
    soft_keyboard_event: ndk_sys::jobject,
) {
    let gti = GAME_TEXT_INPUT.get().unwrap();
    gti.process_event(soft_keyboard_event);
}
