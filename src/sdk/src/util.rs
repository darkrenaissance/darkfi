/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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

use darkfi_serial::Encodable;

use super::error::{ContractError, GenericResult};

/// Calls the `set_return_data` WASM function. Returns Ok(()) on success.
/// Otherwise, convert the i64 error code into a [`ContractError`].
pub fn set_return_data(data: &[u8]) -> Result<(), ContractError> {
    // Ensure that the number of bytes fits within the u32 data type.
    match u32::try_from(data.len()) {
        Ok(len) => unsafe {
            match set_return_data_(data.as_ptr(), len) {
                0 => Ok(()),
                errcode => Err(ContractError::from(errcode)),
            }
        },
        Err(_) => Err(ContractError::DataTooLarge),
    }
}

pub fn put_object_bytes(data: &[u8]) -> Result<i64, ContractError> {
    // Ensure that the number of bytes fits within the u32 data type.
    match u32::try_from(data.len()) {
        Ok(len) => unsafe { Ok(put_object_bytes_(data.as_ptr(), len)) },
        Err(_) => Err(ContractError::DataTooLarge),
    }
}

pub fn get_object_bytes(data: &mut [u8], object_index: u32) -> i64 {
    unsafe { get_object_bytes_(data.as_mut_ptr(), object_index) }
}

pub fn get_object_size(object_index: u32) -> i64 {
    unsafe { get_object_size_(object_index) }
}

/// Auxiliary function to parse db_get return value.
/// If either of these functions returns a negative integer error code,
/// convert it into a [`ContractError`].
pub(crate) fn parse_ret(ret: i64) -> GenericResult<Option<Vec<u8>>> {
    // Negative values represent an error code.
    if ret < 0 {
        // However here on the special case, we'll return Ok(None)
        if ret == crate::error::DB_GET_EMPTY {
            return Ok(None)
        }

        return Err(ContractError::from(ret))
    }

    // Ensure that the returned value fits into the u32 datatype.
    // Note that any negative cases should be caught by the `unimplemented`
    // match arm above.
    let obj = match u32::try_from(ret) {
        Ok(obj) => obj,
        Err(_) => return Err(ContractError::SetRetvalError),
    };
    let obj_size = get_object_size(obj);
    let mut buf = vec![0u8; obj_size as usize];
    get_object_bytes(&mut buf, obj);

    Ok(Some(buf))
}

/// Everyone can call this. Will return runtime configured
/// verifying block height.
///
/// ```
/// block_height = get_verifying_block_height();
/// ```
pub fn get_verifying_block_height() -> u64 {
    unsafe { get_verifying_block_height_() }
}

/// Everyone can call this. Will return runtime configured
/// verifying block height epoch.
///
/// ```
/// epoch = get_verifying_block_height_epoch();
/// ```
pub fn get_verifying_block_height_epoch() -> u64 {
    unsafe { get_verifying_block_height_epoch_() }
}

/// Everyone can call this. Will return current blockchain timestamp.
///
/// ```
/// timestamp = get_blockchain_time();
/// ```
pub fn get_blockchain_time() -> GenericResult<Option<Vec<u8>>> {
    let ret = unsafe { get_blockchain_time_() };
    parse_ret(ret)
}

/// Only exec() can call this. Will return last block height.
///
/// ```
/// last_block_height = get_last_block_height();
/// ```
pub fn get_last_block_height() -> GenericResult<Option<Vec<u8>>> {
    let ret = unsafe { get_last_block_height_() };
    parse_ret(ret)
}

/// Only metadata() and exec() can call this. Will return transaction
/// bytes by provided hash.
///
/// ```
/// tx_bytes = get_tx(hash);
/// tx = deserialize(&tx_bytes)?;
/// ```
pub fn get_tx(hash: blake3::Hash) -> GenericResult<Option<Vec<u8>>> {
    let mut buf = vec![];
    hash.encode(&mut buf)?;

    let ret = unsafe { get_tx_(buf.as_ptr()) };
    parse_ret(ret)
}

extern "C" {
    fn set_return_data_(ptr: *const u8, len: u32) -> i64;
    fn put_object_bytes_(ptr: *const u8, len: u32) -> i64;
    fn get_object_bytes_(ptr: *const u8, len: u32) -> i64;
    fn get_object_size_(len: u32) -> i64;

    fn get_verifying_block_height_() -> u64;
    fn get_verifying_block_height_epoch_() -> u64;
    fn get_blockchain_time_() -> i64;
    fn get_last_block_height_() -> i64;
    fn get_tx_(ptr: *const u8) -> i64;
}
