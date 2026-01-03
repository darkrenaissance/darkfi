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

use miniquad::native::android::ndk_sys;

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
