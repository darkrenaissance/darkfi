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

use super::acl::acl_allow;
use crate::{
    blockchain::contract_store::SMART_CONTRACT_ZKAS_DB_NAME,
    runtime::vm_runtime::{ContractSection, Env},
    zk::{empty_witnesses, VerifyingKey, ZkCircuit},
    zkas::ZkBinary,
};

/// Internal wasm runtime API for sled trees
#[derive(PartialEq)]
pub struct DbHandle {
    pub contract_id: ContractId,
    pub tree: [u8; 32],
}

impl DbHandle {
    pub fn new(contract_id: ContractId, tree: [u8; 32]) -> Self {
        Self { contract_id, tree }
    }
}

/// Create a new database instance for the calling contract.
///
/// This function expects to receive a pointer from which a `ContractId`
/// and the `db_name` will be read.
///
/// This function should **only** be allowed in `ContractSection::Deploy`, as that
/// is called when a contract is being (re)deployed and databases have to be created.
pub(crate) fn db_init(ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, ptr_len: u32) -> i64 {
    let env = ctx.data();
    let cid = &env.contract_id;

    // Enforce function ACL
    if let Err(e) = acl_allow(env, &[ContractSection::Deploy]) {
        error!(target: "runtime::db::db_init", "[wasm-runtime] [Contract:{}] db_init ACL denied: {}", cid, e);
        // TODO: FIXME: We have to fix up the errors used within runtime and the sdk
        return CALLER_ACCESS_DENIED
    }

    // Enforce the ptr_len is no more than 64 bytes.
    if ptr_len > 64 {
        error!(target: "runtime::db::db_init", "[wasm-runtime] [Contract:{}] db_init ptr len is >64", cid);
        return DB_INIT_FAILED
    }

    // This takes lock of the blockchain overlay reference in the wasm env
    let contracts = &env.blockchain.lock().unwrap().contracts;

    // Create a mem slice of the wasm VM memory
    let memory_view = env.memory_view(&ctx);
    let Ok(mem_slice) = ptr.slice(&memory_view, ptr_len) else {
        error!(target: "runtime::db::db_init", "[wasm-runtime] [Contract:{}] Failed to make slice from ptr", cid);
        return DB_INIT_FAILED
    };

    // Allocate a buffer and copy all the data from the pointer into the buffer
    let mut buf = vec![0_u8; ptr_len as usize];
    if let Err(e) = mem_slice.read_slice(&mut buf) {
        error!(target: "runtime::db::db_init", "[wasm-runtime] [Contract:{}] Failed to read memory slice: {}", cid, e);
        return DB_INIT_FAILED
    };

    // Once the data is copied, we'll attempt to deserialize it into the objects
    // we're expecting.
    let mut buf_reader = Cursor::new(buf);

    let read_cid: ContractId = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(target: "runtime::db::db_init", "[wasm-runtime] [Contract:{}] Failed decoding ContractId: {}", cid, e);
            return DB_INIT_FAILED
        }
    };

    let read_db_name: String = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(target: "runtime::db::db_init", "[wasm-runtime] [Contract:{}] Failed decoding db_name: {}", cid, e);
            return DB_INIT_FAILED
        }
    };

    // Make sure we've read the entire buffer
    if buf_reader.position() != ptr_len as u64 {
        error!(target: "runtime::db::db_init", "[wasm-runtime] [Contract:{}] Trailing bytes in argument stream", cid);
        return DB_INIT_FAILED
    }

    // We cannot allow initializing the special zkas db:
    if read_db_name == SMART_CONTRACT_ZKAS_DB_NAME {
        error!(target: "runtime::db::db_init", "[wasm-runtime] [Contract:{}] Attempted to init zkas db", cid);
        return CALLER_ACCESS_DENIED
    }

    // Nor can we allow another contract to initialize a db for someone else:
    if cid != &read_cid {
        error!(target: "runtime::db::db_init", "[wasm-runtime] [Contract:{}] Unauthorized ContractId for db_init", cid);
        return CALLER_ACCESS_DENIED
    }

    // Now try to initialize the tree. If this returns an error,
    // it usually means that this DB was already initialized.
    // An alternative error might happen if something in sled fails,
    // for this we should take care to stop the node or do something to
    // be able to gracefully recover.
    // (src/blockchain/contract_store.rs holds this init() function)
    let tree_handle = match contracts.init(&read_cid, &read_db_name) {
        Ok(v) => v,
        Err(e) => {
            error!(target: "runtime::db::db_init", "[wasm-runtime] [Contract:{}] Failed to init db: {}", cid, e);
            return DB_INIT_FAILED
        }
    };

    // Create the DbHandle
    let db_handle = DbHandle::new(read_cid, tree_handle);
    let mut db_handles = env.db_handles.borrow_mut();

    // Make sure we don't duplicate the DbHandle in the vec.
    // It's not really an issue, but it's better to be pedantic.
    if db_handles.contains(&db_handle) {
        error!(target: "runtime::db::db_init", "[wasm-runtime] [Contract:{}] DbHandle initialized twice during execution", cid);
        return DB_INIT_FAILED
    }

    match db_handles.len().try_into() {
        Ok(db_handle_idx) => {
            db_handles.push(db_handle);
            db_handle_idx
        }
        Err(_) => {
            error!(target: "runtime::db::db_init", "[wasm-runtime] [Contract:{}] Too many open DbHandles", cid);
            DB_INIT_FAILED
        }
    }
}

/// Lookup a database handle from its name. If it does not exist, push it to the Vector of
/// db_handles.
/// Returns the index of the DbHandle in the db_handles Vector on success. Otherwise, returns
/// a negative error value.
/// This function can be called from any [`ContractSection`].
pub(crate) fn db_lookup(ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, ptr_len: u32) -> i64 {
    let env = ctx.data();
    let cid = &env.contract_id;

    // Enforce function ACL
    if let Err(e) = acl_allow(
        env,
        &[
            ContractSection::Deploy,
            ContractSection::Exec,
            ContractSection::Metadata,
            ContractSection::Update,
        ],
    ) {
        error!(target: "runtime::db::db_lookup", "[wasm-runtime] [Contract:{}] db_lookup ACL denied: {}", cid, e);
        // TODO: FIXME: We have to fix up the errors used within runtime and the sdk
        return CALLER_ACCESS_DENIED
    }

    // Enforce the ptr_len is no more than 64 bytes.
    if ptr_len > 64 {
        error!(target: "runtime::db::db_lookup", "[wasm-runtime] db_lookup ptr len is >64");
        return DB_LOOKUP_FAILED
    }

    // Read memory location that contains the ContractId and DB name
    let memory_view = env.memory_view(&ctx);
    let contracts = &env.blockchain.lock().unwrap().contracts;

    let Ok(mem_slice) = ptr.slice(&memory_view, ptr_len) else {
        error!(target: "runtime::db::db_lookup", "[wasm-runtime] [Contract:{}] Failed to make slice from ptr.", cid);
        return DB_LOOKUP_FAILED
    };

    let mut buf = vec![0_u8; ptr_len as usize];
    if let Err(e) = mem_slice.read_slice(&mut buf) {
        error!(target: "runtime::db::db_lookup", "[wasm-runtime] [Contract:{}] Failed to read from memory slice: {}", cid, e);
        return DB_LOOKUP_FAILED
    };

    let mut buf_reader = Cursor::new(buf);

    // Decode ContractId from memory
    let cid: ContractId = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(target: "runtime::db::db_lookup", "[wasm-runtime] [Contract:{}] Failed to decode ContractId: {}", cid, e);
            return DB_LOOKUP_FAILED
        }
    };

    // Decode DB name from memory
    let db_name: String = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(target: "runtime::db::db_lookup", "[wasm-runtime] [Contract:{}] Failed to decode db_name: {}", cid, e);
            return DB_LOOKUP_FAILED
        }
    };

    // Make sure we've read the entire buffer
    if buf_reader.position() != ptr_len as u64 {
        error!(target: "runtime::db::db_lookup", "[wasm-runtime] Trailing bytes in argument stream");
        return DB_LOOKUP_FAILED
    }

    if db_name == SMART_CONTRACT_ZKAS_DB_NAME {
        error!(target: "runtime::db::db_lookup", "[wasm-runtime] [Contract:{}] Attempted to lookup zkas db", cid);
        return CALLER_ACCESS_DENIED
    }

    // Lookup contract state
    let tree_handle = match contracts.lookup(&cid, &db_name) {
        Ok(v) => v,
        Err(_) => return DB_LOOKUP_FAILED,
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
            error!(target: "runtime::db::db_lookup", "[wasm-runtime] [Contract:{}] Too many open DbHandles", cid);
            DB_INIT_FAILED
        }
    }
}

/// Set a value within the transaction. `ptr` must contain the DbHandle index and
/// the key-value pair. The DbHandle must match the ContractId.
/// This function can be called only from the Deploy or Update [`ContractSection`].
/// Returns `0` on success, otherwise returns a (negative) error value.
pub(crate) fn db_set(ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, ptr_len: u32) -> i64 {
    let env = ctx.data();

    if let Err(e) = acl_allow(env, &[ContractSection::Deploy, ContractSection::Update]) {
        error!(target: "runtime::db::db_set", "[wasm-runtime] db_set ACL denied: {}", e);
        // TODO: FIXME: We have to fix up the errors used within runtime and the sdk
        return CALLER_ACCESS_DENIED
    }

    // Ensure that it is possible to read from the memory that this function needs
    let memory_view = env.memory_view(&ctx);

    let Ok(mem_slice) = ptr.slice(&memory_view, ptr_len) else {
        error!(target: "runtime::db::db_set", "Failed to make slice from ptr");
        return DB_SET_FAILED
    };

    let mut buf = vec![0_u8; ptr_len as usize];
    if let Err(e) = mem_slice.read_slice(&mut buf) {
        error!(target: "runtime::db::db_set", "Failed to read from memory slice: {}", e);
        return DB_SET_FAILED
    };

    let mut buf_reader = Cursor::new(buf);

    // Decode DbHandle index
    let db_handle_index: u32 = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(target: "runtime::db::db_set", "Failed to decode DbHandle: {}", e);
            return DB_SET_FAILED
        }
    };
    let db_handle_index = db_handle_index as usize;

    // Decode key and value
    let key: Vec<u8> = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(target: "runtime::db::db_set", "Failed to decode key vec: {}", e);
            return DB_SET_FAILED
        }
    };

    let value: Vec<u8> = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(target: "runtime::db::db_set", "Failed to decode value vec: {}", e);
            return DB_SET_FAILED
        }
    };

    // Make sure we've read the entire buffer
    if buf_reader.position() != ptr_len as u64 {
        error!(target: "runtime::db::db_set", "[wasm-runtime] Trailing bytes in argument stream");
        return DB_SET_FAILED
    }

    let db_handles = env.db_handles.borrow();

    // Check DbHandle index is within bounds
    if db_handles.len() <= db_handle_index {
        error!(target: "runtime::db::db_set", "Requested DbHandle that is out of bounds");
        return DB_SET_FAILED
    }

    // Retrive DbHandle using the index
    let db_handle = &db_handles[db_handle_index];

    // Validate that the DbHandle matches the contract ID
    if db_handle.contract_id != env.contract_id {
        error!(target: "runtime::db::db_set", "Unauthorized to write to DbHandle");
        return CALLER_ACCESS_DENIED
    }

    // Insert key-value pair into the database corresponding to this contract
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
        error!(target: "runtime::db::db_set", "Couldn't insert to db_handle tree");
        return DB_SET_FAILED
    }

    DB_SUCCESS
}

/// Remove a key from the database.
/// This function can be called only from the Deploy or Update [`ContractSection`].
/// Returns `0` on success, otherwise returns a (negative) error value.
pub(crate) fn db_del(ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, ptr_len: u32) -> i64 {
    let env = ctx.data();

    if let Err(e) = acl_allow(env, &[ContractSection::Deploy, ContractSection::Update]) {
        error!(target: "runtime::db::db_del", "[wasm-runtime] db_del ACL denied: {}", e);
        // TODO: FIXME: We have to fix up the errors used within runtime and the sdk
        return CALLER_ACCESS_DENIED
    }

    // Ensure that it is possible to read from the memory that this function needs
    let memory_view = env.memory_view(&ctx);

    let Ok(mem_slice) = ptr.slice(&memory_view, ptr_len) else {
        error!(target: "runtime::db::db_del", "Failed to make slice from ptr");
        return DB_DEL_FAILED
    };

    let mut buf = vec![0_u8; ptr_len as usize];
    if let Err(e) = mem_slice.read_slice(&mut buf) {
        error!(target: "runtime::db::db_del", "Failed to read from memory slice: {}", e);
        return DB_DEL_FAILED
    };

    let mut buf_reader = Cursor::new(buf);

    // Decode DbHandle index
    let db_handle_index: u32 = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(target: "runtime::db::db_del", "Failed to decode DbHandle: {}", e);
            return DB_DEL_FAILED
        }
    };
    let db_handle_index = db_handle_index as usize;

    // Decode key corresponding to the value that will be deleted
    let key: Vec<u8> = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(target: "runtime::db::db_del", "Failed to decode key vec: {}", e);
            return DB_DEL_FAILED
        }
    };

    // Make sure we've read the entire buffer
    if buf_reader.position() != ptr_len as u64 {
        error!(target: "runtime::db::db_del", "[wasm-runtime] Trailing bytes in argument stream");
        return DB_SET_FAILED
    }

    let db_handles = env.db_handles.borrow();

    if db_handles.len() <= db_handle_index {
        error!(target: "runtime::db::db_del()", "Requested DbHandle that is out of bounds");
        return DB_DEL_FAILED
    }

    // Retrive DbHandle using the index
    let db_handle = &db_handles[db_handle_index];

    // Validate that the DbHandle matches the contract ID
    if db_handle.contract_id != env.contract_id {
        error!(target: "runtime::db::db_del()", "Unauthorized to write to DbHandle");
        return CALLER_ACCESS_DENIED
    }

    // Remove key-value pair from the database corresponding to this contract
    if env.blockchain.lock().unwrap().overlay.lock().unwrap().remove(&db_handle.tree, &key).is_err()
    {
        error!(target: "runtime::db::db_del()", "Couldn't remove key from db_handle tree");
        return DB_DEL_FAILED
    }

    DB_SUCCESS
}

/// Reads a value by key from the key-value store.
/// Thie function can be called from the Deploy, Exec, or Metadata [`ContractSection`].
/// On success, returns the length of the `objects` Vector in the environment.
/// Otherwise, returns a negative error code.
pub(crate) fn db_get(ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, ptr_len: u32) -> i64 {
    let env = ctx.data();

    if let Err(e) =
        acl_allow(env, &[ContractSection::Deploy, ContractSection::Exec, ContractSection::Metadata])
    {
        error!(target: "runtime::db::db_get", "[wasm-runtime] db_get ACL denied: {}", e);
        // TODO: FIXME: We have to fix up the errors used within runtime and the sdk
        return CALLER_ACCESS_DENIED
    }

    // Ensure that it is possible to read memory
    let memory_view = env.memory_view(&ctx);

    let Ok(mem_slice) = ptr.slice(&memory_view, ptr_len) else {
        error!(target: "runtime::db::db_get", "Failed to make slice from ptr");
        return DB_GET_FAILED
    };

    let mut buf = vec![0_u8; ptr_len as usize];
    if let Err(e) = mem_slice.read_slice(&mut buf) {
        error!(target: "runtime::db::db_get", "Failed to read from memory slice: {}", e);
        return DB_GET_FAILED
    };

    let mut buf_reader = Cursor::new(buf);

    // Decode DbHandle index
    let db_handle_index: u32 = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(target: "runtime::db::db_get", "Failed to decode DbHandle: {}", e);
            return DB_GET_FAILED
        }
    };
    let db_handle_index = db_handle_index as usize;

    // Decode key for key-value pair that we wish to retrieve
    let key: Vec<u8> = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(target: "runtime::db::db_get", "Failed to decode key from vec: {}", e);
            return DB_GET_FAILED
        }
    };

    // Make sure there are no trailing bytes in the buffer. This means we've used all data that was
    // supplied.
    if buf_reader.position() != ptr_len as u64 {
        error!(target: "runtime::db::db_get", "[wasm-runtime] Trailing bytes in argument stream");
        return DB_GET_FAILED
    }

    let db_handles = env.db_handles.borrow();

    // Ensure that the index is within bounds
    if db_handles.len() <= db_handle_index {
        error!(target: "runtime::db::db_get", "Requested DbHandle that is out of bounds");
        return DB_GET_FAILED
    }

    // Get DbHandle using db_handle_index
    let db_handle = &db_handles[db_handle_index];

    // Retrieve data using the `key`
    let ret =
        match env.blockchain.lock().unwrap().overlay.lock().unwrap().get(&db_handle.tree, &key) {
            Ok(v) => v,
            Err(e) => {
                error!(target: "runtime::db::db_get", "Internal error getting from tree: {}", e);
                return DB_GET_FAILED
            }
        };

    // Return error if the data is empty
    let Some(return_data) = ret else {
        debug!(target: "runtime::db::db_get", "Return data is empty");
        return -127
    };

    // Copy the data (Vec<u8>) to the VM by pushing it to the objects Vector.
    let mut objects = env.objects.borrow_mut();
    objects.push(return_data.to_vec());

    // Return the length of the objects Vector. This is the location of the data that was retrieved
    // and pushed
    (objects.len() - 1) as i64
}

/// Check if a database contains a given key.
/// This function can be called by any [`ContractSection`].
/// Returns `1` if the key is found. Returns `0` if the key is not found and there are no errors.
/// Otherwise, returns a (negative) error code.
pub(crate) fn db_contains_key(ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, ptr_len: u32) -> i64 {
    let env = ctx.data();

    if let Err(e) = acl_allow(
        env,
        &[
            ContractSection::Deploy,
            ContractSection::Exec,
            ContractSection::Metadata,
            ContractSection::Update,
        ],
    ) {
        error!(target: "runtime::db::db_contains_key", "[wasm-runtime] db_contains_key ACL denied: {}", e);
        // TODO: FIXME: We have to fix up the errors used within runtime and the sdk
        return CALLER_ACCESS_DENIED
    }

    // Ensure memory is readable
    let memory_view = env.memory_view(&ctx);

    let Ok(mem_slice) = ptr.slice(&memory_view, ptr_len) else {
        error!(target: "runtime::db::db_contains_key()", "[wasm-runtime] Failed to make slice from ptr");
        return DB_CONTAINS_KEY_FAILED
    };

    let mut buf = vec![0_u8; ptr_len as usize];
    if let Err(e) = mem_slice.read_slice(&mut buf) {
        error!(target: "runtime::db::db_contains_key()", "[wasm-runtime] Failed to read from memory slice: {}", e);
        return DB_CONTAINS_KEY_FAILED
    };

    let mut buf_reader = Cursor::new(buf);

    // Decode DbHandle index
    let db_handle_index: u32 = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(target: "runtime::db::db_contains_key()", "[wasm-runtime] Failed to decode DbHandle: {}", e);
            return DB_CONTAINS_KEY_FAILED
        }
    };
    let db_handle_index = db_handle_index as usize;

    // Decode key
    let key: Vec<u8> = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(target: "runtime::db::db_contains_key()", "[wasm-runtime] Failed to decode key vec: {}", e);
            return DB_CONTAINS_KEY_FAILED
        }
    };

    // Make sure there are no trailing bytes in the buffer. This means we've used all data that was
    // supplied.
    if buf_reader.position() != ptr_len as u64 {
        error!(target: "runtime::db::db_contains_key", "[wasm-runtime] Trailing bytes in argument stream");
        return DB_CONTAINS_KEY_FAILED
    }

    let db_handles = env.db_handles.borrow();

    // Ensure DbHandle index is within bounds
    if db_handles.len() <= db_handle_index {
        error!(target: "runtime::db::db_contains_key", "[wasm-runtime] Requested DbHandle that is out of bounds");
        return DB_CONTAINS_KEY_FAILED
    }

    // Retrieve DbHandle using the index
    let db_handle = &db_handles[db_handle_index];

    // Lookup key parameter in the database
    match env.blockchain.lock().unwrap().overlay.lock().unwrap().contains_key(&db_handle.tree, &key)
    {
        Ok(v) => i64::from(v), // <- 0=false, 1=true. Convert bool to i64.
        Err(e) => {
            error!(target: "runtime::db::db_contains_key", "[wasm-runtime] sled.tree.contains_key failed: {}", e);
            DB_CONTAINS_KEY_FAILED
        }
    }
}

/// Given a zkas circuit, create a VerifyingKey and insert them both into the db.
/// This function can called only from the Deploy [`ContractSection`].
/// Returns `0` on success, otherwise returns a (negative) error code.
pub(crate) fn zkas_db_set(ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, ptr_len: u32) -> i64 {
    let env = ctx.data();

    if let Err(e) = acl_allow(env, &[ContractSection::Deploy]) {
        error!(target: "runtime::db::zkas_db_set", "[wasm-runtime] zkas_db_set ACL denied: {}", e);
        // TODO: FIXME: We have to fix up the errors used within runtime and the sdk
        return CALLER_ACCESS_DENIED
    }

    let memory_view = env.memory_view(&ctx);
    let contract_id = &env.contract_id;

    // Ensure that the memory is readable
    let Ok(mem_slice) = ptr.slice(&memory_view, ptr_len) else {
        error!(target: "runtime::db::zkas_db_set", "[wasm-runtime] Failed to make slice from ptr");
        return DB_SET_FAILED
    };

    let mut buf = vec![0u8; ptr_len as usize];
    if let Err(e) = mem_slice.read_slice(&mut buf) {
        error!(target: "runtime::db::zkas_db_set", "[wasm-runtime] Failed to read from memory slice: {}", e);
        return DB_SET_FAILED
    };

    // Deserialize the ZkBinary bytes from the buffer
    let zkbin_bytes: Vec<u8> = match deserialize(&buf) {
        Ok(zkbin) => zkbin,
        Err(e) => {
            error!(target: "runtime::db::zkas_db_set", "[wasm-runtime] Could not deserialize bytes from buffer: {}", e);
            return DB_SET_FAILED
        }
    };

    // Validate the bytes by decoding them into the ZkBinary format
    let zkbin = match ZkBinary::decode(&zkbin_bytes) {
        Ok(zkbin) => zkbin,
        Err(e) => {
            error!(target: "runtime::db::zkas_db_set", "[wasm-runtime] Invalid zkas bincode passed to function: {}", e);
            return DB_SET_FAILED
        }
    };

    // Because of `Runtime::Deploy`, we should be sure that the zkas db is index zero.
    let db_handles = env.db_handles.borrow();
    let db_handle = &db_handles[0];
    // Redundant check
    if &db_handle.contract_id != contract_id {
        error!(target: "runtime::db::zkas_db_set", "[wasm-runtime] Internal error, zkas db at index 0 incorrect");
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

                if existing_zkbin == zkbin_bytes {
                    debug!(target: "runtime::db::zkas_db_set", "[wasm-runtime] Existing zkas bincode is the same. Skipping.");
                    return DB_SUCCESS
                }
            }
        }
        Err(e) => {
            error!(target: "runtime::db::zkas_db_set", "[wasm-runtime] Internal error getting from tree: {}", e);
            return DB_SET_FAILED
        }
    };

    // We didn't find any existing bincode, so let's create a new VerifyingKey and write it all.
    info!(target: "runtime::db::zkas_db_set", "[wasm-runtime] Creating VerifyingKey for {} zkas circuit", zkbin.namespace);
    let witnesses = match empty_witnesses(&zkbin) {
        Ok(w) => w,
        Err(e) => {
            error!(target: "runtime::db::zkas_db_set", "[wasm-runtime] Failed to create empty witnesses: {}", e);
            return DB_SET_FAILED
        }
    };

    // Construct the circuit and build the VerifyingKey
    let circuit = ZkCircuit::new(witnesses, &zkbin);
    let vk = VerifyingKey::build(zkbin.k, &circuit);
    let mut vk_buf = vec![];
    if let Err(e) = vk.write(&mut vk_buf) {
        error!(target: "runtime::db::zkas_db_set", "[wasm-runtime] Failed to serialize VerifyingKey: {}", e);
        return DB_SET_FAILED
    }

    // Insert the key-value pair into the database.
    let key = serialize(&zkbin.namespace);
    let value = serialize(&(zkbin_bytes, vk_buf));
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
        error!(target: "runtime::db::zkas_db_set()", "[wasm-runtime] Couldn't insert to db_handle tree");
        return DB_SET_FAILED
    }

    DB_SUCCESS
}
