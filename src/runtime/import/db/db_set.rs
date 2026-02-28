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

/// Set a value in the on-chain database for the given DbHandle.
///
/// * `ptr` must contain the DbHandle index and the key-value pair.
/// * The DbHandle must match the ContractId.
///
/// Returns `SUCCESS` on success, otherwise returns an error value.
///
/// ## Permissions
/// * `ContractSection::Deploy`
/// * `ContractSection::Update`
pub(crate) fn db_set(ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, ptr_len: u32) -> i64 {
    db_set_internal(ctx, ptr, ptr_len, false)
}

/// Set a value in the tx-local database for the given DbHandle.
///
/// * `ptr` must contain the DbHandle index and the key-value pair.
/// * The DbHandle must match the ContractId.
///
/// Returns `SUCCESS` on success, otherwise returns an error value.
///
/// ## Permissions
/// * `ContractSection::Deploy`
/// * `ContractSection::Update`
pub(crate) fn db_set_local(ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, ptr_len: u32) -> i64 {
    db_set_internal(ctx, ptr, ptr_len, true)
}

/// Internal `db_set` function which branches to either on-chain or tx-local.
///
/// ## Permissions
/// * `ContractSection::Deploy`
/// * `ContractSection::Update`
fn db_set_internal(
    mut ctx: FunctionEnvMut<Env>,
    ptr: WasmPtr<u8>,
    ptr_len: u32,
    local: bool,
) -> i64 {
    let lt = if local { "db_set_local" } else { "db_set" };
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

    // Subtract used gas. Here we count the bytes written into the database.
    // TODO: We might want to count only the difference in size if we're replacing
    // data and the new data is larger.
    env.subtract_gas(&mut store, ptr_len as u64);

    // Get the wasm memory reader
    let mut buf_reader = match wasm_mem_read(env, &store, ptr, ptr_len) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::db::{lt}",
                "[WASM] [{cid}] {lt}(): Failed to read wasm memory: {e}",
            );
            return darkfi_sdk::error::DB_SET_FAILED
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
            return darkfi_sdk::error::DB_SET_FAILED
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

    // Decode key and value
    let key: Vec<u8> = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::db::{lt}",
                "[WASM] [{cid}] {lt}(): Failed to decode key vec: {e}",
            );
            return darkfi_sdk::error::DB_SET_FAILED
        }
    };

    let value: Vec<u8> = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::db::{lt}",
                "[WASM] [{cid}] {lt}(): Failed to decode value vec: {e}",
            );
            return darkfi_sdk::error::DB_SET_FAILED
        }
    };

    // Make sure we've read the entire buffer
    if buf_reader.position() != ptr_len as u64 {
        error!(
            target: "runtime::db::{lt}",
            "[WASM] [{cid}] {lt}(): Trailing bytes in argument stream",
        );
        return darkfi_sdk::error::DB_SET_FAILED
    }

    // Fetch requested db handles
    let db_handles = if local { env.local_db_handles.borrow() } else { env.db_handles.borrow() };

    // Check DbHandle index is within bounds
    if db_handles.len() <= db_handle_index {
        error!(
            target: "runtime::db::{lt}",
            "[WASM] [{cid}] {lt}(): Requested DbHandle that is out of bounds",
        );
        return darkfi_sdk::error::DB_SET_FAILED
    }

    // Retrive DbHandle using the index
    let db_handle = &db_handles[db_handle_index];

    // Validate that the DbHandle matches the contract ID
    if db_handle.contract_id != env.contract_id {
        error!(
            target: "runtime::db::{lt}",
            "[WASM] [{cid}] {lt}(): Unauthorized to write to DbHandle",
        );
        return darkfi_sdk::error::CALLER_ACCESS_DENIED
    }

    // Insert key-value pair into the database corresponding to this contract
    if local {
        // Safe to unwrap here.
        let mut db = env.tx_local.lock();
        let db_cid = db.get_mut(&cid).unwrap();
        let Some(tree) = db_cid.get_mut(&db_handle.tree) else {
            error!(
                target: "runtime::db::{lt}",
                "[WASM] [{cid}] {lt}(): Could not insert to tx-local tree",
            );
            return darkfi_sdk::error::DB_SET_FAILED
        };

        tree.insert(key, value);
    } else if env
        .blockchain
        .lock()
        .unwrap()
        .overlay
        .lock()
        .unwrap()
        .insert(&db_handle.tree, &key, &value)
        .is_err()
    {
        error!(
            target: "runtime::db::{lt}",
            "[WASM] [{cid}] {lt}(): Couldn't insert to on-chain tree",
        );
        return darkfi_sdk::error::DB_SET_FAILED
    }

    wasm::entrypoint::SUCCESS
}
