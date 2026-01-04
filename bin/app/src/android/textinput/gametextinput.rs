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

use miniquad::native::android::{self, ndk_sys, ndk_utils::*};
use parking_lot::{Mutex as SyncMutex, RwLock};
use std::{
    ffi::CString,
    sync::{Arc, OnceLock},
};

use super::{AndroidTextInputState, SharedStatePtr};

macro_rules! t { ($($arg:tt)*) => { trace!(target: "android::textinput::gametextinput", $($arg)*); } }
macro_rules! w { ($($arg:tt)*) => { warn!(target: "android::textinput::gametextinput", $($arg)*); } }

pub const SPAN_UNDEFINED: i32 = -1;

/// Global GameTextInput instance for JNI bridge
///
/// Single global instance since only ONE editor is active at a time.
pub static GAME_TEXT_INPUT: OnceLock<GameTextInput> = OnceLock::new();

struct StateClassInfo {
    text: ndk_sys::jfieldID,
    selection_start: ndk_sys::jfieldID,
    selection_end: ndk_sys::jfieldID,
    composing_region_start: ndk_sys::jfieldID,
    composing_region_end: ndk_sys::jfieldID,
}

pub struct GameTextInput {
    state: SyncMutex<Option<SharedStatePtr>>,
    input_connection: RwLock<Option<ndk_sys::jobject>>,
    input_connection_class: ndk_sys::jclass,
    state_class: ndk_sys::jclass,
    set_soft_keyboard_active_method: ndk_sys::jmethodID,
    restart_input_method: ndk_sys::jmethodID,
    state_constructor: ndk_sys::jmethodID,
    state_class_info: StateClassInfo,
}

impl GameTextInput {
    pub fn new() -> Self {
        unsafe {
            let env = android::attach_jni_env();

            let find_class = (**env).FindClass.unwrap();

            let state_class_name = b"textinput/State\0";
            let input_connection_class_name = b"textinput/InputConnection\0";

            let state_java_class = find_class(env, state_class_name.as_ptr() as _);
            let input_connection_class = find_class(env, input_connection_class_name.as_ptr() as _);

            let input_connection_class =
                new_global_ref!(env, input_connection_class) as ndk_sys::jclass;

            let get_method_id = (**env).GetMethodID.unwrap();

            let set_state_sig = b"(Ltextinput/State;)V\0";
            let _input_connection_set_state_method = get_method_id(
                env,
                input_connection_class,
                b"setState\0".as_ptr() as _,
                set_state_sig.as_ptr() as _,
            );

            let set_soft_keyboard_active_sig = b"(ZI)V\0";
            let set_soft_keyboard_active_method = get_method_id(
                env,
                input_connection_class,
                b"setSoftKeyboardActive\0".as_ptr() as _,
                set_soft_keyboard_active_sig.as_ptr() as _,
            );

            let restart_input_sig = b"()V\0";
            let restart_input_method = get_method_id(
                env,
                input_connection_class,
                b"restartInput\0".as_ptr() as _,
                restart_input_sig.as_ptr() as _,
            );

            let state_class = new_global_ref!(env, state_java_class) as ndk_sys::jclass;

            let get_field_id = (**env).GetFieldID.unwrap();

            let text_field = get_field_id(
                env,
                state_class,
                b"text\0".as_ptr() as _,
                b"Ljava/lang/String;\0".as_ptr() as _,
            );
            let selection_start_field = get_field_id(
                env,
                state_class,
                b"selectionStart\0".as_ptr() as _,
                b"I\0".as_ptr() as _,
            );
            let selection_end_field = get_field_id(
                env,
                state_class,
                b"selectionEnd\0".as_ptr() as _,
                b"I\0".as_ptr() as _,
            );
            let composing_region_start_field = get_field_id(
                env,
                state_class,
                b"composingRegionStart\0".as_ptr() as _,
                b"I\0".as_ptr() as _,
            );
            let composing_region_end_field = get_field_id(
                env,
                state_class,
                b"composingRegionEnd\0".as_ptr() as _,
                b"I\0".as_ptr() as _,
            );

            let constructor_sig = b"(Ljava/lang/String;IIII)V\0";
            let state_constructor = get_method_id(
                env,
                state_class,
                b"<init>\0".as_ptr() as _,
                constructor_sig.as_ptr() as _,
            );

            let state_class_info = StateClassInfo {
                text: text_field,
                selection_start: selection_start_field,
                selection_end: selection_end_field,
                composing_region_start: composing_region_start_field,
                composing_region_end: composing_region_end_field,
            };

            Self {
                state: SyncMutex::new(None),
                input_connection: RwLock::new(None),
                input_connection_class,
                state_class,
                set_soft_keyboard_active_method,
                restart_input_method,
                state_constructor,
                state_class_info,
            }
        }
    }

    pub fn focus(&self, state: SharedStatePtr) {
        // Replace old focus
        let mut active_focus = self.state.lock();
        if let Some(old_focus) = &mut *active_focus {
            // Mark old focused state as no longer active
            old_focus.lock().is_active = false;
        }
        *active_focus = Some(state.clone());
        drop(active_focus);

        let mut new_focus = state.lock();
        // Mark new state as active
        new_focus.is_active = true;
        let new_state = new_focus.state.clone();
        drop(new_focus);

        // Push changes to the Java side
        self.push_update(&new_state);
    }

    pub fn push_update(&self, state: &AndroidTextInputState) {
        let Some(input_connection) = *self.input_connection.read() else {
            w!("push_update() - no input_connection set");
            return
        };
        unsafe {
            let env = android::attach_jni_env();
            let jstate = self.state_to_java(state);
            call_void_method!(env, input_connection, "setState", "(Ltextinput/State;)V", jstate);

            let delete_local_ref = (**env).DeleteLocalRef.unwrap();
            delete_local_ref(env, jstate);
        }
    }

    pub fn set_select(&self, start: i32, end: i32) -> Result<(), ()> {
        let Some(input_connection) = *self.input_connection.read() else {
            w!("push_update() - no input_connection set");
            return Err(())
        };
        let is_success = unsafe {
            let env = android::attach_jni_env();
            call_bool_method!(env, input_connection, "setSelection", "(II)Z", start, end)
        };
        if is_success == 0u8 {
            return Err(())
        }
        Ok(())
    }

    pub fn set_input_connection(&self, input_connection: ndk_sys::jobject) {
        unsafe {
            let env = android::attach_jni_env();
            let mut ic = self.input_connection.write();
            if let Some(old_ref) = *ic {
                let delete_global_ref = (**env).DeleteGlobalRef.unwrap();
                delete_global_ref(env, old_ref);
            }
            *ic = Some(new_global_ref!(env, input_connection));
        }
    }

    pub fn process_event(&self, event_state: ndk_sys::jobject) {
        let state = self.state_from_java(event_state);
        t!("IME event: {state:?}");

        let Some(shared) = &*self.state.lock() else {
            w!("process_event() - no shared state set");
            return
        };

        let mut inner = shared.lock();
        inner.state = state.clone();
        let _ = inner.sender.try_send(state);
    }

    pub fn show_ime(&self, flags: u32) {
        let Some(input_connection) = *self.input_connection.read() else {
            w!("show_ime() - no input_connection set");
            return
        };
        unsafe {
            let env = android::attach_jni_env();
            let call_void_method = (**env).CallVoidMethod.unwrap();
            call_void_method(
                env,
                input_connection,
                self.set_soft_keyboard_active_method,
                1, // active: true
                flags as ndk_sys::jint,
            );
        }
    }

    pub fn hide_ime(&self, flags: u32) {
        let Some(input_connection) = *self.input_connection.read() else {
            w!("hide_ime() - no input_connection set");
            return
        };
        unsafe {
            let env = android::attach_jni_env();
            let call_void_method = (**env).CallVoidMethod.unwrap();
            call_void_method(
                env,
                input_connection,
                self.set_soft_keyboard_active_method,
                0, // active: false
                flags as ndk_sys::jint,
            );
        }
    }

    pub fn restart_input(&self) {
        let Some(input_connection) = *self.input_connection.read() else {
            w!("restart_input() - no input_connection set");
            return
        };
        unsafe {
            let env = android::attach_jni_env();
            let call_void_method = (**env).CallVoidMethod.unwrap();
            call_void_method(env, input_connection, self.restart_input_method);
        }
    }

    fn state_to_java(&self, state: &AndroidTextInputState) -> ndk_sys::jobject {
        unsafe {
            let env = android::attach_jni_env();
            let new_string_utf = (**env).NewStringUTF.unwrap();
            let text_str = CString::new(state.text.as_str()).unwrap();
            let jtext = new_string_utf(env, text_str.as_ptr());

            let new_object = (**env).NewObject.unwrap();

            let (compose_start, compose_end) = match state.compose {
                Some((start, end)) => (start as i32, end as i32),
                None => (SPAN_UNDEFINED, SPAN_UNDEFINED),
            };

            let jobj = new_object(
                env,
                self.state_class,
                self.state_constructor,
                jtext,
                state.select.0 as i32,
                state.select.1 as i32,
                compose_start,
                compose_end,
            );

            let delete_local_ref = (**env).DeleteLocalRef.unwrap();
            delete_local_ref(env, jtext);
            jobj
        }
    }

    fn state_from_java(&self, event_state: ndk_sys::jobject) -> AndroidTextInputState {
        unsafe {
            let env = android::attach_jni_env();
            let get_object_field = (**env).GetObjectField.unwrap();
            let jtext =
                get_object_field(env, event_state, self.state_class_info.text) as ndk_sys::jstring;

            let text = get_utf_str!(env, jtext);

            let get_int_field = (**env).GetIntField.unwrap();
            let select_start =
                get_int_field(env, event_state, self.state_class_info.selection_start);
            let select_end = get_int_field(env, event_state, self.state_class_info.selection_end);
            let compose_start =
                get_int_field(env, event_state, self.state_class_info.composing_region_start);
            let compose_end =
                get_int_field(env, event_state, self.state_class_info.composing_region_end);

            let delete_local_ref = (**env).DeleteLocalRef.unwrap();
            delete_local_ref(env, jtext);

            let compose = if compose_start >= 0 {
                Some((compose_start as usize, compose_end as usize))
            } else {
                assert!(compose_end < 0);
                None
            };

            AndroidTextInputState {
                text,
                select: (select_start as usize, select_end as usize),
                compose,
            }
        }
    }
}

impl Drop for GameTextInput {
    fn drop(&mut self) {
        unsafe {
            let env = android::attach_jni_env();
            let delete_global_ref = (**env).DeleteGlobalRef.unwrap();
            if self.input_connection_class != std::ptr::null_mut() {
                delete_global_ref(env, self.input_connection_class);
            }
            if self.state_class != std::ptr::null_mut() {
                delete_global_ref(env, self.state_class);
            }
            if let Some(input_connection) = *self.input_connection.read() {
                delete_global_ref(env, input_connection);
            }
        }
    }
}

unsafe impl Send for GameTextInput {}
unsafe impl Sync for GameTextInput {}
