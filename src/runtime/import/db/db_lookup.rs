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

use darkfi_sdk::crypto::contract_id::{
    ContractId, SMART_CONTRACT_MONOTREE_DB_NAME, SMART_CONTRACT_ZKAS_DB_NAME,
};
use darkfi_serial::Decodable;
use tracing::error;
use wasmer::{FunctionEnvMut, WasmPtr};

use crate::runtime::{
    import::acl::acl_allow,
    vm_runtime::{ContractSection, Env},
};

use super::{util::wasm_mem_read, DbHandle};

/// Lookup an on-chain database handle from its name.
/// If it exists, push it to the list of db_handles.
///
/// Returns the index of the DbHandle in the db_handles Vector on success.
/// Otherwise, returns an error value.
///
/// This function can be called from any [`ContractSection`].
///
/// ## Permissions
/// * `ContractSection::Deploy`
/// * `ContractSection::Metadata`
/// * `ContractSection::Exec`
/// * `ContractSection::Update`
pub(crate) fn db_lookup(mut ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, ptr_len: u32) -> i64 {
    let (env, mut store) = ctx.data_and_store_mut();
    let cid = env.contract_id;

    // Enforce function ACL
    if let Err(e) = acl_allow(
        env,
        &[
            ContractSection::Deploy,
            ContractSection::Metadata,
            ContractSection::Exec,
            ContractSection::Update,
        ],
    ) {
        error!(
            target: "runtime::db::db_lookup",
            "[WASM] [{cid}] db_lookup() called in unauthorized section: {e}",
        );
        return darkfi_sdk::error::CALLER_ACCESS_DENIED
    }

    // Subtract used gas. Opening an existing db should be free (i.e. 1 gas unit).
    env.subtract_gas(&mut store, 1);

    // Get the wasm memory reader
    let mut buf_reader = match wasm_mem_read(env, &store, ptr, ptr_len) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::db::db_lookup",
                "[WASM] [{cid}] db_lookup(): Failed to read WASM memory: {e}",
            );
            return darkfi_sdk::error::DB_LOOKUP_FAILED
        }
    };

    // This takes lock of the blockchain overlay reference in the wasm env
    let contracts = &env.blockchain.lock().unwrap().contracts;

    // Decode ContractId from memory
    let cid: ContractId = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::db::db_lookup",
                "[WASM] [{cid}] db_lookup(): Failed to decode ContractId: {e}",
            );
            return darkfi_sdk::error::DB_LOOKUP_FAILED
        }
    };

    // Decode DB name from memory
    let db_name: String = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::db::db_lookup",
                "[WASM] [{cid}] db_lookup(): Failed to decode db_name: {e}",
            );
            return darkfi_sdk::error::DB_LOOKUP_FAILED
        }
    };

    // Make sure we've read the entire buffer
    if buf_reader.position() != ptr_len as u64 {
        error!(
            target: "runtime::db::db_lookup",
            "[WASM] [{cid}] db_lookup(), Trailing bytes in argument stream",
        );
        return darkfi_sdk::error::DB_LOOKUP_FAILED
    }

    // We won't allow reading from the special zkas db or monotree db
    if [SMART_CONTRACT_ZKAS_DB_NAME, SMART_CONTRACT_MONOTREE_DB_NAME].contains(&db_name.as_str()) {
        error!(
            target: "runtime::db::db_lookup",
            "[WASM] [{cid}] db_lookup(): Attempted to lookup special db ({db_name})"
        );
        return darkfi_sdk::error::CALLER_ACCESS_DENIED
    }

    // Lookup contract state
    let tree_handle = match contracts.lookup(&cid, &db_name) {
        Ok(v) => v,
        Err(_) => return darkfi_sdk::error::DB_LOOKUP_FAILED,
    };

    // Create the DbHandle
    let db_handle = DbHandle::new(cid, tree_handle);
    let mut db_handles = env.db_handles.borrow_mut();

    // Make sure we don't duplicate the DbHandle in the vec
    if let Some(index) = db_handles.iter().position(|x| x == &db_handle) {
        return index as i64
    }

    // Push the new DbHandle to the Vec of opened DbHandles
    match db_handles.len().try_into() {
        Ok(db_handle_idx) => {
            db_handles.push(db_handle);
            db_handle_idx
        }
        Err(_) => {
            error!(
                target: "runtime::db::db_lookup",
                "[WASM] [{cid}] db_lookup(): Too many open DbHandles",
            );
            darkfi_sdk::error::DB_LOOKUP_FAILED
        }
    }
}
