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

pub mod insets;
pub mod textinput;
mod util;
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
