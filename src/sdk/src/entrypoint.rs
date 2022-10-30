/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2022 Dyne.org foundation
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

use std::{mem::size_of, slice::from_raw_parts};

/// Success exit code for a contract
pub const SUCCESS: u64 = 0;

/// This macro is used to flag the contract entrypoint function.
/// All contracts must provide such a function and accept a payload.
///
/// The payload is a slice of u8 prepended with a little-endian u64
/// that tells the slice's length.
#[macro_export]
macro_rules! entrypoint {
    ($process_instruction:ident) => {
        /// # Safety
        #[no_mangle]
        pub unsafe extern "C" fn entrypoint(input: *mut u8) -> u64 {
            let instruction_data = $crate::entrypoint::deserialize(input);

            match $process_instruction(&instruction_data) {
                Ok(()) => $crate::entrypoint::SUCCESS,
                Err(e) => e.into(),
            }
        }
    };
}

/// Deserialize a given payload in `entrypoint`
/// # Safety
pub unsafe fn deserialize<'a>(input: *mut u8) -> &'a [u8] {
    let mut offset: usize = 0;

    let instruction_data_len = *(input.add(offset) as *const u64) as usize;
    offset += size_of::<u64>();
    let instruction_data = { from_raw_parts(input.add(offset), instruction_data_len) };

    instruction_data
}
