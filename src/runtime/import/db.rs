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
use darkfi_serial::{deserialize, serialize, Decodable};
use log::{debug, error, info};
use wasmer::{FunctionEnvMut, WasmPtr};

use crate::{
    blockchain::contract_store::SMART_CONTRACT_ZKAS_DB_NAME,
    runtime::vm_runtime::{ContractSection, Env},
    zk::{empty_witnesses, VerifyingKey, ZkCircuit},
    zkas::ZkBinary,
};

/// Internal wasm runtime API for sled trees
pub struct DbHandle {
    pub contract_id: ContractId,
    pub tree: [u8; 32],
}

impl DbHandle {
    pub fn new(contract_id: ContractId, tree: [u8; 32]) -> Self {
        Self { contract_id, tree }
    }
}

/// Only deploy() can call this. Creates a new database instance for this contract.
pub(crate) fn db_init(ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, len: u32) -> i32 {
    let env = ctx.data();

    // Exit as soon as possible
    if env.contract_section != ContractSection::Deploy {
        error!(target: "runtime::db::db_init()", "db_init called in unauthorized section");
        return CALLER_ACCESS_DENIED
    }

    let memory_view = env.memory_view(&ctx);
    let contracts = &env.blockchain.lock().unwrap().contracts;
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

    // TODO: Disabled until cursor_remaining feature is available on master.
    // Then enable #![feature(cursor_remaining)] in src/lib.rs
    /*if !buf_reader.is_empty() {
        error!(target: "runtime::db::db_init()", "Trailing bytes in argument stream");
        return DB_DEL_FAILED
    }*/

    if db_name == SMART_CONTRACT_ZKAS_DB_NAME {
        error!(target: "runtime::db::db_init()", "Attempted to lookup zkas db");
        return CALLER_ACCESS_DENIED
    }

    if &cid != contract_id {
        error!(target: "runtime::db::db_init()", "Unauthorized ContractId for db_init");
        return CALLER_ACCESS_DENIED
    }

    let tree_handle = match contracts.init(&cid, &db_name) {
        Ok(v) => v,
        Err(e) => {
            error!(target: "runtime::db::db_init()", "Failed to init db: {}", e);
            return DB_INIT_FAILED
        }
    };

    // TODO: Make sure we don't duplicate the DbHandle in the vec.
    //       It should behave like an ordered set.
    let mut db_handles = env.db_handles.borrow_mut();
    db_handles.push(DbHandle::new(cid, tree_handle));
    (db_handles.len() - 1) as i32
}

/// Everyone can call this. Lookups up a database handle from its name.
pub(crate) fn db_lookup(ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, len: u32) -> i32 {
    let env = ctx.data();

    match env.contract_section {
        ContractSection::Deploy |
        ContractSection::Exec |
        ContractSection::Update |
        ContractSection::Metadata => {
            // pass
        }

        _ => {
            error!(target: "runtime::db::db_lookup()", "db_lookup called in unauthorized section");
            return CALLER_ACCESS_DENIED
        }
    }

    let memory_view = env.memory_view(&ctx);
    let contracts = &env.blockchain.lock().unwrap().contracts;

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

    if db_name == SMART_CONTRACT_ZKAS_DB_NAME {
        error!(target: "runtime::db::db_lookup()", "Attempted to lookup zkas db");
        return CALLER_ACCESS_DENIED
    }

    // TODO: Disabled until cursor_remaining feature is available on master.
    // Then enable #![feature(cursor_remaining)] in src/lib.rs
    /*if !buf_reader.is_empty() {
        error!(target: "runtime::db::db_lookup()", "Trailing bytes in argument stream");
        return DB_LOOKUP_FAILED
    }*/

    let tree_handle = match contracts.lookup(&cid, &db_name) {
        Ok(v) => v,
        Err(_) => return DB_LOOKUP_FAILED,
    };

    // TODO: Make sure we don't duplicate the DbHandle in the vec.
    //       It should behave like an ordered set.
    let mut db_handles = env.db_handles.borrow_mut();
    db_handles.push(DbHandle::new(cid, tree_handle));
    (db_handles.len() - 1) as i32
}

/// Set a value within the transaction.
pub(crate) fn db_set(ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, len: u32) -> i32 {
    let env = ctx.data();

    if env.contract_section != ContractSection::Deploy &&
        env.contract_section != ContractSection::Update
    {
        error!(target: "runtime::db::db_set()", "db_set called in unauthorized section");
        return CALLER_ACCESS_DENIED
    }

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

    let db_handle_index: u32 = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(target: "runtime::db::db_set()", "Failed to decode DbHandle: {}", e);
            return DB_SET_FAILED
        }
    };
    let db_handle_index = db_handle_index as usize;

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
    // TODO: Then enable #![feature(cursor_remaining)] in src/lib.rs
    /*if !buf_reader.is_empty() {
        error!(target: "runtime::db::db_set()", "Trailing bytes in argument stream");
        return DB_DEL_FAILED
    }*/

    let db_handles = env.db_handles.borrow();

    if db_handles.len() <= db_handle_index {
        error!(target: "runtime::db::db_set()", "Requested DbHandle that is out of bounds");
        return DB_SET_FAILED
    }

    let db_handle = &db_handles[db_handle_index];

    if db_handle.contract_id != env.contract_id {
        error!(target: "runtime::db::db_set()", "Unauthorized to write to DbHandle");
        return CALLER_ACCESS_DENIED
    }

    if env
        .blockchain
        .lock()
        .unwrap()
        .overlay
        .lock()
        .unwrap()
        .insert(&db_handle.tree, &key, &value)
        .is_err()
    {
        error!(target: "runtime::db::db_set()", "Couldn't insert to db_handle tree");
        return DB_SET_FAILED
    }

    DB_SUCCESS
}

/// Remove a key from the database.
pub(crate) fn db_del(ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, len: u32) -> i32 {
    let env = ctx.data();

    if env.contract_section != ContractSection::Deploy &&
        env.contract_section != ContractSection::Update
    {
        error!(target: "runtime::db::db_del()", "db_del called in unauthorized section");
        return CALLER_ACCESS_DENIED
    }

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

    let db_handle_index: u32 = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(target: "runtime::db::db_del()", "Failed to decode DbHandle: {}", e);
            return DB_DEL_FAILED
        }
    };
    let db_handle_index = db_handle_index as usize;

    let key: Vec<u8> = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(target: "runtime::db::db_del()", "Failed to decode key vec: {}", e);
            return DB_DEL_FAILED
        }
    };

    // TODO: Disabled until cursor_remaining feature is available on master.
    // Then enable #![feature(cursor_remaining)] in src/lib.rs
    /*if !buf_reader.is_empty() {
        error!(target: "runtime::db::db_del()", "Trailing bytes in argument stream");
        return DB_DEL_FAILED
    }*/

    let db_handles = env.db_handles.borrow();

    if db_handles.len() <= db_handle_index {
        error!(target: "runtime::db::db_del()", "Requested DbHandle that is out of bounds");
        return DB_DEL_FAILED
    }

    let db_handle = &db_handles[db_handle_index];

    if db_handle.contract_id != env.contract_id {
        error!(target: "runtime::db::db_del()", "Unauthorized to write to DbHandle");
        return CALLER_ACCESS_DENIED
    }

    if env.blockchain.lock().unwrap().overlay.lock().unwrap().remove(&db_handle.tree, &key).is_err()
    {
        error!(target: "runtime::db::db_del()", "Couldn't remove key from db_handle tree");
        return DB_DEL_FAILED
    }

    DB_SUCCESS
}

/// Will read a key from the key-value store.
pub(crate) fn db_get(ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, len: u32) -> i64 {
    let env = ctx.data();

    if env.contract_section != ContractSection::Deploy &&
        env.contract_section != ContractSection::Exec &&
        env.contract_section != ContractSection::Metadata
    {
        error!(target: "runtime::db::db_get()", "db_get called in unauthorized section");
        return CALLER_ACCESS_DENIED.into()
    }

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

    let db_handle_index: u32 = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(target: "runtime::db::db_get()", "Failed to decode DbHandle: {}", e);
            return DB_GET_FAILED.into()
        }
    };
    let db_handle_index = db_handle_index as usize;

    let key: Vec<u8> = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(target: "runtime::db::db_get()", "Failed to decode key from vec: {}", e);
            return DB_GET_FAILED.into()
        }
    };

    // TODO: Disabled until cursor_remaining feature is available on master.
    // Then enable #![feature(cursor_remaining)] in src/lib.rs
    /*if !buf_reader.is_empty() {
        error!(target: "runtime::db::db_get()", "Trailing bytes in argument stream");
        return DB_GET_FAILED.into()
    }*/

    let db_handles = env.db_handles.borrow();

    if db_handles.len() <= db_handle_index {
        error!(target: "runtime::db::db_get()", "Requested DbHandle that is out of bounds");
        return DB_GET_FAILED.into()
    }

    let db_handle = &db_handles[db_handle_index];

    let ret =
        match env.blockchain.lock().unwrap().overlay.lock().unwrap().get(&db_handle.tree, &key) {
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
    objects.push(return_data.to_vec());
    (objects.len() - 1) as i64
}

/// Everyone can call this. Will check if a given db contains given key.
pub(crate) fn db_contains_key(ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, len: u32) -> i32 {
    let env = ctx.data();

    if env.contract_section != ContractSection::Deploy &&
        env.contract_section != ContractSection::Exec &&
        env.contract_section != ContractSection::Update &&
        env.contract_section != ContractSection::Metadata
    {
        error!(target: "runtime::db::db_contains_key()", "db_contains_key called in unauthorized section");
        return CALLER_ACCESS_DENIED
    }

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

    let db_handle_index: u32 = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(target: "runtime::db::db_contains_key()", "Failed to decode DbHandle: {}", e);
            return DB_CONTAINS_KEY_FAILED
        }
    };
    let db_handle_index = db_handle_index as usize;

    let key: Vec<u8> = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(target: "runtime::db::db_contains_key()", "Failed to decode key vec: {}", e);
            return DB_CONTAINS_KEY_FAILED
        }
    };

    // TODO: Disabled until cursor_remaining feature is available on master.
    // Then enable #![feature(cursor_remaining)] in src/lib.rs
    /*if !buf_reader.is_empty() {
        error!(target: "runtime::db::db_contains_key()", "Trailing bytes in argument stream");
        return DB_CONTAINS_KEY_FAILED
    }*/

    let db_handles = env.db_handles.borrow();

    if db_handles.len() <= db_handle_index {
        error!(target: "runtime::db::db_contains_key()", "Requested DbHandle that is out of bounds");
        return DB_CONTAINS_KEY_FAILED
    }

    let db_handle = &db_handles[db_handle_index];

    match env.blockchain.lock().unwrap().overlay.lock().unwrap().contains_key(&db_handle.tree, &key)
    {
        Ok(v) => i32::from(v), // <- 0=false, 1=true
        Err(e) => {
            error!(target: "runtime::db::db_contains_key()", "sled.tree.contains_key failed: {}", e);
            DB_CONTAINS_KEY_FAILED
        }
    }
}

/// Only `deploy()` can call this. Given a zkas circuit, create a VerifyingKey and insert
/// them both into the db.
pub(crate) fn zkas_db_set(ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, len: u32) -> i32 {
    let env = ctx.data();

    if env.contract_section != ContractSection::Deploy {
        error!(target: "runtime::db::zkas_db_set()", "zkas_db_set called in unauthorized section");
        return CALLER_ACCESS_DENIED
    }

    let memory_view = env.memory_view(&ctx);
    let contract_id = &env.contract_id;

    let Ok(mem_slice) = ptr.slice(&memory_view, len) else {
        error!(target: "runtime::db::zkas_db_set()", "Failed to make slice from ptr");
        return DB_SET_FAILED
    };

    let mut buf = vec![0u8; len as usize];
    if let Err(e) = mem_slice.read_slice(&mut buf) {
        error!(target: "runtime::db::zkas_db_set()", "Failed to read from memory slice: {}", e);
        return DB_SET_FAILED
    };

    let mut buf_reader = Cursor::new(buf);

    let zkas_bincode: Vec<u8> = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(target: "runtime::db::zkas_db_set()", "Failed to decode zkas bincode bytes: {}", e);
            return DB_SET_FAILED
        }
    };

    // Make sure that we're actually working on legitimate bincode.
    let Ok(zkbin) = ZkBinary::decode(&zkas_bincode) else {
        error!(target: "runtime::db::zkas_db_set()", "Invalid zkas bincode passed to function");
        return DB_SET_FAILED
    };

    // Because of `Runtime::Deploy`, we should be sure that the zkas db is index zero.
    let db_handles = env.db_handles.borrow();
    let db_handle = &db_handles[0];
    // Redundant check
    if &db_handle.contract_id != contract_id {
        error!(target: "runtime::db::zkas_db_set()", "Internal error, zkas db at index 0 incorrect");
        return DB_SET_FAILED
    }

    // Check if there is existing bincode and compare it. Return DB_SUCCESS if
    // they're the same. The assumption should be that VerifyingKey was generated
    // already so we can skip things after this guard.
    match env
        .blockchain
        .lock()
        .unwrap()
        .overlay
        .lock()
        .unwrap()
        .get(&db_handle.tree, &serialize(&zkbin.namespace))
    {
        Ok(v) => {
            if let Some(bytes) = v {
                // We allow a panic here because this db should never be corrupted in this way.
                let (existing_zkbin, _): (Vec<u8>, Vec<u8>) =
                    deserialize(&bytes).expect("deserialize tuple");

                if existing_zkbin == zkas_bincode {
                    debug!(target: "runtime::db::zkas_db_set()", "Existing zkas bincode is the same. Skipping.");
                    return DB_SUCCESS
                }
            }
        }
        Err(e) => {
            error!(target: "runtime::db::zkas_db_set()", "Internal error getting from tree: {}", e);
            return DB_SET_FAILED
        }
    };

    // We didn't find any existing bincode, so let's create a new VerifyingKey and write it all.
    info!(target: "runtime::db::zkas_db_set()", "Creating VerifyingKey for {} zkas circuit", zkbin.namespace);
    let witnesses = match empty_witnesses(&zkbin) {
        Ok(w) => w,
        Err(e) => {
            error!(target: "runtime::db::zkas_db_set()", "Failed to create empty witnesses: {}", e);
            return DB_SET_FAILED
        }
    };

    // Construct the circuit and build the VerifyingKey
    let circuit = ZkCircuit::new(witnesses, &zkbin);
    let vk = VerifyingKey::build(zkbin.k, &circuit);
    let mut vk_buf = vec![];
    if let Err(e) = vk.write(&mut vk_buf) {
        error!(target: "runtime::db::zkas_db_set()", "Failed to serialize VerifyingKey: {}", e);
        return DB_SET_FAILED
    }

    let key = serialize(&zkbin.namespace);
    let value = serialize(&(zkas_bincode, vk_buf));
    if env
        .blockchain
        .lock()
        .unwrap()
        .overlay
        .lock()
        .unwrap()
        .insert(&db_handle.tree, &key, &value)
        .is_err()
    {
        error!(target: "runtime::db::zkas_db_set()", "Couldn't insert to db_handle tree");
        return DB_SET_FAILED
    }

    DB_SUCCESS
}
