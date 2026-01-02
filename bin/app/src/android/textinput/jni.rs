/* GameTextInput JNI bridge functions */

use crate::android::textinput::{init_game_text_input, GAME_TEXT_INPUT};
use miniquad::native::android::ndk_sys;

/// Set the InputConnection for GameTextInput (called from Java)
///
/// This follows the official Android GameTextInput integration pattern:
/// https://developer.android.com/games/agdk/add-support-for-text-input
///
/// Called from MainActivity when the InputConnection is created. It passes
/// the Java InputConnection object to the native GameTextInput library.
///
/// # Arguments
/// * `env` - JNI environment pointer
/// * `_class` - JNI class reference (unused)
/// * `input_connection` - Java InputConnection object from textinput.InputConnection
#[no_mangle]
pub extern "C" fn Java_darkfi_darkfi_1app_MainActivity_setInputConnectionNative(
    _env: *mut ndk_sys::JNIEnv,
    _class: ndk_sys::jclass,
    input_connection: ndk_sys::jobject,
) {
    // Initialize GameTextInput first
    init_game_text_input();

    // Now set the InputConnection
    if let Some(gti) = &mut *GAME_TEXT_INPUT.write() {
        gti.set_input_connection(input_connection);
    }
}

/// Process IME state event from Java Listener.stateChanged()
///
/// This follows the official Android GameTextInput integration pattern.
/// Called from the Java InputConnection's Listener whenever the IME sends
/// a state change (text typed, cursor moved, etc.).
///
/// # Arguments
/// * `env` - JNI environment pointer
/// * `_class` - JNI class reference (unused)
/// * `soft_keyboard_event` - Java State object from textinput.State
#[no_mangle]
pub extern "C" fn Java_darkfi_darkfi_1app_MainActivity_onTextInputEventNative(
    _env: *mut ndk_sys::JNIEnv,
    _class: ndk_sys::jclass,
    soft_keyboard_event: ndk_sys::jobject,
) {
    if let Some(gti) = &mut *GAME_TEXT_INPUT.write() {
        gti.process_event(soft_keyboard_event);
    }
}
