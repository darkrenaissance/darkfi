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

use darkfi_serial::Decodable;
use tracing::error;
use wasmer::{FunctionEnvMut, WasmPtr};

use crate::runtime::{
    import::acl::acl_allow,
    vm_runtime::{ContractSection, Env},
};

use super::util::wasm_mem_read;

/// Check if an on-chain database contains a given key.
///
/// Returns `1` if the key is found.
/// Returns `0` if the key is not found and there are no errors.
/// Otherwise, returns an error code.
///
/// ## Permissions
/// * `ContractSection::Deploy`
/// * `ContractSection::Metadata`
/// * `ContractSection::Exec`
pub(crate) fn db_contains_key(ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, ptr_len: u32) -> i64 {
    db_contains_key_internal(ctx, ptr, ptr_len, false)
}

/// Check if a tx-local database contains a given key.
///
/// Returns `1` if the key is found.
/// Returns `0` if the key is not found and there are no errors.
/// Otherwise, returns an error code.
///
/// ## Permissions
/// * `ContractSection::Deploy`
/// * `ContractSection::Metadata`
/// * `ContractSection::Exec`
pub(crate) fn db_contains_key_local(
    ctx: FunctionEnvMut<Env>,
    ptr: WasmPtr<u8>,
    ptr_len: u32,
) -> i64 {
    db_contains_key_internal(ctx, ptr, ptr_len, true)
}

/// Internal `db_contains_key` function which branches to either on-chain or
/// tx-local.
///
/// ## Permissions
/// * `ContractSection::Deploy`
/// * `ContractSection::Metadata`
/// * `ContractSection::Exec`
fn db_contains_key_internal(
    mut ctx: FunctionEnvMut<Env>,
    ptr: WasmPtr<u8>,
    ptr_len: u32,
    local: bool,
) -> i64 {
    let lt = if local { "db_contains_key_local" } else { "db_contains_key" };
    let (env, mut store) = ctx.data_and_store_mut();
    let cid = env.contract_id;

    // Enforce function ACL
    if let Err(e) =
        acl_allow(env, &[ContractSection::Deploy, ContractSection::Metadata, ContractSection::Exec])
    {
        error!(
            target: "runtime::db::{lt}",
            "[WASM] [{cid}] {lt}(): Called in unauthorized section: {e}",
        );
        return darkfi_sdk::error::CALLER_ACCESS_DENIED
    }

    // Subtract used gas.
    // Reading is free.
    env.subtract_gas(&mut store, 1);

    // Get the WASM memory reader
    let mut buf_reader = match wasm_mem_read(env, &store, ptr, ptr_len) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::db::{lt}",
                "[WASM] [{cid}] {lt}(): Failed to read wasm memory: {e}",
            );
            return darkfi_sdk::error::DB_CONTAINS_KEY_FAILED
        }
    };

    // Decode DbHandle index
    let db_handle_index: u32 = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::db::{lt}",
                "[WASM] [{cid}] {lt}(): Failed to decode DbHandle: {e}",
            );
            return darkfi_sdk::error::DB_CONTAINS_KEY_FAILED
        }
    };

    let db_handle_index = db_handle_index as usize;

    // Decode key
    let key: Vec<u8> = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::db::{lt}",
                "[WASM] [{cid}] {lt}(): Failed to decode key vec: {e}",
            );
            return darkfi_sdk::error::DB_CONTAINS_KEY_FAILED
        }
    };

    // Make sure we've read the entire buffer
    if buf_reader.position() != ptr_len as u64 {
        error!(
            target: "runtime::db::{lt}",
            "[WASM] [{cid}] {lt}(): Trailing bytes in argument stream",
        );
        return darkfi_sdk::error::DB_CONTAINS_KEY_FAILED
    }

    // Fetch requested db handles
    let db_handles = if local { env.local_db_handles.borrow() } else { env.db_handles.borrow() };

    // Ensure DbHandle index is within bounds
    if db_handles.len() <= db_handle_index {
        error!(
            target: "runtime::db::{lt}",
            "[WASM] [{cid}] {lt}(): Requested DbHandle out of bounds",
        );
        return darkfi_sdk::error::DB_CONTAINS_KEY_FAILED
    }

    // Retrieve DbHandle using the index
    let db_handle = &db_handles[db_handle_index];

    // Lookup key parameter in the appropriate db
    if local {
        let db = env.tx_local.lock();
        let Some(db_cid) = db.get(&db_handle.contract_id) else {
            error!(
                target: "runtime::db::{lt}",
                "[WASM] [{cid}] {lt}(): Could not find db for {}",
                db_handle.contract_id,
            );
            return darkfi_sdk::error::DB_CONTAINS_KEY_FAILED
        };

        let Some(tree) = db_cid.get(&db_handle.tree) else {
            error!(
                target: "runtime::db::{lt}",
                "[WASM] [{cid}] {lt}(): Could not find db tree for {}",
                db_handle.contract_id,
            );
            return darkfi_sdk::error::DB_CONTAINS_KEY_FAILED
        };

        // 0=false, 1=true. Convert bool to i64.
        return i64::from(tree.contains_key(&key))
    }

    // On-chain db
    match env.blockchain.lock().unwrap().overlay.lock().unwrap().contains_key(&db_handle.tree, &key)
    {
        Ok(v) => i64::from(v), // <- 0=false, 1=true. Convert bool to i64.
        Err(e) => {
            error!(
                target: "runtime::db::{lt}",
                "[WASM] [{cid}] {lt}(): sled.tree.contains_key failed: {e}",
            );
            darkfi_sdk::error::DB_CONTAINS_KEY_FAILED
        }
    }
}
