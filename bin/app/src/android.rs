/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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

use miniquad::native::android::{self, ndk_sys, ndk_utils};
use std::{
    path::PathBuf,
    sync::{LazyLock, Mutex as SyncMutex},
};

struct GlobalData {
    sender: Option<async_channel::Sender<AndroidSuggestEvent>>,
}

unsafe impl Send for GlobalData {}
unsafe impl Sync for GlobalData {}

static GLOBALS: LazyLock<SyncMutex<GlobalData>> =
    LazyLock::new(|| SyncMutex::new(GlobalData { sender: None }));

pub enum AndroidSuggestEvent {
    Compose { text: String, cursor_pos: i32, is_commit: bool },
    ComposeRegion { start: usize, end: usize },
}

#[no_mangle]
pub unsafe extern "C" fn Java_autosuggest_CustomInputConnection_onCompose(
    env: *mut ndk_sys::JNIEnv,
    _: ndk_sys::jobject,
    text: ndk_sys::jobject,
    cursor_pos: ndk_sys::jint,
    is_commit: ndk_sys::jboolean,
) {
    let text = ndk_utils::get_utf_str!(env, text);
    if let Some(sender) = &GLOBALS.lock().unwrap().sender {
        let _ = sender.try_send(AndroidSuggestEvent::Compose {
            text: text.to_string(),
            cursor_pos,
            is_commit: is_commit != 0,
        });
    }
}

#[no_mangle]
pub unsafe extern "C" fn Java_autosuggest_CustomInputConnection_onSetComposeRegion(
    env: *mut ndk_sys::JNIEnv,
    _: ndk_sys::jobject,
    start: ndk_sys::jint,
    end: ndk_sys::jint,
) {
    let begin = std::cmp::min(start, end);
    let end = std::cmp::max(start, end);

    if begin < 0 || end < 0 {
        warn!(target: "android", "setComposeRegion({start}, {end}) is < 0 so skipping");
        return
    }

    let start = begin as usize;
    let end = end as usize;

    if let Some(sender) = &GLOBALS.lock().unwrap().sender {
        let _ = sender.try_send(AndroidSuggestEvent::ComposeRegion { start, end });
    }
}

pub fn set_sender(sender: async_channel::Sender<AndroidSuggestEvent>) {
    GLOBALS.lock().unwrap().sender = Some(sender);
}

pub fn cancel_composition() {
    unsafe {
        let env = android::attach_jni_env();

        ndk_utils::call_void_method!(env, android::ACTIVITY, "cancelComposition", "()V");
    }
}

pub fn get_keyboard_height() -> usize {
    unsafe {
        let env = android::attach_jni_env();

        ndk_utils::call_int_method!(env, android::ACTIVITY, "getKeyboardHeight", "()I") as usize
    }
}

pub fn get_appdata_path() -> PathBuf {
    let path = unsafe {
        let env = android::attach_jni_env();

        let text = ndk_utils::call_object_method!(
            env,
            android::ACTIVITY,
            "getAppDataPath",
            "()Ljava/lang/String;"
        );
        ndk_utils::get_utf_str!(env, text).to_string()
    };
    path.into()
}
pub fn get_external_storage_path() -> PathBuf {
    let path = unsafe {
        let env = android::attach_jni_env();

        let text = ndk_utils::call_object_method!(
            env,
            android::ACTIVITY,
            "getExternalStoragePath",
            "()Ljava/lang/String;"
        );
        ndk_utils::get_utf_str!(env, text).to_string()
    };
    path.into()
}
