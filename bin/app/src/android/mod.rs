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

use miniquad::native::android::{self, ndk_sys, ndk_utils};
use parking_lot::Mutex as SyncMutex;
use std::{collections::HashMap, path::PathBuf, sync::LazyLock};

use crate::AndroidSuggestEvent;

pub mod insets;
pub mod vid;

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
macro_rules! call_mainactivity_float_method {
    ($method:expr) => {{
        unsafe {
            let env = android::attach_jni_env();
            ndk_utils::call_method!(CallFloatMethod, env, android::ACTIVITY, $method, "()F")
        }
    }};
}
macro_rules! call_mainactivity_bool_method {
    ($method:expr) => {{
        unsafe {
            let env = android::attach_jni_env();
            ndk_utils::call_method!(CallBooleanMethod, env, android::ACTIVITY, $method, "()Z") !=
                0u8
        }
    }};
}

struct GlobalData {
    senders: HashMap<usize, async_channel::Sender<AndroidSuggestEvent>>,
    next_id: usize,
}

fn send(id: usize, ev: AndroidSuggestEvent) {
    let globals = &GLOBALS.lock();
    let Some(sender) = globals.senders.get(&id) else {
        warn!(target: "android", "Unknown composer_id={id} discard ev: {ev:?}");
        return
    };
    let _ = sender.try_send(ev);
}

unsafe impl Send for GlobalData {}
unsafe impl Sync for GlobalData {}

static GLOBALS: LazyLock<SyncMutex<GlobalData>> =
    LazyLock::new(|| SyncMutex::new(GlobalData { senders: HashMap::new(), next_id: 0 }));

#[no_mangle]
pub unsafe extern "C" fn Java_darkfi_darkfi_1app_MainActivity_onInitEdit(
    _env: *mut ndk_sys::JNIEnv,
    _: ndk_sys::jobject,
    id: ndk_sys::jint,
) {
    assert!(id >= 0);
    let id = id as usize;
    send(id, AndroidSuggestEvent::Init);
}

#[no_mangle]
pub unsafe extern "C" fn Java_autosuggest_InvisibleInputView_onCreateInputConnect(
    _env: *mut ndk_sys::JNIEnv,
    _: ndk_sys::jobject,
    id: ndk_sys::jint,
) {
    assert!(id >= 0);
    let id = id as usize;
    send(id, AndroidSuggestEvent::CreateInputConnect);
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
    send(
        id,
        AndroidSuggestEvent::Compose {
            text: text.to_string(),
            cursor_pos,
            is_commit: is_commit == 1,
        },
    );
}
#[no_mangle]
pub unsafe extern "C" fn Java_autosuggest_CustomInputConnection_onSetComposeRegion(
    _env: *mut ndk_sys::JNIEnv,
    _: ndk_sys::jobject,
    id: ndk_sys::jint,
    start: ndk_sys::jint,
    end: ndk_sys::jint,
) {
    assert!(id >= 0);
    let id = id as usize;
    send(id, AndroidSuggestEvent::ComposeRegion { start: start as usize, end: end as usize });
}
#[no_mangle]
pub unsafe extern "C" fn Java_autosuggest_CustomInputConnection_onFinishCompose(
    _env: *mut ndk_sys::JNIEnv,
    _: ndk_sys::jobject,
    id: ndk_sys::jint,
) {
    assert!(id >= 0);
    let id = id as usize;
    send(id, AndroidSuggestEvent::FinishCompose);
}
#[no_mangle]
pub unsafe extern "C" fn Java_autosuggest_CustomInputConnection_onDeleteSurroundingText(
    _env: *mut ndk_sys::JNIEnv,
    _: ndk_sys::jobject,
    id: ndk_sys::jint,
    left: ndk_sys::jint,
    right: ndk_sys::jint,
) {
    assert!(id >= 0);
    let id = id as usize;
    send(
        id,
        AndroidSuggestEvent::DeleteSurroundingText { left: left as usize, right: right as usize },
    );
}

pub fn create_composer(sender: async_channel::Sender<AndroidSuggestEvent>) -> usize {
    let composer_id = {
        let mut globals = GLOBALS.lock();
        let id = globals.next_id;
        globals.next_id += 1;
        globals.senders.insert(id, sender);
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
pub fn unfocus(id: usize) -> Option<()> {
    let is_success = unsafe {
        let env = android::attach_jni_env();

        ndk_utils::call_bool_method!(env, android::ACTIVITY, "unfocus", "(I)Z", id as i32)
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
        let delete_local_ref = (**env).DeleteLocalRef.unwrap();

        let res = ndk_utils::call_bool_method!(
            env,
            android::ACTIVITY,
            "setText",
            "(ILjava/lang/String;)Z",
            id as i32,
            jtext
        );
        delete_local_ref(env, jtext);
        res
    };
    if is_success == 0u8 {
        None
    } else {
        Some(())
    }
}

pub fn set_selection(id: usize, select_start: usize, select_end: usize) -> Option<()> {
    //trace!(target: "android", "set_selection({id}, {select_start}, {select_end})");
    let is_success = unsafe {
        let env = android::attach_jni_env();
        ndk_utils::call_bool_method!(
            env,
            android::ACTIVITY,
            "setSelection",
            "(III)Z",
            id as i32,
            select_start as i32,
            select_end as i32
        )
    };
    if is_success == 0u8 {
        None
    } else {
        Some(())
    }
}

pub fn commit_text(id: usize, text: &str) -> Option<()> {
    let ctext = std::ffi::CString::new(text).unwrap();
    let is_success = unsafe {
        let env = android::attach_jni_env();

        let new_string_utf = (**env).NewStringUTF.unwrap();
        let delete_local_ref = (**env).DeleteLocalRef.unwrap();

        let jtext = new_string_utf(env, ctext.as_ptr());

        let res = ndk_utils::call_bool_method!(
            env,
            android::ACTIVITY,
            "commitText",
            "(ILjava/lang/String;)Z",
            id as i32,
            jtext
        );
        delete_local_ref(env, jtext);
        res
    };
    if is_success == 0u8 {
        None
    } else {
        Some(())
    }
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
        let input_view = ndk_utils::call_object_method!(
            env,
            android::ACTIVITY,
            "getInputView",
            "(I)Lautosuggest/InvisibleInputView;",
            id as i32
        );
        if input_view.is_null() {
            return None
        }

        let buffer =
            ndk_utils::call_object_method!(env, input_view, "rawText", "()Ljava/lang/String;");
        assert!(!buffer.is_null());
        let buffer = ndk_utils::get_utf_str!(env, buffer).to_string();

        let select_start = ndk_utils::call_int_method!(env, input_view, "getSelectionStart", "()I");

        let select_end = ndk_utils::call_int_method!(env, input_view, "getSelectionEnd", "()I");

        let compose_start = ndk_utils::call_int_method!(env, input_view, "getComposeStart", "()I");

        let compose_end = ndk_utils::call_int_method!(env, input_view, "getComposeEnd", "()I");

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

pub fn get_appdata_path() -> PathBuf {
    call_mainactivity_str_method!("getAppDataPath").into()
}
pub fn get_external_storage_path() -> PathBuf {
    call_mainactivity_str_method!("getExternalStoragePath").into()
}

pub fn get_keyboard_height() -> usize {
    call_mainactivity_int_method!("getKeyboardHeight", "()I") as usize
}

pub fn get_screen_density() -> f32 {
    call_mainactivity_float_method!("getScreenDensity")
}

pub fn is_ime_visible() -> bool {
    call_mainactivity_bool_method!("isImeVisible")
}
