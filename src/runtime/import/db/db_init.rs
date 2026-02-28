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

/// Create a new on-chain database instance for the calling contract.
/// When created, push it to the list of db_handles.
///
/// This function expects to receive a pointer from which a `ContractId`
/// and the `db_name` will be read.
///
/// This function should only be allowed in `ContractSection::Deploy`, as
/// that is called when a contract is being (re)deployed and databases have
/// to be created.
///
/// ## Permissions
/// * `ContractSection::Deploy`
pub(crate) fn db_init(mut ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, ptr_len: u32) -> i64 {
    let (env, mut store) = ctx.data_and_store_mut();
    let cid = env.contract_id;

    // Enforce function ACL
    if let Err(e) = acl_allow(env, &[ContractSection::Deploy]) {
        error!(
            target: "runtime::db::db_init",
            "[WASM] [{cid}] db_init(): Called in unauthorized section: {e}",
        );
        return darkfi_sdk::error::CALLER_ACCESS_DENIED
    }

    // Subtract used gas.
    // TODO: There should probably be an additional fee to open a new sled tree.
    env.subtract_gas(&mut store, 1);

    // Get the wasm memory reader
    let mut buf_reader = match wasm_mem_read(env, &store, ptr, ptr_len) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::db::db_init",
                "[WASM] [{cid}] db_init(): Failed to read WASM memory: {e}",
            );
            return darkfi_sdk::error::DB_INIT_FAILED
        }
    };

    // This takes lock of the blockchain overlay reference in the wasm env
    let contracts = &env.blockchain.lock().unwrap().contracts;

    // Deserialize the Contract ID from the reader
    let read_cid: ContractId = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::db::db_init",
                "[WASM] [{cid}] db_init(): Failed decoding ContractId: {e}",
            );
            return darkfi_sdk::error::DB_INIT_FAILED
        }
    };

    // Deserialize the db name to read from the reader
    let db_name: String = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::db::db_init",
                "[WASM] [{cid}] db_init(): Failed decoding db_name: {e}",
            );
            return darkfi_sdk::error::DB_INIT_FAILED
        }
    };

    // Make sure we've read the entire buffer
    if buf_reader.position() != ptr_len as u64 {
        error!(
            target: "runtime::db::db_init",
            "[WASM] [{cid}] db_init(): Trailing bytes in argument stream",
        );
        return darkfi_sdk::error::DB_INIT_FAILED
    }

    // We cannot allow initializing the special zkas db or monotree db
    if [SMART_CONTRACT_ZKAS_DB_NAME, SMART_CONTRACT_MONOTREE_DB_NAME].contains(&db_name.as_str()) {
        error!(
            target: "runtime::db::db_init",
            "[WASM] [{cid}] db_init(): Attempted to init special db ({db_name})",
        );
        return darkfi_sdk::error::CALLER_ACCESS_DENIED
    }

    // Nor can we allow another contract to initialize a db for someone else:
    if cid != read_cid {
        error!(
            target: "runtime::db::db_init",
            "[WASM] [{cid}] db_init(): Unauthorized ContractId",
        );
        return darkfi_sdk::error::CALLER_ACCESS_DENIED
    }

    // Now try to initialize the tree. If this returns an error,
    // it usually means that this DB was already initialized.
    // An alternative error might happen if something in sled fails,
    // for this we should take care to stop the node or do something to
    // be able to gracefully recover.
    // (src/blockchain/contract_store.rs holds this init() function)
    let tree_handle = match contracts.init(&read_cid, &db_name) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::db::db_init",
                "[WASM] [{cid}] db_init(): Failed to init db: {e}",
            );
            return darkfi_sdk::error::DB_INIT_FAILED
        }
    };

    // Create the DbHandle
    let db_handle = DbHandle::new(read_cid, tree_handle);
    let mut db_handles = env.db_handles.borrow_mut();

    // Make sure we don't duplicate the DbHandle in the vec
    if let Some(index) = db_handles.iter().position(|x| x == &db_handle) {
        return index as i64
    }

    // Push the new DbHandle into the Vec of opened DbHandle
    match db_handles.len().try_into() {
        Ok(db_handle_idx) => {
            db_handles.push(db_handle);
            db_handle_idx
        }
        Err(_) => {
            error!(
                target: "runtime::db::db_init",
                "[WASM] [{cid}] db_init(): Too many open DbHandles",
            );
            darkfi_sdk::error::DB_INIT_FAILED
        }
    }
}
