/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2022 Dyne.org foundation
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

use darkfi_sdk::crypto::ContractId;
use log::error;
use wasmer::{FunctionEnvMut, WasmPtr};

use crate::runtime::vm_runtime::{ContractSection, Env};

/// Internal wasm runtime API for sled trees
pub struct DbHandle {
    contract_id: ContractId,
    tree: sled::Tree,
}

impl DbHandle {
    pub fn new(contract_id: ContractId, tree: sled::Tree) -> Self {
        Self { contract_id, tree }
    }
}

/// Only deploy() can call this. Creates a new database instance for this contract.
///
/// ```
///     type DbHandle = u32;
///     db_init(db_name) -> DbHandle
/// ```
pub(crate) fn db_init(mut ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, len: u32) -> i32 {
    let env = ctx.data();
    match env.contract_section {
        ContractSection::Deploy => {
            let memory_view = env.memory_view(&ctx);
            let db = &env.blockchain.sled_db;
            let contracts = &env.blockchain.contracts;
            let contract_id = &env.contract_id;

            /*
            let Ok(cid_slice) = cid_ptr.slice(&memory_view, 32) else {
                error!(target: "wasm_runtime::db_init", "Failed to read contract id from ptr");
                return -2
            };

            let Ok(cid_bytes) = cid_slice.read_to_vec() else {
                error!(target: "wasm_runtime::db_init", "Failed to read slice to vec in db_init");
                return -2
            };

            // FIXME: Could panic
            let cid = ContractId::from_bytes(cid_bytes.try_into().unwrap());

            if &cid != contract_id {
                error!(target: "wasm_runtime::db_init", "Unauthorized ContractId for db_init");
                return -1
            }
            */

            let Ok(db_name) = ptr.read_utf8_string(&memory_view, len) else {
                error!(target: "wasm_runtime::db_init", "Failed to read string from VM memory");
                return -2
            };

            let tree_handle = match contracts.init(db, contract_id, &db_name) {
                Ok(v) => v,
                Err(e) => {
                    error!(target: "wasm_runtime::db_init", "Failed to init db: {}", e);
                    return -2
                }
            };

            let mut db_handles = env.db_handles.borrow_mut();
            db_handles.push(DbHandle::new(*contract_id, tree_handle));
            return (db_handles.len() - 1) as i32
        }
        _ => {
            error!(target: "wasm_runtime::db_init", "db_init called in unauthorized section");
            return -1
        }
    }
}

/// Everyone can call this. Lookups up a database handle from its name.
///
/// ```
///     type DbHandle = u32;
///     db_lookup(db_name) -> DbHandle
/// ```
pub(crate) fn db_lookup(mut ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, len: u32) -> i32 {
    let env = ctx.data();
    match env.contract_section {
        ContractSection::Deploy | ContractSection::Exec | ContractSection::Update => {
            let env = ctx.data();
            let memory_view = env.memory_view(&ctx);

            match ptr.read_utf8_string(&memory_view, len) {
                Ok(db_name) => {
                    // db_name = blake3_hash(contract_id, db_name)
                    return 110
                }
                Err(_) => {
                    error!(target: "wasm_runtime::drk_log", "Failed to read UTF-8 string from VM memory");
                    return -2
                }
            }
        }
        _ => -1,
    }
}

/// Everyone can call this. Will read a key from the key-value store.
///
/// ```
///     value = db_get(db_handle, key);
/// ```
pub(crate) fn db_get(mut ctx: FunctionEnvMut<Env>) -> i32 {
    let env = ctx.data();
    match env.contract_section {
        ContractSection::Exec => 0,
        _ => -1,
    }
}

/// Only update() can call this. Starts an atomic transaction.
///
/// ```
///     tx_handle = db_begin_tx();
/// ```
pub(crate) fn db_begin_tx(mut ctx: FunctionEnvMut<Env>) -> i32 {
    let env = ctx.data();
    match env.contract_section {
        ContractSection::Deploy | ContractSection::Update => 0,
        _ => -1,
    }
}

/// Only update() can call this. Set a value within the transaction.
///
/// ```
///     db_set(tx_handle, key, value);
/// ```
pub(crate) fn db_set(mut ctx: FunctionEnvMut<Env>) -> i32 {
    let env = ctx.data();
    match env.contract_section {
        ContractSection::Deploy | ContractSection::Update => 0,
        _ => -1,
    }
}

/// Only update() can call this. This writes the atomic tx to the database.
///
/// ```
///     db_end_tx(db_handle, tx_handle);
/// ```
pub(crate) fn db_end_tx(mut ctx: FunctionEnvMut<Env>) -> i32 {
    let env = ctx.data();
    match env.contract_section {
        ContractSection::Deploy | ContractSection::Update => 0,
        _ => -1,
    }
}
