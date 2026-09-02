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
//!
//! On Android a started but idle stream keeps the audio pipeline and its
//! system wakelock active, so the stream is paused once the sounds have
//! finished and resumed when the next one plays.

use rodio::Source;
use std::{io::Cursor, sync::LazyLock};

#[cfg(any(target_os = "android", feature = "emulate-android"))]
use std::{
    sync::mpsc::{self, RecvTimeoutError},
    time::Duration,
};

#[cfg(any(target_os = "android", feature = "emulate-android"))]
use crate::util::spawn_thread;

macro_rules! e { ($($arg:tt)*) => { error!(target: "app::sfx", $($arg)*); } }

static CLICK_OGA: &[u8] = include_bytes!("../data/sfx/click.oga");
static COMMUP_OGA: &[u8] = include_bytes!("../data/sfx/commup.oga");
static CLOAK_OGA: &[u8] = include_bytes!("../data/sfx/cloak.oga");

/// Idle window after the last sound before the stream is paused.
/// Pausing flushes buffered audio, so it must exceed the duration of
/// the longest sound (cloak, ~3.4s).
#[cfg(any(target_os = "android", feature = "emulate-android"))]
const IDLE_MARGIN: Duration = Duration::from_secs(6);

struct Sfx {
    /// Keeps the output stream alive for the process lifetime
    _output: rodio::MixerDeviceSink,
    /// Shared mixer the sounds get appended to
    mixer: rodio::mixer::Mixer,
    /// Decoded click sound, cloned on every play
    click: rodio::buffer::SamplesBuffer,
    /// Decoded startup sound
    commup: rodio::buffer::SamplesBuffer,
    /// Used for showing p2p overlay
    cloak: rodio::buffer::SamplesBuffer,
    /// Signals playback activity to the idle reaper thread
    #[cfg(any(target_os = "android", feature = "emulate-android"))]
    activity: mpsc::Sender<()>,
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
    let commup = rodio::Decoder::try_from(Cursor::new(COMMUP_OGA))?.record();
    let cloak = rodio::Decoder::try_from(Cursor::new(CLOAK_OGA))?.record();

    // Leaving the sound mixer on in Android will just hold a wakelock
    // and drain battery. It is playing a silent sound so we must pause
    // the stream.
    #[cfg(any(target_os = "android", feature = "emulate-android"))]
    let activity = spawn_idle_reaper();

    Ok(Sfx {
        _output: output,
        mixer,
        click,
        commup,
        cloak,
        #[cfg(any(target_os = "android", feature = "emulate-android"))]
        activity,
    })
}

/// Pauses the output stream once the idle window passes without playback,
/// and resumes it when the next sound is played. A started but idle
/// AAudio stream keeps the audio pipeline and its system wakelock open,
/// so it must not stay running while nothing plays. Pausing flushes
/// buffered audio, which is why the idle window always exceeds the
/// longest sound. cpal's pause and play block on a state change with a
/// timeout, so they run on this dedicated thread instead of the UI
/// thread.
#[cfg(any(target_os = "android", feature = "emulate-android"))]
fn spawn_idle_reaper() -> mpsc::Sender<()> {
    let (tx, rx) = mpsc::channel();
    spawn_thread("sfx-idle", move || {
        // The stream runs from creation, then alternates between running
        // while sounds keep arriving and paused once the idle window
        // elapses, so every play() lands on a paused stream and every
        // pause() on a running one.
        loop {
            loop {
                match rx.recv_timeout(IDLE_MARGIN) {
                    // Another sound has been played so continue loop
                    Ok(()) => {}
                    // Idle timeout so pause the stream
                    Err(RecvTimeoutError::Timeout) => {
                        if let Some(sfx) = &*SFX {
                            sfx._output.pause();
                        }
                        break;
                    }
                    // Receiver dropped which means this thread can exit
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            }
            match rx.recv() {
                // Sound is received for playing
                Ok(()) => {
                    if let Some(sfx) = &*SFX {
                        sfx._output.play();
                    }
                }
                // Receiver dropped which means this thread can exit
                Err(_) => return,
            }
        }
    });
    tx
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

impl Sfx {
    fn play(&self, sound: &rodio::buffer::SamplesBuffer) {
        // Notify the reaper before appending, so the stream is resumed
        #[cfg(any(target_os = "android", feature = "emulate-android"))]
        let _ = self.activity.send(());
        self.mixer.add(sound.clone());
    }
}

pub fn play_click() {
    if let Some(sfx) = &*SFX {
        sfx.play(&sfx.click);
    }
}

pub fn play_commup() {
    if let Some(sfx) = &*SFX {
        sfx.play(&sfx.commup);
    }
}

pub fn play_cloak() {
    if let Some(sfx) = &*SFX {
        sfx.play(&sfx.cloak);
    }
}
