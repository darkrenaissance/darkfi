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

//! Android video decoder JNI functions for MediaCodec integration

use miniquad::native::android::{self, ndk_sys, ndk_utils};
use parking_lot::Mutex as SyncMutex;
use std::{
    collections::HashMap,
    sync::{mpsc, LazyLock},
};

use super::util::get_jni_env;

pub struct DecodedFrame {
    pub width: usize,
    pub height: usize,
    pub y_data: Vec<u8>,
    pub u_data: Vec<u8>,
    pub v_data: Vec<u8>,
}

struct VideoDecoderGlobals {
    senders: HashMap<usize, mpsc::Sender<DecodedFrame>>,
    next_id: usize,
}

unsafe impl Send for VideoDecoderGlobals {}
unsafe impl Sync for VideoDecoderGlobals {}

static VIDEO_DECODER_GLOBALS: LazyLock<SyncMutex<VideoDecoderGlobals>> =
    LazyLock::new(|| SyncMutex::new(VideoDecoderGlobals { senders: HashMap::new(), next_id: 0 }));

fn send(id: usize, frame: DecodedFrame) {
    let globals = &VIDEO_DECODER_GLOBALS.lock();
    if let Some(sender) = globals.senders.get(&id) {
        let _ = sender.send(frame);
    }
}

pub fn register(sender: mpsc::Sender<DecodedFrame>) -> usize {
    let mut globals = VIDEO_DECODER_GLOBALS.lock();
    let id = globals.next_id;
    globals.next_id += 1;
    globals.senders.insert(id, sender);
    id
}

pub fn unregister(id: usize) {
    VIDEO_DECODER_GLOBALS.lock().senders.remove(&id);
}

#[no_mangle]
pub unsafe extern "C" fn Java_videodecode_VideoDecoder_onFrameDecoded(
    env: *mut ndk_sys::JNIEnv,
    _: ndk_sys::jobject,
    decoder_id: ndk_sys::jint,
    y_data: ndk_sys::jbyteArray,
    u_data: ndk_sys::jbyteArray,
    v_data: ndk_sys::jbyteArray,
    width: ndk_sys::jint,
    height: ndk_sys::jint,
) {
    use std::slice;

    let get_array_length = (**env).GetArrayLength.unwrap();
    let y_len = get_array_length(env, y_data) as usize;
    let u_len = get_array_length(env, u_data) as usize;
    let v_len = get_array_length(env, v_data) as usize;

    let get_byte_array_elements = (**env).GetByteArrayElements.unwrap();
    let y_ptr = get_byte_array_elements(env, y_data, std::ptr::null_mut()) as *const u8;
    let u_ptr = get_byte_array_elements(env, u_data, std::ptr::null_mut()) as *const u8;
    let v_ptr = get_byte_array_elements(env, v_data, std::ptr::null_mut()) as *const u8;

    let y_vec = slice::from_raw_parts(y_ptr, y_len).to_vec();
    let u_vec = slice::from_raw_parts(u_ptr, u_len).to_vec();
    let v_vec = slice::from_raw_parts(v_ptr, v_len).to_vec();

    let release_byte_array_elements = (**env).ReleaseByteArrayElements.unwrap();
    release_byte_array_elements(env, y_data, y_ptr as *mut i8, 0);
    release_byte_array_elements(env, u_data, u_ptr as *mut i8, 0);
    release_byte_array_elements(env, v_data, v_ptr as *mut i8, 0);

    let frame = DecodedFrame {
        width: width as usize,
        height: height as usize,
        y_data: y_vec,
        u_data: u_vec,
        v_data: v_vec,
    };

    send(decoder_id as usize, frame);
}

pub struct VideoDecoderHandle {
    pub obj: ndk_sys::jobject,
}

impl Drop for VideoDecoderHandle {
    fn drop(&mut self) {
        unsafe {
            let env = get_jni_env();
            let delete_local_ref = (**env).DeleteLocalRef.unwrap();
            delete_local_ref(env, self.obj);
        }
    }
}

pub fn videodecoder_init(path: &str) -> Option<VideoDecoderHandle> {
    unsafe {
        let env = get_jni_env();

        let decoder_obj = ndk_utils::call_object_method!(
            env,
            android::ACTIVITY,
            "createVideoDecoder",
            "()Lvideodecode/VideoDecoder;"
        );

        if decoder_obj.is_null() {
            error!(target: "android::vid", "Failed to create VideoDecoder object");
            return None;
        }

        let cpath = std::ffi::CString::new(path).unwrap();
        let jpath = (**env).NewStringUTF.unwrap()(env, cpath.as_ptr());

        let result =
            ndk_utils::call_bool_method!(env, decoder_obj, "init", "(Ljava/lang/String;)Z", jpath);

        let delete_local_ref = (**env).DeleteLocalRef.unwrap();
        delete_local_ref(env, jpath);

        if result == 0 {
            error!(target: "android::vid", "VideoDecoder.init() failed");
            return None;
        }

        Some(VideoDecoderHandle { obj: decoder_obj })
    }
}

pub fn videodecoder_set_id(decoder_obj: ndk_sys::jobject, id: usize) {
    unsafe {
        let env = get_jni_env();
        ndk_utils::call_void_method!(env, decoder_obj, "setDecoderId", "(I)V", id as i32);
    }
}

pub fn videodecoder_decode_all(decoder_obj: ndk_sys::jobject) -> i32 {
    unsafe {
        let env = get_jni_env();
        ndk_utils::call_int_method!(env, decoder_obj, "decodeAll", "()I")
    }
}
