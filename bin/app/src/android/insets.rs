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
use std::sync::LazyLock;

type Insets = [f32; 4];
type InsetsSender = async_channel::Sender<Insets>;

struct InsetsGlobals {
    sender: Option<InsetsSender>,
    insets: Insets,
}

static GLOBALS: LazyLock<SyncMutex<InsetsGlobals>> =
    LazyLock::new(|| SyncMutex::new(InsetsGlobals { sender: None, insets: [0.; 4] }));

pub fn set_sender(sender: InsetsSender) {
    GLOBALS.lock().sender = Some(sender);
}

pub fn get_insets() -> Insets {
    GLOBALS.lock().insets.clone()
}

#[no_mangle]
pub unsafe extern "C" fn Java_darkfi_darkfi_1app_ResizingLayout_onApplyInsets(
    _env: *mut ndk_sys::JNIEnv,
    _: ndk_sys::jobject,
    sys_left: ndk_sys::jint,
    sys_top: ndk_sys::jint,
    sys_right: ndk_sys::jint,
    sys_bottom: ndk_sys::jint,
    ime_left: ndk_sys::jint,
    ime_top: ndk_sys::jint,
    ime_right: ndk_sys::jint,
    ime_bottom: ndk_sys::jint,
) {
    debug!(
        target: "android::insets",
        "onApplyInsets() \
            sys=({sys_left}, {sys_top}, {sys_right}, {sys_bottom}) \
            ime=({ime_left}, {ime_top}, {ime_right}, {ime_bottom}) \
        )"
    );
    let mut globals = GLOBALS.lock();
    globals.insets = [sys_left as f32, sys_top as f32, sys_right as f32, sys_bottom as f32];
    if ime_bottom > 0 {
        globals.insets[3] = ime_bottom as f32;
    }
    if let Some(sender) = &globals.sender {
        let _ = sender.try_send(globals.insets.clone());
    } else {
        warn!(target: "android::insets", "Dropping insets notify since no sender is set");
    }
}
