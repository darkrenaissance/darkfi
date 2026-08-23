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

//! UI sound effects played through rodio (cpal underneath).
//!
//! The audio device is opened lazily on the first sound played. If the
//! device cannot be opened, sounds are silently disabled for the session.
//! On Android the JavaVM/Activity context must be registered with
//! `ndk_context` before cpal initializes its AAudio backend.

use rodio::Source;
use std::{io::Cursor, sync::LazyLock};

macro_rules! e { ($($arg:tt)*) => { error!(target: "app::sfx", $($arg)*); } }

static CLICK_OGA: &[u8] = include_bytes!("../data/click.oga");

struct Sfx {
    /// Keeps the output stream alive for the process lifetime
    _output: rodio::MixerDeviceSink,
    /// Shared mixer the sounds get appended to
    mixer: rodio::mixer::Mixer,
    /// Decoded click sound, cloned on every play
    click: rodio::buffer::SamplesBuffer,
}

static SFX: LazyLock<Option<Sfx>> = LazyLock::new(|| match init() {
    Ok(sfx) => Some(sfx),
    Err(err) => {
        e!("Audio init failed, disabling sounds: {err}");
        None
    }
});

fn init() -> Result<Sfx, Box<dyn std::error::Error>> {
    #[cfg(target_os = "android")]
    init_android_context();

    let output = rodio::DeviceSinkBuilder::open_default_sink()?;
    let mixer = output.mixer().clone();
    let click = rodio::Decoder::try_from(Cursor::new(CLICK_OGA))?.record();
    Ok(Sfx { _output: output, mixer, click })
}

/// cpal's AAudio backend fetches the JavaVM and Activity from the
/// `ndk-context` crate, which miniquad does not initialize itself.
#[cfg(target_os = "android")]
fn init_android_context() {
    use miniquad::native::android;

    unsafe {
        let env = crate::android::get_jni_env();
        let mut vm: *mut android::ndk_sys::JavaVM = std::ptr::null_mut();
        let get_java_vm = (**env).GetJavaVM.unwrap();
        assert_eq!(get_java_vm(env, &mut vm), 0);
        assert!(!vm.is_null());
        assert!(!android::ACTIVITY.is_null());
        ndk_context::initialize_android_context(vm as *mut _, android::ACTIVITY as *mut _);
    }
}

pub fn play_click() {
    if let Some(sfx) = SFX.as_ref() {
        sfx.mixer.add(sfx.click.clone());
    }
}
