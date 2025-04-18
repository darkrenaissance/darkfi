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

use miniquad::native::android::{self, ndk_sys, ndk_utils};
use std::{
    path::PathBuf,
    sync::{LazyLock, Mutex as SyncMutex},
};

use crate::AndroidSuggestEvent;

macro_rules! call_mainactivity_int_method {
    ($method:expr, $sig:expr $(, $args:expr)*) => {{
        unsafe {
            let env = android::attach_jni_env();
            ndk_utils::call_int_method!(env, android::ACTIVITY, $method, $sig $(, $args)*)
        }
    }};
}
macro_rules! call_mainactivity_str_method {
    ($method:expr) => {{
        unsafe {
            let env = android::attach_jni_env();
            let text = ndk_utils::call_object_method!(
                env,
                android::ACTIVITY,
                $method,
                "()Ljava/lang/String;"
            );
            ndk_utils::get_utf_str!(env, text)
        }
    }};
}

struct GlobalData {
    sender: Option<async_channel::Sender<AndroidSuggestEvent>>,
    next_id: usize,
}

unsafe impl Send for GlobalData {}
unsafe impl Sync for GlobalData {}

static GLOBALS: LazyLock<SyncMutex<GlobalData>> =
    LazyLock::new(|| SyncMutex::new(GlobalData { sender: None, next_id: 0 }));

#[no_mangle]
pub unsafe extern "C" fn Java_darkfi_darkfi_1app_MainActivity_onInitEdit(
    env: *mut ndk_sys::JNIEnv,
    _: ndk_sys::jobject,
    id: ndk_sys::jint,
) {
    trace!(target: "android", "onInit() CALLED");
    assert!(id >= 0);
    let id = id as usize;
    if let Some(sender) = &GLOBALS.lock().unwrap().sender {
        trace!(target: "android", "onInit()");
        let _ = sender.try_send(AndroidSuggestEvent::Init);
    }
}

#[no_mangle]
pub unsafe extern "C" fn Java_autosuggest_InvisibleInputView_onCreateInputConnect(
    env: *mut ndk_sys::JNIEnv,
    _: ndk_sys::jobject,
    id: ndk_sys::jint,
) {
    assert!(id >= 0);
    let id = id as usize;
    if let Some(sender) = &GLOBALS.lock().unwrap().sender {
        let _ = sender.try_send(AndroidSuggestEvent::CreateInputConnect);
    }
}

#[no_mangle]
pub unsafe extern "C" fn Java_autosuggest_CustomInputConnection_onCompose(
    env: *mut ndk_sys::JNIEnv,
    _: ndk_sys::jobject,
    id: ndk_sys::jint,
    text: ndk_sys::jobject,
    cursor_pos: ndk_sys::jint,
    is_commit: ndk_sys::jboolean,
) {
    assert!(id >= 0);
    let id = id as usize;
    let text = ndk_utils::get_utf_str!(env, text);
    if let Some(sender) = &GLOBALS.lock().unwrap().sender {
        let _ = sender.try_send(AndroidSuggestEvent::Compose {
            text: text.to_string(),
            cursor_pos,
            is_commit: is_commit == 1,
        });
    }
}
#[no_mangle]
pub unsafe extern "C" fn Java_autosuggest_CustomInputConnection_onSetComposeRegion(
    env: *mut ndk_sys::JNIEnv,
    _: ndk_sys::jobject,
    id: ndk_sys::jint,
    start: ndk_sys::jint,
    end: ndk_sys::jint,
) {
    assert!(id >= 0);
    let id = id as usize;
    if let Some(sender) = &GLOBALS.lock().unwrap().sender {
        let _ = sender.try_send(AndroidSuggestEvent::ComposeRegion {
            start: start as usize,
            end: end as usize,
        });
    }
}
#[no_mangle]
pub unsafe extern "C" fn Java_autosuggest_CustomInputConnection_onFinishCompose(
    env: *mut ndk_sys::JNIEnv,
    _: ndk_sys::jobject,
    id: ndk_sys::jint,
) {
    assert!(id >= 0);
    let id = id as usize;
    if let Some(sender) = &GLOBALS.lock().unwrap().sender {
        let _ = sender.try_send(AndroidSuggestEvent::FinishCompose);
    }
}
#[no_mangle]
pub unsafe extern "C" fn Java_autosuggest_CustomInputConnection_onDeleteSurroundingText(
    env: *mut ndk_sys::JNIEnv,
    _: ndk_sys::jobject,
    id: ndk_sys::jint,
    left: ndk_sys::jint,
    right: ndk_sys::jint,
) {
    assert!(id >= 0);
    let id = id as usize;
    if let Some(sender) = &GLOBALS.lock().unwrap().sender {
        let _ = sender.try_send(AndroidSuggestEvent::DeleteSurroundingText {
            left: left as usize,
            right: right as usize,
        });
    }
}

pub fn create_composer(sender: async_channel::Sender<AndroidSuggestEvent>) -> usize {
    let composer_id = {
        let mut globals = GLOBALS.lock().unwrap();
        let id = globals.next_id;
        globals.next_id += 1;
        globals.sender = Some(sender);
        id
    };
    unsafe {
        let env = android::attach_jni_env();
        ndk_utils::call_void_method!(env, android::ACTIVITY, "createComposer", "(I)V", composer_id);
    }
    composer_id
}

pub fn focus(id: usize) -> Option<()> {
    let is_success = unsafe {
        let env = android::attach_jni_env();

        ndk_utils::call_bool_method!(env, android::ACTIVITY, "focus", "(I)Z", id as i32)
    };
    if is_success == 0u8 {
        None
    } else {
        Some(())
    }
}

pub fn set_text(id: usize, text: &str) -> Option<()> {
    let ctext = std::ffi::CString::new(text).unwrap();
    let is_success = unsafe {
        let env = android::attach_jni_env();

        let new_string_utf = (**env).NewStringUTF.unwrap();
        let jtext = new_string_utf(env, ctext.as_ptr());

        ndk_utils::call_bool_method!(
            env,
            android::ACTIVITY,
            "setText",
            "(ILjava/lang/String;)Z",
            id as i32,
            jtext
        )
    };
    if is_success == 0u8 {
        None
    } else {
        Some(())
    }
}

pub fn set_selection(id: usize, select_start: usize, select_end: usize) -> Option<()> {
    //trace!(target: "android", "set_selection({id}, {select_start}, {select_end})");
    unsafe {
        let env = android::attach_jni_env();
        let input_connect = ndk_utils::call_object_method!(
            env,
            android::ACTIVITY,
            "getInputConnect",
            "(I)Lautosuggest/CustomInputConnection;",
            id as i32
        );
        if input_connect.is_null() {
            return None
        }

        ndk_utils::call_bool_method!(env, input_connect, "beginBatchEdit", "()Z");
        ndk_utils::call_bool_method!(
            env,
            input_connect,
            "setSelection",
            "(II)Z",
            select_start,
            select_end
        );
        ndk_utils::call_bool_method!(env, input_connect, "endBatchEdit", "()Z");
    }
    Some(())
}

pub struct Editable {
    pub buffer: String,
    pub select_start: usize,
    pub select_end: usize,
    pub compose_start: Option<usize>,
    pub compose_end: Option<usize>,
}

pub fn get_editable(id: usize) -> Option<Editable> {
    //trace!(target: "android", "get_editable({id})");
    unsafe {
        let env = android::attach_jni_env();
        let input_connect = ndk_utils::call_object_method!(
            env,
            android::ACTIVITY,
            "getInputConnect",
            "(I)Lautosuggest/CustomInputConnection;",
            id as i32
        );
        if input_connect.is_null() {
            return None
        }

        let buffer =
            ndk_utils::call_object_method!(env, input_connect, "rawText", "()Ljava/lang/String;");
        let buffer = ndk_utils::get_utf_str!(env, buffer).to_string();

        let select_start =
            ndk_utils::call_int_method!(env, input_connect, "getSelectionStart", "()I");

        let select_end = ndk_utils::call_int_method!(env, input_connect, "getSelectionEnd", "()I");

        let compose_start =
            ndk_utils::call_int_method!(env, input_connect, "getComposeStart", "()I");

        let compose_end = ndk_utils::call_int_method!(env, input_connect, "getComposeEnd", "()I");

        assert!(select_start >= 0);
        assert!(select_end >= 0);
        assert!(compose_start >= 0 || compose_start == compose_end);
        assert!(compose_start <= compose_end);

        Some(Editable {
            buffer,
            select_start: select_start as usize,
            select_end: select_end as usize,
            compose_start: if compose_start < 0 { None } else { Some(compose_start as usize) },
            compose_end: if compose_end < 0 { None } else { Some(compose_end as usize) },
        })
    }
}

pub fn get_keyboard_height() -> usize {
    call_mainactivity_int_method!("getKeyboardHeight", "()I") as usize
}

pub fn get_appdata_path() -> PathBuf {
    call_mainactivity_str_method!("getAppDataPath").into()
}
pub fn get_external_storage_path() -> PathBuf {
    call_mainactivity_str_method!("getExternalStoragePath").into()
}
