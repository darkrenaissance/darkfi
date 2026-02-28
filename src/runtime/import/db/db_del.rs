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

use darkfi_sdk::wasm;
use darkfi_serial::Decodable;
use tracing::error;
use wasmer::{FunctionEnvMut, WasmPtr};

use crate::runtime::{
    import::acl::acl_allow,
    vm_runtime::{ContractSection, Env},
};

use super::util::wasm_mem_read;

/// Remove a key from the on-chain database.
///
/// Returns `SUCCESS` on success, otherwise returns an error value.
///
/// ## Permissions
/// * `ContractSection::Deploy`
/// * `ContractSection::Update`
pub(crate) fn db_del(ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, ptr_len: u32) -> i64 {
    db_del_internal(ctx, ptr, ptr_len, false)
}

/// Remove a key from the tx-local database.
///
/// Returns `SUCCESS` on success, otherwise returns an error value.
///
/// ## Permissions
/// * `ContractSection::Deploy`
/// * `ContractSection::Update`
pub(crate) fn db_del_local(ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, ptr_len: u32) -> i64 {
    db_del_internal(ctx, ptr, ptr_len, true)
}

/// Internal `db_del` function which branches to either on-chain or tx-local.
///
/// ## Permissions
/// * `ContractSection::Deploy`
/// * `ContractSection::Update`
fn db_del_internal(
    mut ctx: FunctionEnvMut<Env>,
    ptr: WasmPtr<u8>,
    ptr_len: u32,
    local: bool,
) -> i64 {
    let lt = if local { "db_del_local" } else { "db_del" };
    let (env, mut store) = ctx.data_and_store_mut();
    let cid = env.contract_id;

    // Enforce function ACL
    if let Err(e) = acl_allow(env, &[ContractSection::Deploy, ContractSection::Update]) {
        error!(
            target: "runtime::db::{lt}",
            "[WASM] [{cid}] {lt}(): Called in unauthorized section: {e}",
        );
        return darkfi_sdk::error::CALLER_ACCESS_DENIED
    }

    // Subtract used gas.
    // We make deletion free.
    env.subtract_gas(&mut store, 1);

    // Get the WASM memory reader
    let mut buf_reader = match wasm_mem_read(env, &store, ptr, ptr_len) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::db::{lt}",
                "[WASM] [{cid}] {lt}(): Failed to read WASM memory: {e}",
            );
            return darkfi_sdk::error::DB_DEL_FAILED
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
            return darkfi_sdk::error::DB_DEL_FAILED
        }
    };

    let db_handle_index = db_handle_index as usize;

    // If we're in ContractSection::Deploy, the zkas db handle is index 0.
    // We should disallow writing with this.
    if env.contract_section == ContractSection::Deploy && db_handle_index == 0 {
        error!(
            target: "runtime::db::{lt}",
            "[WASM] [{cid}] {lt}(): Tried to write to zkas db",
        );
        return darkfi_sdk::error::CALLER_ACCESS_DENIED
    }

    // Decode key corresponding to the value that will be deleted
    let key: Vec<u8> = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::db::{lt}",
                "[WASM] [{cid}] {lt}(): Failed to decode key Vec: {e}",
            );
            return darkfi_sdk::error::DB_DEL_FAILED
        }
    };

    // Make sure we've read the entire buffer
    if buf_reader.position() != ptr_len as u64 {
        error!(
            target: "runtime::db::{lt}",
            "[WASM] [{cid}] {lt}(): Trailing bytes in argument stream",
        );
        return darkfi_sdk::error::DB_DEL_FAILED
    }

    // Fetch requested db handles
    let db_handles = if local { env.local_db_handles.borrow() } else { env.db_handles.borrow() };

    // Check DbHandle index is within bounds
    if db_handles.len() <= db_handle_index {
        error!(
            target: "runtime::db::{lt}",
            "[WASM] [{cid}] {lt}(): Requested DbHandle out of bounds",
        );
        return darkfi_sdk::error::DB_DEL_FAILED
    }

    // Retrieve DbHandle using the index
    let db_handle = &db_handles[db_handle_index];

    // Validate that the DbHandle matches the contract ID.
    // We're not letting foreign contracts write to others' dbs.
    if db_handle.contract_id != cid {
        error!(
            target: "runtime::db::{lt}",
            "[WASM] [{cid}] {lt}(): Unauthorized write to DbHandle",
        );
        return darkfi_sdk::error::CALLER_ACCESS_DENIED
    }

    // Delete from appropriate db
    if local {
        // Safe to unwrap here.
        let mut db = env.tx_local.lock();
        let db_cid = db.get_mut(&cid).unwrap();
        let Some(tree) = db_cid.get_mut(&db_handle.tree) else {
            error!(
                target: "runtime::db::{lt}",
                "[WASM] [{cid}] {lt}(): Could not remove key from tx-local tree",
            );
            return darkfi_sdk::error::DB_DEL_FAILED
        };

        tree.remove(&key);
    } else if env
        .blockchain
        .lock()
        .unwrap()
        .overlay
        .lock()
        .unwrap()
        .remove(&db_handle.tree, &key)
        .is_err()
    {
        error!(
            target: "runtime::db::{lt}",
            "[WASM] [{cid}] {lt}(): Could not remove key from on-chain tree",
        );
        return darkfi_sdk::error::DB_DEL_FAILED
    }

    wasm::entrypoint::SUCCESS
}
