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
use miniquad::native::android::{ndk_sys, ndk_utils::*};
use parking_lot::Mutex as SyncMutex;
use std::ffi::CString;

use super::AndroidTextInputState;

const DEFAULT_MAX_STRING_SIZE: usize = 1 << 16;

pub const SPAN_UNDEFINED: i32 = -1;

struct StateClassInfo {
    text: ndk_sys::jfieldID,
    selection_start: ndk_sys::jfieldID,
    selection_end: ndk_sys::jfieldID,
    composing_region_start: ndk_sys::jfieldID,
    composing_region_end: ndk_sys::jfieldID,
}

pub struct GameTextInput {
    env: *mut ndk_sys::JNIEnv,
    state: SyncMutex<AndroidTextInputState>,
    input_connection: Option<ndk_sys::jobject>,
    input_connection_class: ndk_sys::jclass,
    set_soft_keyboard_active_method: ndk_sys::jmethodID,
    restart_input_method: ndk_sys::jmethodID,
    state_class_info: StateClassInfo,
    pub event_sender: Option<AsyncSender<AndroidTextInputState>>
}

impl GameTextInput {
    pub fn new(env: *mut ndk_sys::JNIEnv, max_string_size: u32) -> Self {
        unsafe {
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

            let state_java_class = new_global_ref!(env, state_java_class);

            let get_field_id = (**env).GetFieldID.unwrap();

            let text_field = get_field_id(
                env,
                state_java_class,
                b"text\0".as_ptr() as _,
                b"Ljava/lang/String;\0".as_ptr() as _,
            );
            let selection_start_field = get_field_id(
                env,
                state_java_class,
                b"selectionStart\0".as_ptr() as _,
                b"I\0".as_ptr() as _,
            );
            let selection_end_field = get_field_id(
                env,
                state_java_class,
                b"selectionEnd\0".as_ptr() as _,
                b"I\0".as_ptr() as _,
            );
            let composing_region_start_field = get_field_id(
                env,
                state_java_class,
                b"composingRegionStart\0".as_ptr() as _,
                b"I\0".as_ptr() as _,
            );
            let composing_region_end_field = get_field_id(
                env,
                state_java_class,
                b"composingRegionEnd\0".as_ptr() as _,
                b"I\0".as_ptr() as _,
            );

            let state_class_info = StateClassInfo {
                text: text_field,
                selection_start: selection_start_field,
                selection_end: selection_end_field,
                composing_region_start: composing_region_start_field,
                composing_region_end: composing_region_end_field,
            };

            Self {
                env,
                state: SyncMutex::new(AndroidTextInputState::new()),
                input_connection: None,
                input_connection_class,
                set_soft_keyboard_active_method,
                restart_input_method,
                state_class_info,
                event_sender: None,
            }
        }
    }

    pub fn set_state(&mut self, state: &AndroidTextInputState) {
        if let Some(input_connection) = self.input_connection {
            unsafe {
                let jstate = self.state_to_java(state);
                call_void_method!(
                    self.env,
                    input_connection,
                    "setState",
                    "(Ltextinput/State;)V",
                    jstate
                );
                let delete_local_ref = (**self.env).DeleteLocalRef.unwrap();
                delete_local_ref(self.env, jstate);
            }
        }
        *self.state.lock() = state.clone();
    }

    fn set_state_inner(&mut self, state: AndroidTextInputState) {
        *self.state.lock() = state;
    }

    pub fn get_state(&self) -> AndroidTextInputState {
        self.state.lock().clone()
    }

    pub fn set_input_connection(&mut self, input_connection: ndk_sys::jobject) {
        unsafe {
            if let Some(old_ref) = self.input_connection {
                let delete_global_ref = (**self.env).DeleteGlobalRef.unwrap();
                delete_global_ref(self.env, old_ref);
            }
            self.input_connection = Some(new_global_ref!(self.env, input_connection));
        }
    }

    pub fn process_event(&mut self, event_state: ndk_sys::jobject) {
        let state = self.state_from_java(event_state);
            if let Some(sender) = &self.event_sender {
                let _ = sender.try_send(state.clone());
            }
            self.set_state_inner(state);
    }

    pub fn show_ime(&self, flags: u32) {
        if let Some(input_connection) = self.input_connection {
            unsafe {
                let call_void_method = (**self.env).CallVoidMethod.unwrap();
                call_void_method(
                    self.env,
                    input_connection,
                    self.set_soft_keyboard_active_method,
                    1, // active: true
                    flags as ndk_sys::jint,
                );
            }
        }
    }

    pub fn hide_ime(&self, flags: u32) {
        if let Some(input_connection) = self.input_connection {
            unsafe {
                let call_void_method = (**self.env).CallVoidMethod.unwrap();
                call_void_method(
                    self.env,
                    input_connection,
                    self.set_soft_keyboard_active_method,
                    0, // active: false
                    flags as ndk_sys::jint,
                );
            }
        }
    }

    pub fn restart_input(&self) {
        if let Some(input_connection) = self.input_connection {
            unsafe {
                let call_void_method = (**self.env).CallVoidMethod.unwrap();
                call_void_method(self.env, input_connection, self.restart_input_method);
            }
        }
    }

    fn state_to_java(&self, state: &AndroidTextInputState) -> ndk_sys::jobject {
        unsafe {
            let new_string_utf = (**self.env).NewStringUTF.unwrap();
            let text_str = CString::new(state.text.as_str()).unwrap_or_else(|_| {
                tracing::error!("Failed to convert text to CString");
                CString::new("").unwrap()
            });
            let jtext = new_string_utf(self.env, text_str.as_ptr());

            let new_object = (**self.env).NewObject.unwrap();
            let get_method_id = (**self.env).GetMethodID.unwrap();
            let find_class = (**self.env).FindClass.unwrap();

            let state_class_name = b"textinput/State\0";
            let state_java_class = find_class(self.env, state_class_name.as_ptr() as _);

            let constructor_sig = b"(Ljava/lang/String;IIII)V\0";
            let constructor = get_method_id(
                self.env,
                state_java_class,
                b"<init>\0".as_ptr() as _,
                constructor_sig.as_ptr() as _,
            );

            let (compose_start, compose_end) = match state.compose {
                Some((start, end)) => (start as i32, end as i32),
                None => (SPAN_UNDEFINED, SPAN_UNDEFINED),
            };

            let jobj = new_object(
                self.env,
                state_java_class,
                constructor,
                jtext,
                state.select.0 as i32,
                state.select.1 as i32,
                compose_start,
                compose_end,
            );

            let delete_local_ref = (**self.env).DeleteLocalRef.unwrap();
            delete_local_ref(self.env, jtext);
            delete_local_ref(self.env, state_java_class);
            jobj
        }
    }

    fn state_from_java(&self, event_state: ndk_sys::jobject) -> AndroidTextInputState {
        unsafe {
            let get_object_field = (**self.env).GetObjectField.unwrap();
            let jtext = get_object_field(self.env, event_state, self.state_class_info.text)
                as ndk_sys::jstring;

            let text = get_utf_str!(self.env, jtext);

            let get_int_field = (**self.env).GetIntField.unwrap();
            let select_start =
                get_int_field(self.env, event_state, self.state_class_info.selection_start);
            let select_end =
                get_int_field(self.env, event_state, self.state_class_info.selection_end);
            let compose_start =
                get_int_field(self.env, event_state, self.state_class_info.composing_region_start);
            let compose_end =
                get_int_field(self.env, event_state, self.state_class_info.composing_region_end);

            let delete_local_ref = (**self.env).DeleteLocalRef.unwrap();
            delete_local_ref(self.env, jtext);

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
            let delete_global_ref = (**self.env).DeleteGlobalRef.unwrap();
            if self.input_connection_class != std::ptr::null_mut() {
                delete_global_ref(self.env, self.input_connection_class);
            }
            if let Some(input_connection) = self.input_connection {
                delete_global_ref(self.env, input_connection);
            }
        }
    }
}

unsafe impl Send for GameTextInput {}
unsafe impl Sync for GameTextInput {}
