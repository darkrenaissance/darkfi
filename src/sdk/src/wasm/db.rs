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

use darkfi_serial::Encodable;

use crate::{
    crypto::ContractId,
    error::{ContractError, GenericResult},
    wasm,
};

pub type DbHandle = u32;

/// Create a new on-chain database instance for the given contract.
/// A contract is only able to create a db for itself.
///
/// Returns a `DbHandle` which provides methods for reading and writing.
///
/// ## Permissions
/// * `ContractSection::Deploy`
pub fn db_init(contract_id: ContractId, db_name: &str) -> GenericResult<DbHandle> {
    let mut len = 0;
    let mut buf = vec![];
    len += contract_id.encode(&mut buf)?;
    len += db_name.to_string().encode(&mut buf)?;

    let ret = unsafe { db_init_(buf.as_ptr(), len as u32) };

    if ret < 0 {
        return Err(ContractError::from(ret))
    }

    Ok(ret as u32)
}

/// Open an existing on-chain database instance for the given contract.
/// A contract is able to read any on-chain database.
///
/// Returns a `DbHandle` which is used with methods for reading and writing.
///
/// ## Permissions
/// * `ContractSection::Deploy`
/// * `ContractSection::Metadata`
/// * `ContractSection::Exec`
/// * `ContractSection::Update`
pub fn db_lookup(contract_id: ContractId, db_name: &str) -> GenericResult<DbHandle> {
    db_lookup_internal(contract_id, db_name, false)
}

/// Open a tx-local database instance for the given contract.
/// A contract is able to read any tx-local database.
///
/// If the calling contract is opening its own db, the db will be created
/// and initialized in-memory.
///
/// Returns a `DbHandle` which is used with methods for reading and writing.
///
/// ## Permissions
/// * `ContractSection::Deploy`
/// * `ContractSection::Metadata`
/// * `ContractSection::Exec`
/// * `ContractSection::Update`
pub fn db_lookup_local(contract_id: ContractId, db_name: &str) -> GenericResult<DbHandle> {
    db_lookup_internal(contract_id, db_name, true)
}

/// Internal function for `db_lookup` which branches to either on-chain or
/// transaction-local.
fn db_lookup_internal(
    contract_id: ContractId,
    db_name: &str,
    local: bool,
) -> GenericResult<DbHandle> {
    let mut len = 0;
    let mut buf = vec![];
    len += contract_id.encode(&mut buf)?;
    len += db_name.to_string().encode(&mut buf)?;

    let ret = unsafe {
        if local {
            db_lookup_local_(buf.as_ptr(), len as u32)
        } else {
            db_lookup_(buf.as_ptr(), len as u32)
        }
    };

    if ret < 0 {
        return Err(ContractError::from(ret))
    }

    Ok(ret as u32)
}

/// Read a key from the on-chain key-value store given a `DbHandle` and `key`.
///
/// Returns the `Vec<u8>` value if the key exists, otherwise `None`.
///
/// ## Permissions
/// * `ContractSection::Deploy`
/// * `ContractSection::Metadata`
/// * `ContractSection::Exec`
pub fn db_get(db_handle: DbHandle, key: &[u8]) -> GenericResult<Option<Vec<u8>>> {
    db_get_internal(db_handle, key, false)
}

/// Read a key from the tx-local key-value store given a `DbHandle` and `key`.
///
/// Returns the `Vec<u8>` value if the key exists, otherwise `None`.
///
/// ## Permissions
/// * `ContractSection::Deploy`
/// * `ContractSection::Metadata`
/// * `ContractSection::Exec`
pub fn db_get_local(db_handle: DbHandle, key: &[u8]) -> GenericResult<Option<Vec<u8>>> {
    db_get_internal(db_handle, key, true)
}

/// Internal function for `db_get` which branches to either on-chain or
/// transaction-local.
fn db_get_internal(db_handle: DbHandle, key: &[u8], local: bool) -> GenericResult<Option<Vec<u8>>> {
    let mut len = 0;
    let mut buf = vec![];
    len += db_handle.encode(&mut buf)?;
    len += key.encode(&mut buf)?;

    let ret = unsafe {
        if local {
            db_get_local_(buf.as_ptr(), len as u32)
        } else {
            db_get_(buf.as_ptr(), len as u32)
        }
    };

    wasm::util::parse_ret(ret)
}

/// Check if a key is contained in the on-chain key-value store given a
/// `DbHandle` and `key`.
///
/// Returns a boolean value.
///
/// ## Permissions
/// * `ContractSection::Deploy`
/// * `ContractSection::Metadata`
/// * `ContractSection::Exec`
pub fn db_contains_key(db_handle: DbHandle, key: &[u8]) -> GenericResult<bool> {
    db_contains_key_internal(db_handle, key, false)
}

/// Check if a key is contained in the tx-local key-value store given a
/// `DbHandle` and `key`.
///
/// Returns a boolean value.
///
/// ## Permissions
/// * `ContractSection::Deploy`
/// * `ContractSection::Metadata`
/// * `ContractSection::Exec`
pub fn db_contains_key_local(db_handle: DbHandle, key: &[u8]) -> GenericResult<bool> {
    db_contains_key_internal(db_handle, key, true)
}

/// Internal function for `db_contains_key` which branches to either on-chain
/// or transaction-local.
fn db_contains_key_internal(db_handle: DbHandle, key: &[u8], local: bool) -> GenericResult<bool> {
    let mut len = 0;
    let mut buf = vec![];
    len += db_handle.encode(&mut buf)?;
    len += key.encode(&mut buf)?;

    let ret = unsafe {
        if local {
            db_contains_key_local_(buf.as_ptr(), len as u32)
        } else {
            db_contains_key_(buf.as_ptr(), len as u32)
        }
    };

    if ret < 0 {
        return Err(ContractError::from(ret))
    }

    match ret {
        0 => Ok(false),
        1 => Ok(true),
        _ => unreachable!(),
    }
}

/// Set a key and value in the on-chain database for the given `DbHandle`.
///
/// Returns `Ok` on success.
///
/// ## Permissions
/// * `ContractSection::Deploy`
/// * `ContractSection::Update`
pub fn db_set(db_handle: DbHandle, key: &[u8], value: &[u8]) -> GenericResult<()> {
    db_set_internal(db_handle, key, value, false)
}

/// Set a key and value in the tx-local database for the given `DbHandle`.
///
/// Returns `Ok` on success.
///
/// ## Permissions
/// * `ContractSection::Deploy`
/// * `ContractSection::Update`
pub fn db_set_local(db_handle: DbHandle, key: &[u8], value: &[u8]) -> GenericResult<()> {
    db_set_internal(db_handle, key, value, true)
}

/// Internal function for `db_set` which branches to either on-chain or
/// transaction-local.
fn db_set_internal(
    db_handle: DbHandle,
    key: &[u8],
    value: &[u8],
    local: bool,
) -> GenericResult<()> {
    let mut len = 0;
    let mut buf = vec![];
    len += db_handle.encode(&mut buf)?;
    len += key.encode(&mut buf)?;
    len += value.encode(&mut buf)?;

    let ret = unsafe {
        if local {
            db_set_local_(buf.as_ptr(), len as u32)
        } else {
            db_set_(buf.as_ptr(), len as u32)
        }
    };

    if ret != wasm::entrypoint::SUCCESS {
        return Err(ContractError::from(ret))
    }

    Ok(())
}

/// Remove a key from the on-chain database given a `DbHandle` and `key`.
///
/// Returns `Ok` on success.
///
/// ## Permissions
/// * `ContractSection::Deploy`
/// * `ContractSection::Update`
pub fn db_del(db_handle: DbHandle, key: &[u8]) -> GenericResult<()> {
    db_del_internal(db_handle, key, false)
}

/// Remove a key from the tx-local database given a `DbHandle` and `key`.
///
/// Returns `Ok` on success.
///
/// ## Permissions
/// * `ContractSection::Deploy`
/// * `ContractSection::Update`
pub fn db_del_local(db_handle: DbHandle, key: &[u8]) -> GenericResult<()> {
    db_del_internal(db_handle, key, true)
}

/// Internal function for `db_del` which branches to either on-chain or
/// transaction-local.
fn db_del_internal(db_handle: DbHandle, key: &[u8], local: bool) -> GenericResult<()> {
    let mut len = 0;
    let mut buf = vec![];
    len += db_handle.encode(&mut buf)?;
    len += key.encode(&mut buf)?;

    let ret = unsafe {
        if local {
            db_del_local_(buf.as_ptr(), len as u32)
        } else {
            db_del_(buf.as_ptr(), len as u32)
        }
    };

    if ret != wasm::entrypoint::SUCCESS {
        return Err(ContractError::from(ret))
    }

    Ok(())
}

/// Given a zkas circuit, create a VerifyingKey and insert them both
/// into the on-chain db.
///
/// Returns `Ok` on success, otherwise returns an error code.
///
/// ## Permissions
/// * `ContractSection::Deploy`
pub fn zkas_db_set(bincode: &[u8]) -> GenericResult<()> {
    let mut len = 0;
    let mut buf = vec![];
    len += bincode.encode(&mut buf)?;

    let ret = unsafe { zkas_db_set_(buf.as_ptr(), len as u32) };

    if ret != wasm::entrypoint::SUCCESS {
        return Err(ContractError::from(ret))
    }

    Ok(())
}

extern "C" {
    fn db_init_(ptr: *const u8, len: u32) -> i64;

    fn db_lookup_(ptr: *const u8, len: u32) -> i64;
    fn db_lookup_local_(ptr: *const u8, len: u32) -> i64;

    fn db_get_(ptr: *const u8, len: u32) -> i64;
    fn db_get_local_(ptr: *const u8, len: u32) -> i64;

    fn db_contains_key_(ptr: *const u8, len: u32) -> i64;
    fn db_contains_key_local_(ptr: *const u8, len: u32) -> i64;

    fn db_set_(ptr: *const u8, len: u32) -> i64;
    fn db_set_local_(ptr: *const u8, len: u32) -> i64;

    fn db_del_(ptr: *const u8, len: u32) -> i64;
    fn db_del_local_(ptr: *const u8, len: u32) -> i64;

    fn zkas_db_set_(ptr: *const u8, len: u32) -> i64;
}
