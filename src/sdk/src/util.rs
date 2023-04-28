/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
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

use super::error::{ContractError, GenericResult};

pub const CALL_FAILED: u64 = 0;

pub fn set_return_data(data: &[u8]) -> Result<(), ContractError> {
    unsafe {
        match set_return_data_(data.as_ptr(), data.len() as u32) {
            0 => Ok(()),
            errcode => Err(ContractError::from(errcode)),
        }
    }
}

pub fn put_object_bytes(data: &[u8]) -> i64 {
    unsafe { put_object_bytes_(data.as_ptr(), data.len() as u32) }
}

pub fn get_object_bytes(data: &mut [u8], object_index: u32) -> i64 {
    unsafe { get_object_bytes_(data.as_mut_ptr(), object_index) }
}

pub fn get_object_size(object_index: u32) -> i64 {
    unsafe { get_object_size_(object_index) }
}

/// Everyone can call this. Will return current system timestamp.
///
/// ```
/// timestamp = get_system_time();
/// ```
pub fn get_system_time() -> GenericResult<u64> {
    let ret = unsafe { get_system_time_() };

    match ret {
        // 0 here means system time is less or equal than UNIX_EPOCH
        CALL_FAILED => return Err(ContractError::GetSystemTimeFailed),
        // In any other case we can return the value
        _ => Ok(ret),
    }
}

extern "C" {
    fn set_return_data_(ptr: *const u8, len: u32) -> i64;
    fn put_object_bytes_(ptr: *const u8, len: u32) -> i64;
    fn get_object_bytes_(ptr: *const u8, len: u32) -> i64;
    fn get_object_size_(len: u32) -> i64;

    fn get_system_time_() -> u64;
}
