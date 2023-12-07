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

use super::{
    db::{CALLER_ACCESS_DENIED, DB_GET_FAILED},
    error::{ContractError, GenericResult},
};

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

/// Auxiliary function to parse db_get and get_slot return value.
pub(crate) fn parse_ret(ret: i64) -> GenericResult<Option<Vec<u8>>> {
    if ret < 0 {
        match ret {
            CALLER_ACCESS_DENIED => return Err(ContractError::CallerAccessDenied),
            DB_GET_FAILED => return Err(ContractError::DbGetFailed),
            -127 => return Ok(None),
            _ => unimplemented!(),
        }
    }

    let obj = ret as u32;
    let obj_size = get_object_size(obj);
    let mut buf = vec![0u8; obj_size as usize];
    get_object_bytes(&mut buf, obj);

    Ok(Some(buf))
}

/// Everyone can call this. Will return current epoch.
///
/// ```
/// epoch = get_current_epoch();
/// ```
pub fn get_current_epoch() -> u64 {
    unsafe { get_current_epoch_() }
}

/// Everyone can call this. Will return current slot.
///
/// ```
/// slot = get_current_slot();
/// ```
pub fn get_current_slot() -> u64 {
    unsafe { get_current_slot_() }
}

/// Everyone can call this. Will return runtime configured
/// verifying slot.
///
/// ```
/// slot = get_verifying_slot();
/// ```
pub fn get_verifying_slot() -> u64 {
    unsafe { get_verifying_slot_() }
}

/// Everyone can call this. Will return runtime configured
/// verifying slot epoch.
///
/// ```
/// slot = get_verifying_slot_epoch();
/// ```
pub fn get_verifying_slot_epoch() -> u64 {
    unsafe { get_verifying_slot_epoch_() }
}

/// Everyone can call this. Will return requested slot from `SlotStore`.
///
/// ```
/// slot = get_slot(slot);
/// ```
pub fn get_slot(slot: u64) -> GenericResult<Option<Vec<u8>>> {
    let ret = unsafe { get_slot_(slot) };
    parse_ret(ret)
}

/// Everyone can call this. Will return current blockchain timestamp.
///
/// ```
/// timestamp = get_blockchain_time();
/// ```
pub fn get_blockchain_time() -> u64 {
    unsafe { get_blockchain_time_() }
}

extern "C" {
    fn set_return_data_(ptr: *const u8, len: u32) -> i64;
    fn put_object_bytes_(ptr: *const u8, len: u32) -> i64;
    fn get_object_bytes_(ptr: *const u8, len: u32) -> i64;
    fn get_object_size_(len: u32) -> i64;

    fn get_current_epoch_() -> u64;
    fn get_current_slot_() -> u64;
    fn get_verifying_slot_() -> u64;
    fn get_verifying_slot_epoch_() -> u64;
    fn get_slot_(slot: u64) -> i64;
    fn get_blockchain_time_() -> u64;
}
