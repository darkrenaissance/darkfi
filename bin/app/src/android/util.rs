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

use miniquad::native::android::{self, ndk_sys};
use std::cell::RefCell;

thread_local! {
    static JNI_ENV: RefCell<JniEnvHolder> = RefCell::new(JniEnvHolder {
        env: std::ptr::null_mut(),
        vm: std::ptr::null_mut(),
    });
}

struct JniEnvHolder {
    env: *mut ndk_sys::JNIEnv,
    vm: *mut ndk_sys::JavaVM,
}

impl Drop for JniEnvHolder {
    fn drop(&mut self) {
        assert!(!self.env.is_null());
        unsafe {
            let detach_current_thread = (**self.vm).DetachCurrentThread.unwrap();
            let _ = detach_current_thread(self.vm);
        }
    }
}

/// Get the JNIEnv for the current thread, attaching if necessary.
/// The returned pointer is cached per-thread and automatically detached
/// when the thread exits.
pub unsafe fn get_jni_env() -> *mut ndk_sys::JNIEnv {
    JNI_ENV.with(|holder| {
        let mut holder = holder.borrow_mut();
        if !holder.env.is_null() {
            return holder.env;
        }

        // Call miniquad's attach_jni_env to get the env
        let env = android::attach_jni_env();
        assert!(!env.is_null());

        // Retrieve the JavaVM from the JNIEnv
        let get_java_vm = (**env).GetJavaVM.unwrap();
        let mut vm: *mut ndk_sys::JavaVM = std::ptr::null_mut();
        let res = get_java_vm(env, &mut vm);
        assert!(res == 0);
        assert!(!vm.is_null());

        holder.env = env;
        holder.vm = vm;
        env
    })
}

/// Check for pending Java exceptions and log them
///
/// This function should be called after any JNI call that might throw an exception.
/// It will use ExceptionDescribe to print the exception to logcat, then clear it.
unsafe fn check_except(env: *mut ndk_sys::JNIEnv, context: &str) {
    let exception_check = (**env).ExceptionCheck.unwrap();
    if exception_check(env) != 0 {
        // Use ExceptionDescribe to print the exception stack trace to logcat
        // This is safe to call even with a pending exception and handles StackOverflowError gracefully
        let exception_describe = (**env).ExceptionDescribe.unwrap();
        exception_describe(env);

        let exception_clear = (**env).ExceptionClear.unwrap();
        exception_clear(env);

        panic!("Java exception detected in {context}");
    }
}
