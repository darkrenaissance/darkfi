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

use darkfi_serial::{Decodable, Encodable};
use std::io::Cursor;

use crate::{
    error::{ContractError, GenericResult},
    tx::TransactionHash,
};

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

/// Internal function, get raw bytes from the objects store
pub fn get_object_bytes(data: &mut [u8], object_index: u32) -> i64 {
    unsafe { get_object_bytes_(data.as_mut_ptr(), object_index) }
}

/// Internal function, get bytes size for an object in the store
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

fn parse_retval_u32(ret: i64) -> GenericResult<u32> {
    if ret < 0 {
        return Err(ContractError::from(ret))
    }
    assert!(ret >= 0);
    // This should always be possible
    let obj = ret as u32;
    Ok(obj)
}

/// Everyone can call this. Will return runtime configured
/// verifying block height.
///
/// ```
/// block_height = get_verifying_block_height();
/// ```
pub fn get_verifying_block_height() -> GenericResult<u32> {
    let ret = unsafe { get_verifying_block_height_() };
    parse_retval_u32(ret)
}

/// Everyone can call this. Will return runtime configured
/// block target.
///
/// ```
/// block_target = get_block_target();
/// ```
pub fn get_block_target() -> GenericResult<u32> {
    let ret = unsafe { get_block_target_() };
    parse_retval_u32(ret)
}

/// Only deploy(), metadata() and exec() can call this. Will return runtime configured
/// transaction hash.
///
/// ```
/// tx_hash = get_tx_hash();
/// ```
pub fn get_tx_hash() -> GenericResult<TransactionHash> {
    let ret = unsafe { get_tx_hash_() };
    let obj = parse_retval_u32(ret)?;
    let mut tx_hash_data = [0u8; 32];
    assert_eq!(get_object_size(obj), 32);
    get_object_bytes(&mut tx_hash_data, obj);
    Ok(TransactionHash(tx_hash_data))
}

/// Only deploy(), metadata() and exec() can call this. Will return runtime configured
/// verifying block height.
///
/// ```
/// call_idx = get_call_index();
/// ```
pub fn get_call_index() -> GenericResult<u8> {
    let ret = unsafe { get_call_index_() };
    if ret < 0 {
        return Err(ContractError::from(ret))
    }
    assert!(ret >= 0);
    // This should always be possible
    let obj = ret as u8;
    Ok(obj)
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
pub fn get_tx(hash: &TransactionHash) -> GenericResult<Option<Vec<u8>>> {
    let mut buf = vec![];
    hash.encode(&mut buf)?;

    let ret = unsafe { get_tx_(buf.as_ptr()) };
    parse_ret(ret)
}

/// Only metadata() and exec() can call this. Will return transaction
/// location by provided hash.
///
/// ```
/// (block_height, tx_index) = get_tx_location(hash)?;
/// ```
pub fn get_tx_location(hash: &TransactionHash) -> GenericResult<(u32, u16)> {
    let mut buf = vec![];
    hash.encode(&mut buf)?;

    let ret = unsafe { get_tx_location_(buf.as_ptr()) };
    let loc_data = parse_ret(ret)?.ok_or(ContractError::DbGetFailed)?;
    let mut cursor = Cursor::new(loc_data);
    Ok((Decodable::decode(&mut cursor)?, Decodable::decode(&mut cursor)?))
}

extern "C" {
    fn set_return_data_(ptr: *const u8, len: u32) -> i64;
    fn get_object_bytes_(ptr: *const u8, len: u32) -> i64;
    fn get_object_size_(len: u32) -> i64;

    fn get_verifying_block_height_() -> i64;
    fn get_block_target_() -> i64;
    fn get_tx_hash_() -> i64;
    fn get_call_index_() -> i64;
    fn get_blockchain_time_() -> i64;
    fn get_last_block_height_() -> i64;
    fn get_tx_(ptr: *const u8) -> i64;
    fn get_tx_location_(ptr: *const u8) -> i64;
}
