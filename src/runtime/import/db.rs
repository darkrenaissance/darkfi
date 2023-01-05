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

use std::io::Cursor;

use darkfi_sdk::{
    crypto::ContractId,
    db::{
        CALLER_ACCESS_DENIED, DB_CONTAINS_KEY_FAILED, DB_DEL_FAILED, DB_GET_FAILED, DB_INIT_FAILED,
        DB_LOOKUP_FAILED, DB_SET_FAILED, DB_SUCCESS,
    },
};
use darkfi_serial::Decodable;
use log::{debug, error};
use wasmer::{FunctionEnvMut, WasmPtr};

use crate::{
    runtime::vm_runtime::{ContractSection, Env},
    Result,
};

/// Internal wasm runtime API for sled trees
pub struct DbHandle {
    pub contract_id: ContractId,
    tree: sled::Tree,
}

impl DbHandle {
    pub fn new(contract_id: ContractId, tree: sled::Tree) -> Self {
        Self { contract_id, tree }
    }

    pub fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        if let Some(v) = self.tree.get(key)? {
            return Ok(Some(v.to_vec()))
        };

        Ok(None)
    }

    pub fn contains_key(&self, key: &[u8]) -> Result<bool> {
        Ok(self.tree.contains_key(key)?)
    }

    pub fn apply_batch(&self, batch: sled::Batch) -> Result<()> {
        Ok(self.tree.apply_batch(batch)?)
    }

    pub fn flush(&self) -> Result<()> {
        let _ = self.tree.flush()?;
        Ok(())
    }
}

/// Only deploy() can call this. Creates a new database instance for this contract.
pub(crate) fn db_init(ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, len: u32) -> i32 {
    let env = ctx.data();
    match env.contract_section {
        ContractSection::Deploy => {
            let memory_view = env.memory_view(&ctx);
            let db = &env.blockchain.sled_db;
            let contracts = &env.blockchain.contracts;
            let contract_id = &env.contract_id;

            let Ok(mem_slice) = ptr.slice(&memory_view, len) else {
                error!(target: "runtime::db::db_init()", "Failed to make slice from ptr");
                return DB_INIT_FAILED
            };

            let mut buf = vec![0_u8; len as usize];
            if let Err(e) = mem_slice.read_slice(&mut buf) {
                error!(target: "runtime::db::db_init()", "Failed to read from memory slice: {}", e);
                return DB_INIT_FAILED
            };

            let mut buf_reader = Cursor::new(buf);

            let cid: ContractId = match Decodable::decode(&mut buf_reader) {
                Ok(v) => v,
                Err(e) => {
                    error!(target: "runtime::db::db_init()", "Failed to decode ContractId: {}", e);
                    return DB_INIT_FAILED
                }
            };

            let db_name: String = match Decodable::decode(&mut buf_reader) {
                Ok(v) => v,
                Err(e) => {
                    error!(target: "runtime::db::db_init()", "Failed to decode db_name: {}", e);
                    return DB_INIT_FAILED
                }
            };

            // Disabled until cursor_remaining feature is available on master.
            // Then enable #![feature(cursor_remaining)] in src/lib.rs
            /*if !buf_reader.is_empty() {
                error!(target: "runtime::db::db_init()", "Trailing bytes in argument stream");
                return DB_DEL_FAILED
            }*/

            if &cid != contract_id {
                error!(target: "runtime::db::db_init()", "Unauthorized ContractId for db_init");
                return CALLER_ACCESS_DENIED
            }

            let tree_handle = match contracts.init(db, &cid, &db_name) {
                Ok(v) => v,
                Err(e) => {
                    error!(target: "runtime::db::db_init()", "Failed to init db: {}", e);
                    return DB_INIT_FAILED
                }
            };

            // TODO: Make sure we don't duplicate the DbHandle in the vec.
            //       It should behave like an ordered set.
            // In `lookup()` we also create a `sled::Batch`. This is done for
            // some simplicity reasons, and also for possible future changes.
            // However, we make sure that unauthorized writes are not available
            // from other functions that interface with the databases.
            let mut db_handles = env.db_handles.borrow_mut();
            let mut db_batches = env.db_batches.borrow_mut();
            db_handles.push(DbHandle::new(cid, tree_handle));
            db_batches.push(sled::Batch::default());
            (db_handles.len() - 1) as i32
        }
        _ => {
            error!(target: "runtime::db::db_init()", "db_init called in unauthorized section");
            CALLER_ACCESS_DENIED
        }
    }
}

/// Everyone can call this. Lookups up a database handle from its name.
pub(crate) fn db_lookup(ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, len: u32) -> i32 {
    let env = ctx.data();
    match env.contract_section {
        ContractSection::Deploy |
        ContractSection::Exec |
        ContractSection::Update |
        ContractSection::Metadata => {
            let memory_view = env.memory_view(&ctx);
            let db = &env.blockchain.sled_db;
            let contracts = &env.blockchain.contracts;

            let Ok(mem_slice) = ptr.slice(&memory_view, len) else {
                error!(target: "runtime::db::db_lookup()", "Failed to make slice from ptr");
                return DB_LOOKUP_FAILED
            };

            let mut buf = vec![0_u8; len as usize];
            if let Err(e) = mem_slice.read_slice(&mut buf) {
                error!(target: "runtime::db::db_lookup()", "Failed to read from memory slice: {}", e);
                return DB_LOOKUP_FAILED
            };

            let mut buf_reader = Cursor::new(buf);

            let cid: ContractId = match Decodable::decode(&mut buf_reader) {
                Ok(v) => v,
                Err(e) => {
                    error!(target: "runtime::db::db_lookup()", "Failed to decode ContractId: {}", e);
                    return DB_LOOKUP_FAILED
                }
            };

            let db_name: String = match Decodable::decode(&mut buf_reader) {
                Ok(v) => v,
                Err(e) => {
                    error!(target: "runtime::db::db_lookup()", "Failed to decode db_name: {}", e);
                    return DB_LOOKUP_FAILED
                }
            };

            // Disabled until cursor_remaining feature is available on master.
            // Then enable #![feature(cursor_remaining)] in src/lib.rs
            /*if !buf_reader.is_empty() {
                error!(target: "runtime::db::db_lookup()", "Trailing bytes in argument stream");
                return DB_LOOKUP_FAILED
            }*/

            let tree_handle = match contracts.lookup(db, &cid, &db_name) {
                Ok(v) => v,
                Err(e) => {
                    error!(target: "runtime::db::db_lookup()", "Failed to lookup db: {}", e);
                    return DB_LOOKUP_FAILED
                }
            };

            // TODO: Make sure we don't duplicate the DbHandle in the vec.
            //       It should behave like an ordered set.
            // In `lookup()` we also create a `sled::Batch`. This is done for
            // some simplicity reasons, and also for possible future changes.
            // However, we make sure that unauthorized writes are not available
            // from other functions that interface with the databases.
            let mut db_handles = env.db_handles.borrow_mut();
            let mut db_batches = env.db_batches.borrow_mut();
            db_handles.push(DbHandle::new(cid, tree_handle));
            db_batches.push(sled::Batch::default());
            (db_handles.len() - 1) as i32
        }
        _ => {
            error!(target: "runtime::db::db_lookup()", "db_lookup called in unauthorized section");
            CALLER_ACCESS_DENIED
        }
    }
}

/// Only update() can call this. Set a value within the transaction.
pub(crate) fn db_set(ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, len: u32) -> i32 {
    let env = ctx.data();
    match env.contract_section {
        ContractSection::Deploy | ContractSection::Update => {
            let memory_view = env.memory_view(&ctx);

            let Ok(mem_slice) = ptr.slice(&memory_view, len) else {
                error!(target: "runtime::db::db_set()", "Failed to make slice from ptr");
                return DB_SET_FAILED
            };

            let mut buf = vec![0_u8; len as usize];
            if let Err(e) = mem_slice.read_slice(&mut buf) {
                error!(target: "runtime::db::db_set()", "Failed to read from memory slice: {}", e);
                return DB_SET_FAILED
            };

            let mut buf_reader = Cursor::new(buf);

            // FIXME: There's a type DbHandle=u32, but this should maybe be renamed
            let db_handle: u32 = match Decodable::decode(&mut buf_reader) {
                Ok(v) => v,
                Err(e) => {
                    error!(target: "runtime::db::db_set()", "Failed to decode DbHandle: {}", e);
                    return DB_SET_FAILED
                }
            };
            let db_handle = db_handle as usize;

            let key: Vec<u8> = match Decodable::decode(&mut buf_reader) {
                Ok(v) => v,
                Err(e) => {
                    error!(target: "runtime::db::db_set()", "Failed to decode key vec: {}", e);
                    return DB_SET_FAILED
                }
            };

            let value: Vec<u8> = match Decodable::decode(&mut buf_reader) {
                Ok(v) => v,
                Err(e) => {
                    error!(target: "runtime::db::db_set()", "Failed to decode value vec: {}", e);
                    return DB_SET_FAILED
                }
            };

            // Disabled until cursor_remaining feature is available on master.
            // Then enable #![feature(cursor_remaining)] in src/lib.rs
            /*if !buf_reader.is_empty() {
                error!(target: "runtime::db::db_set()", "Trailing bytes in argument stream");
                return DB_DEL_FAILED
            }*/

            let db_handles = env.db_handles.borrow();
            let mut db_batches = env.db_batches.borrow_mut();

            if db_handles.len() <= db_handle || db_batches.len() <= db_handle {
                error!(target: "runtime::db::db_set()", "Requested DbHandle that is out of bounds");
                return DB_SET_FAILED
            }

            let handle_idx = db_handle;
            let db_handle = &db_handles[handle_idx];
            let db_batch = &mut db_batches[handle_idx];

            if db_handle.contract_id != env.contract_id {
                error!(target: "runtime::db::db_set()", "Unauthorized to write to DbHandle");
                return CALLER_ACCESS_DENIED
            }

            db_batch.insert(key, value);

            DB_SUCCESS
        }
        _ => CALLER_ACCESS_DENIED,
    }
}

/// Only update() can call this. Remove a key from the database.
pub(crate) fn db_del(ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, len: u32) -> i32 {
    let env = ctx.data();
    match env.contract_section {
        ContractSection::Deploy | ContractSection::Update => {
            let memory_view = env.memory_view(&ctx);

            let Ok(mem_slice) = ptr.slice(&memory_view, len) else {
                error!(target: "runtime::db::db_del()", "Failed to make slice from ptr");
                return DB_DEL_FAILED
            };

            let mut buf = vec![0_u8; len as usize];
            if let Err(e) = mem_slice.read_slice(&mut buf) {
                error!(target: "runtime::db::db_del()", "Failed to read from memory slice: {}", e);
                return DB_DEL_FAILED
            };

            let mut buf_reader = Cursor::new(buf);

            // FIXME: There's a type DbHandle=u32, but this should maybe be renamed
            let db_handle: u32 = match Decodable::decode(&mut buf_reader) {
                Ok(v) => v,
                Err(e) => {
                    error!(target: "runtime::db::db_del()", "Failed to decode DbHandle: {}", e);
                    return DB_DEL_FAILED
                }
            };
            let db_handle = db_handle as usize;

            let key: Vec<u8> = match Decodable::decode(&mut buf_reader) {
                Ok(v) => v,
                Err(e) => {
                    error!(target: "runtime::db::db_del()", "Failed to decode key vec: {}", e);
                    return DB_DEL_FAILED
                }
            };

            // Disabled until cursor_remaining feature is available on master.
            // Then enable #![feature(cursor_remaining)] in src/lib.rs
            /*if !buf_reader.is_empty() {
                error!(target: "runtime::db::db_del()", "Trailing bytes in argument stream");
                return DB_DEL_FAILED
            }*/

            let db_handles = env.db_handles.borrow();
            let mut db_batches = env.db_batches.borrow_mut();

            if db_handles.len() <= db_handle || db_batches.len() <= db_handle {
                error!(target: "runtime::db::db_del()", "Requested DbHandle that is out of bounds");
                return DB_DEL_FAILED
            }

            let handle_idx = db_handle;
            let db_handle = &db_handles[handle_idx];
            let db_batch = &mut db_batches[handle_idx];

            if db_handle.contract_id != env.contract_id {
                error!(target: "runtime::db::db_del()", "Unauthorized to write to DbHandle");
                return CALLER_ACCESS_DENIED
            }

            db_batch.remove(key);

            DB_SUCCESS
        }
        _ => CALLER_ACCESS_DENIED,
    }
}

/// Everyone can call this. Will read a key from the key-value store.
pub(crate) fn db_get(ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, len: u32) -> i64 {
    let env = ctx.data();
    match env.contract_section {
        ContractSection::Deploy | ContractSection::Exec | ContractSection::Metadata => {
            let memory_view = env.memory_view(&ctx);

            let Ok(mem_slice) = ptr.slice(&memory_view, len) else {
                error!(target: "runtime::db::db_get()", "Failed to make slice from ptr");
                return DB_GET_FAILED.into()
            };

            let mut buf = vec![0_u8; len as usize];
            if let Err(e) = mem_slice.read_slice(&mut buf) {
                error!(target: "runtime::db::db_get()", "Failed to read from memory slice: {}", e);
                return DB_GET_FAILED.into()
            };

            let mut buf_reader = Cursor::new(buf);

            // FIXME: There's a type DbHandle=u32, but this should maybe be renamed
            let db_handle: u32 = match Decodable::decode(&mut buf_reader) {
                Ok(v) => v,
                Err(e) => {
                    error!(target: "runtime::db::db_get()", "Failed to decode DbHandle: {}", e);
                    return DB_GET_FAILED.into()
                }
            };
            let db_handle = db_handle as usize;

            let key: Vec<u8> = match Decodable::decode(&mut buf_reader) {
                Ok(v) => v,
                Err(e) => {
                    error!(target: "runtime::db::db_get()", "Failed to decode key from vec: {}", e);
                    return DB_GET_FAILED.into()
                }
            };

            // Disabled until cursor_remaining feature is available on master.
            // Then enable #![feature(cursor_remaining)] in src/lib.rs
            /*if !buf_reader.is_empty() {
                error!(target: "runtime::db::db_get()", "Trailing bytes in argument stream");
                return DB_GET_FAILED.into()
            }*/

            let db_handles = env.db_handles.borrow();

            if db_handles.len() <= db_handle {
                error!(target: "runtime::db::db_get()", "Requested DbHandle that is out of bounds");
                return DB_GET_FAILED.into()
            }

            let handle_idx = db_handle;
            let db_handle = &db_handles[handle_idx];

            let ret = match db_handle.get(&key) {
                Ok(v) => v,
                Err(e) => {
                    error!(target: "runtime::db::db_get()", "Internal error getting from tree: {}", e);
                    return DB_GET_FAILED.into()
                }
            };

            let Some(return_data) = ret else {
                debug!(target: "runtime::db::db_get()", "returned empty vec");
                return -127
            };

            // Copy Vec<u8> to the VM
            let mut objects = env.objects.borrow_mut();
            objects.push(return_data);
            (objects.len() - 1) as i64
        }
        _ => CALLER_ACCESS_DENIED.into(),
    }
}

/// Everyone can call this. Will check if a given db contains given key.
pub(crate) fn db_contains_key(ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, len: u32) -> i32 {
    let env = ctx.data();
    match env.contract_section {
        ContractSection::Deploy |
        ContractSection::Exec |
        ContractSection::Update |
        ContractSection::Metadata => {
            let memory_view = env.memory_view(&ctx);

            let Ok(mem_slice) = ptr.slice(&memory_view, len) else {
                error!(target: "runtime::db::db_contains_key()", "Failed to make slice from ptr");
                return DB_CONTAINS_KEY_FAILED
            };

            let mut buf = vec![0_u8; len as usize];
            if let Err(e) = mem_slice.read_slice(&mut buf) {
                error!(target: "runtime::db::db_contains_key()", "Failed to read from memory slice: {}", e);
                return DB_CONTAINS_KEY_FAILED
            };

            let mut buf_reader = Cursor::new(buf);

            // FIXME: There's a type DbHandle=u32, but this should maybe be renamed
            let db_handle: u32 = match Decodable::decode(&mut buf_reader) {
                Ok(v) => v,
                Err(e) => {
                    error!(target: "runtime::db::db_contains_key()", "Failed to decode DbHandle: {}", e);
                    return DB_CONTAINS_KEY_FAILED
                }
            };
            let db_handle = db_handle as usize;

            let key: Vec<u8> = match Decodable::decode(&mut buf_reader) {
                Ok(v) => v,
                Err(e) => {
                    error!(target: "runtime::db::db_contains_key()", "Failed to decode key vec: {}", e);
                    return DB_CONTAINS_KEY_FAILED
                }
            };

            // Disabled until cursor_remaining feature is available on master.
            // Then enable #![feature(cursor_remaining)] in src/lib.rs
            /*if !buf_reader.is_empty() {
                error!(target: "runtime::db::db_contains_key()", "Trailing bytes in argument stream");
                return DB_CONTAINS_KEY_FAILED
            }*/

            let db_handles = env.db_handles.borrow();

            if db_handles.len() <= db_handle {
                error!(target: "runtime::db::db_contains_key()", "Requested DbHandle that is out of bounds");
                return DB_CONTAINS_KEY_FAILED
            }

            let handle_idx = db_handle;
            let db_handle = &db_handles[handle_idx];

            match db_handle.contains_key(&key) {
                Ok(v) => i32::from(v), // <- 0=false, 1=true
                Err(e) => {
                    error!(target: "runtime::db::db_contains_key()", "sled.tree.contains_key failed: {}", e);
                    DB_CONTAINS_KEY_FAILED
                }
            }
        }
        _ => CALLER_ACCESS_DENIED,
    }
}
