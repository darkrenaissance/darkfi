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

use darkfi_serial::Encodable;

use super::{
    crypto::ContractId,
    error::{ContractError, GenericResult},
    util::parse_ret,
};

pub type DbHandle = u32;

pub const DB_SUCCESS: i64 = 0;
pub const CALLER_ACCESS_DENIED: i64 = -1;
pub const DB_INIT_FAILED: i64 = -2;
pub const DB_LOOKUP_FAILED: i64 = -3;
pub const DB_GET_FAILED: i64 = -4;
pub const DB_CONTAINS_KEY_FAILED: i64 = -5;
pub const DB_SET_FAILED: i64 = -6;
pub const DB_DEL_FAILED: i64 = -7;

/// Create a new database instance for the given contract.
/// This should be called in the `init_contract()` section to create any databases
/// that the contract might need or use.
///
/// Returns a `DbHandle` which provides methods for reading and writing.
pub fn db_init(contract_id: ContractId, db_name: &str) -> GenericResult<DbHandle> {
    unsafe {
        let mut len = 0;
        let mut buf = vec![];
        len += contract_id.encode(&mut buf)?;
        len += db_name.to_string().encode(&mut buf)?;

        let ret = db_init_(buf.as_ptr(), len as u32);

        if ret < 0 {
            match ret {
                CALLER_ACCESS_DENIED => return Err(ContractError::CallerAccessDenied),
                DB_INIT_FAILED => return Err(ContractError::DbInitFailed),
                _ => unimplemented!(),
            }
        }

        Ok(ret as u32)
    }
}

/// Everyone can call this. Assumes that the database already went through `db_init()`.
pub fn db_lookup(contract_id: ContractId, db_name: &str) -> GenericResult<DbHandle> {
    unsafe {
        let mut len = 0;
        let mut buf = vec![];
        len += contract_id.encode(&mut buf)?;
        len += db_name.to_string().encode(&mut buf)?;

        let ret = db_lookup_(buf.as_ptr(), len as u32);

        if ret < 0 {
            match ret {
                CALLER_ACCESS_DENIED => return Err(ContractError::CallerAccessDenied),
                DB_LOOKUP_FAILED => return Err(ContractError::DbLookupFailed),
                _ => unimplemented!(),
            }
        }

        Ok(ret as u32)
    }
}

/// Everyone can call this. Will read a key from the key-value store.
///
/// ```
/// value = db_get(db_handle, key);
/// ```
pub fn db_get(db_handle: DbHandle, key: &[u8]) -> GenericResult<Option<Vec<u8>>> {
    let mut len = 0;
    let mut buf = vec![];
    len += db_handle.encode(&mut buf)?;
    len += key.to_vec().encode(&mut buf)?;

    let ret = unsafe { db_get_(buf.as_ptr(), len as u32) };
    parse_ret(ret)
}

/// Everyone can call this. Checks if a key is contained in the key-value store.
///
/// ```
/// if db_contains_key(db_handle, key) {
///     println!("true");
/// }
/// ```
pub fn db_contains_key(db_handle: DbHandle, key: &[u8]) -> GenericResult<bool> {
    let mut len = 0;
    let mut buf = vec![];
    len += db_handle.encode(&mut buf)?;
    len += key.to_vec().encode(&mut buf)?;

    let ret = unsafe { db_contains_key_(buf.as_ptr(), len as u32) };

    match ret {
        CALLER_ACCESS_DENIED => Err(ContractError::CallerAccessDenied),
        DB_CONTAINS_KEY_FAILED => Err(ContractError::DbContainsKeyFailed),
        0 => Ok(false),
        1 => Ok(true),
        _ => unimplemented!(),
    }
}

/// Only update() can call this. Set a value within the transaction.
///
/// ```
/// db_set(tx_handle, key, value);
/// ```
pub fn db_set(db_handle: DbHandle, key: &[u8], value: &[u8]) -> GenericResult<()> {
    // Check entry for tx_handle is not None
    unsafe {
        let mut len = 0;
        let mut buf = vec![];
        len += db_handle.encode(&mut buf)?;
        len += key.to_vec().encode(&mut buf)?;
        len += value.to_vec().encode(&mut buf)?;

        match db_set_(buf.as_ptr(), len as u32) {
            CALLER_ACCESS_DENIED => Err(ContractError::CallerAccessDenied),
            DB_SET_FAILED => Err(ContractError::DbSetFailed),
            DB_SUCCESS => Ok(()),
            _ => unreachable!(),
        }
    }
}

/// Only update() can call this. Removes a key from the db.
///
/// ```
///     db_del(tx_handle, key);
/// ```
pub fn db_del(db_handle: DbHandle, key: &[u8]) -> GenericResult<()> {
    // Check entry for tx_handle is not None
    unsafe {
        let mut len = 0;
        let mut buf = vec![];
        len += db_handle.encode(&mut buf)?;
        len += key.to_vec().encode(&mut buf)?;

        match db_del_(buf.as_ptr(), len as u32) {
            CALLER_ACCESS_DENIED => Err(ContractError::CallerAccessDenied),
            DB_DEL_FAILED => Err(ContractError::DbDelFailed),
            DB_SUCCESS => Ok(()),
            _ => unreachable!(),
        }
    }
}

/// Only deploy() can call this.
pub fn zkas_db_set(bincode: &[u8]) -> GenericResult<()> {
    unsafe {
        let mut len = 0;
        let mut buf = vec![];
        len += bincode.to_vec().encode(&mut buf)?;

        match zkas_db_set_(buf.as_ptr(), len as u32) {
            CALLER_ACCESS_DENIED => Err(ContractError::CallerAccessDenied),
            DB_SET_FAILED => Err(ContractError::DbSetFailed),
            DB_SUCCESS => Ok(()),
            _ => unreachable!(),
        }
    }
}

extern "C" {
    fn db_init_(ptr: *const u8, len: u32) -> i64;
    fn db_lookup_(ptr: *const u8, len: u32) -> i64;
    fn db_get_(ptr: *const u8, len: u32) -> i64;
    fn db_contains_key_(ptr: *const u8, len: u32) -> i64;
    fn db_set_(ptr: *const u8, len: u32) -> i64;
    fn db_del_(ptr: *const u8, len: u32) -> i64;

    fn zkas_db_set_(ptr: *const u8, len: u32) -> i64;
}
