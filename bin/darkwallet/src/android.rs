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
use std::sync::{LazyLock, Mutex as SyncMutex};

struct GlobalData {
    inp_conn: ndk_sys::jobject,
}

unsafe impl Send for GlobalData {}
unsafe impl Sync for GlobalData {}

static GLOBALS: LazyLock<SyncMutex<GlobalData>> =
    LazyLock::new(|| SyncMutex::new(GlobalData { inp_conn: std::ptr::null_mut() }));

#[no_mangle]
pub unsafe extern "C" fn Java_autosuggest_CustomInputConnection_setup() {
    let env = android::attach_jni_env();

    let inp_conn = ndk_utils::new_object!(env, "autosuggest/CustomInputConnection", "()V");
    assert!(!inp_conn.is_null());

    let inp_conn = ndk_utils::new_global_ref!(env, inp_conn);
    GLOBALS.lock().unwrap().inp_conn = inp_conn;
}

#[no_mangle]
pub unsafe extern "C" fn Java_autosuggest_CustomInputConnection_onCommitText(
    env: *mut ndk_sys::JNIEnv,
    _: ndk_sys::jobject,
    text: ndk_sys::jobject,
) {
    let text = ndk_utils::get_utf_str!(env, text);
    debug!(target: "android", "onCommitText: {text}");
}

#[no_mangle]
pub unsafe extern "C" fn Java_autosuggest_CustomInputConnection_onEndEdit(
    env: *mut ndk_sys::JNIEnv,
    _: ndk_sys::jobject,
    text: ndk_sys::jobject,
) {
    let text = ndk_utils::get_utf_str!(env, text);
    debug!(target: "android", "onEditText: {text}");
}
