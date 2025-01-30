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

/// Print a message to the log
#[macro_export]
macro_rules! msg {
    ($msg:expr) => {
        $crate::log::drk_log($msg)
    };
    ($($arg:tt)*) => ($crate::log::drk_log(&format!($($arg)*)));
}

#[inline]
pub fn drk_log(message: &str) {
    #[cfg(target_arch = "wasm32")]
    unsafe {
        drk_log_(message.as_ptr(), message.len());
    }

    #[cfg(not(target_arch = "wasm32"))]
    println!("{}", message);
}

#[cfg(target_arch = "wasm32")]
extern "C" {
    fn drk_log_(ptr: *const u8, len: usize);
}
