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

use std::io::Cursor;

use darkfi_sdk::{
    crypto::contract_id::{
        ContractId, SMART_CONTRACT_MONOTREE_DB_NAME, SMART_CONTRACT_ZKAS_DB_NAME,
    },
    wasm,
};
use darkfi_serial::{deserialize, serialize, Decodable};
use tracing::{debug, error, info};
use wasmer::{FunctionEnvMut, WasmPtr};

use super::acl::acl_allow;
use crate::{
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
///
/// Permissions: deploy
pub(crate) fn db_init(mut ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, ptr_len: u32) -> i64 {
    let (env, mut store) = ctx.data_and_store_mut();
    let cid = env.contract_id;

    // Enforce function ACL
    if let Err(e) = acl_allow(env, &[ContractSection::Deploy]) {
        error!(
            target: "runtime::db::db_init",
            "[WASM] [{cid}] db_init(): Called in unauthorized section: {e}"
        );
        return darkfi_sdk::error::CALLER_ACCESS_DENIED
    }

    // Subtract used gas.
    // TODO: There should probably be an additional fee to open a new sled tree.
    env.subtract_gas(&mut store, 1);

    // This takes lock of the blockchain overlay reference in the wasm env
    let contracts = &env.blockchain.lock().unwrap().contracts;

    // Create a mem slice of the wasm VM memory
    let memory_view = env.memory_view(&store);
    let Ok(mem_slice) = ptr.slice(&memory_view, ptr_len) else {
        error!(
            target: "runtime::db::db_init",
            "[WASM] [{cid}] db_init(): Failed to make slice from ptr"
        );
        return darkfi_sdk::error::DB_INIT_FAILED
    };

    // Allocate a buffer and copy all the data from the pointer into the buffer
    let mut buf = vec![0_u8; ptr_len as usize];
    if let Err(e) = mem_slice.read_slice(&mut buf) {
        error!(
            target: "runtime::db::db_init",
            "[WASM] [{cid}] db_init(): Failed to read memory slice: {e}"
        );
        return darkfi_sdk::error::DB_INIT_FAILED
    };

    // Once the data is copied, we'll attempt to deserialize it into the objects
    // we're expecting.
    let mut buf_reader = Cursor::new(buf);
    let read_cid: ContractId = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::db::db_init",
                "[WASM] [{cid}] db_init(): Failed decoding ContractId: {e}"
            );
            return darkfi_sdk::error::DB_INIT_FAILED
        }
    };

    let read_db_name: String = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::db::db_init",
                "[WASM] [{cid}] db_init(): Failed decoding db_name: {e}"
            );
            return darkfi_sdk::error::DB_INIT_FAILED
        }
    };

    // Make sure we've read the entire buffer
    if buf_reader.position() != ptr_len as u64 {
        error!(
            target: "runtime::db::db_init",
            "[WASM] [{cid}] db_init(): Trailing bytes in argument stream"
        );
        return darkfi_sdk::error::DB_INIT_FAILED
    }

    // We cannot allow initializing the special zkas db:
    if read_db_name == SMART_CONTRACT_ZKAS_DB_NAME {
        error!(
            target: "runtime::db::db_init",
            "[WASM] [{cid}] db_init(): Attempted to init zkas db"
        );
        return darkfi_sdk::error::CALLER_ACCESS_DENIED
    }

    // Nor can we allow initializing the special monotree db:
    if read_db_name == SMART_CONTRACT_MONOTREE_DB_NAME {
        error!(
            target: "runtime::db::db_init",
            "[WASM] [{cid}] db_init(): Attempted to init monotree db"
        );
        return darkfi_sdk::error::CALLER_ACCESS_DENIED
    }

    // Nor can we allow another contract to initialize a db for someone else:
    if cid != read_cid {
        error!(
            target: "runtime::db::db_init",
            "[WASM] [{cid}] db_init(): Unauthorized ContractId"
        );
        return darkfi_sdk::error::CALLER_ACCESS_DENIED
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
            error!(
                target: "runtime::db::db_init",
                "[WASM] [{cid}] db_init(): Failed to init db: {e}"
            );
            return darkfi_sdk::error::DB_INIT_FAILED
        }
    };

    // Create the DbHandle
    let db_handle = DbHandle::new(read_cid, tree_handle);
    let mut db_handles = env.db_handles.borrow_mut();

    // Make sure we don't duplicate the DbHandle in the vec.
    // It's not really an issue, but it's better to be pedantic.
    if db_handles.contains(&db_handle) {
        error!(
            target: "runtime::db::db_init",
            "[WASM] [{cid}] db_init(): DbHandle initialized twice during execution"
        );
        return darkfi_sdk::error::DB_INIT_FAILED
    }

    // This tries to cast into u32
    match db_handles.len().try_into() {
        Ok(db_handle_idx) => {
            db_handles.push(db_handle);
            // Return the db handle index
            db_handle_idx
        }
        Err(_) => {
            error!(
                target: "runtime::db::db_init",
                "[WASM] [{cid}] db_init(): Too many open DbHandles"
            );
            darkfi_sdk::error::DB_INIT_FAILED
        }
    }
}

/// Lookup a database handle from its name.
/// If it exists, push it to the Vector of db_handles.
///
/// Returns the index of the DbHandle in the db_handles Vector on success.
/// Otherwise, returns an error value.
///
/// This function can be called from any [`ContractSection`].
///
/// Permissions: deploy, metadata, exec, update
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
            "[WASM] [{cid}] db_lookup() called in unauthorized section: {e}"
        );
        return darkfi_sdk::error::CALLER_ACCESS_DENIED
    }

    // Subtract used gas. Opening an existing db should be free (i.e. 1 gas unit).
    env.subtract_gas(&mut store, 1);

    // Read memory location that contains the ContractId and DB name
    let memory_view = env.memory_view(&store);
    let contracts = &env.blockchain.lock().unwrap().contracts;

    let Ok(mem_slice) = ptr.slice(&memory_view, ptr_len) else {
        error!(
            target: "runtime::db::db_lookup",
            "[WASM] [{cid}] db_lookup(): Failed to make slice from ptr."
        );
        return darkfi_sdk::error::DB_LOOKUP_FAILED
    };

    let mut buf = vec![0_u8; ptr_len as usize];
    if let Err(e) = mem_slice.read_slice(&mut buf) {
        error!(
            target: "runtime::db::db_lookup",
            "[WASM] [{cid}] db_lookup(): Failed to read from memory slice: {e}"
        );
        return darkfi_sdk::error::DB_LOOKUP_FAILED
    };

    // Wrap the buffer into a Cursor for stream reading
    let mut buf_reader = Cursor::new(buf);

    // Decode ContractId from memory
    let cid: ContractId = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::db::db_lookup",
                "[WASM] [{cid}] db_lookup(): Failed to decode ContractId: {e}"
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
                "[WASM] [{cid}] db_lookup(): Failed to decode db_name: {e}"
            );
            return darkfi_sdk::error::DB_LOOKUP_FAILED
        }
    };

    // Make sure we've read the entire buffer
    if buf_reader.position() != ptr_len as u64 {
        error!(
            target: "runtime::db::db_lookup",
            "[WASM] [{cid}] db_lookup(), Trailing bytes in argument stream"
        );
        return darkfi_sdk::error::DB_LOOKUP_FAILED
    }

    if db_name == SMART_CONTRACT_ZKAS_DB_NAME {
        error!(
            target: "runtime::db::db_lookup",
            "[WASM] [{cid}] db_lookup(): Attempted to lookup zkas db"
        );
        return darkfi_sdk::error::CALLER_ACCESS_DENIED
    }

    if db_name == SMART_CONTRACT_MONOTREE_DB_NAME {
        error!(
            target: "runtime::db::db_lookup",
            "[WASM] [{cid}] db_lookup(): Attempted to lookup monotree db"
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
                "[WASM] [{cid}] db_lookup(): Too many open DbHandles"
            );
            darkfi_sdk::error::DB_LOOKUP_FAILED
        }
    }
}

/// Set a value within the transaction.
///
/// * `ptr` must contain the DbHandle index and the key-value pair.
/// * The DbHandle must match the ContractId.
///
/// This function can be called only from the Deploy or Update [`ContractSection`].
/// Returns `SUCCESS` on success, otherwise returns an error value.
///
/// Permissions: deploy, update
pub(crate) fn db_set(mut ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, ptr_len: u32) -> i64 {
    let (env, mut store) = ctx.data_and_store_mut();
    let cid = env.contract_id;

    if let Err(e) = acl_allow(env, &[ContractSection::Deploy, ContractSection::Update]) {
        error!(
            target: "runtime::db::db_set",
            "[WASM] [{cid}] db_set(): Called in unauthorized section: {e}"
        );
        return darkfi_sdk::error::CALLER_ACCESS_DENIED
    }

    // Subtract used gas. Here we count the bytes written into the database.
    // TODO: We might want to count only the difference in size if we're replacing
    // data and the new data is larger.
    env.subtract_gas(&mut store, ptr_len as u64);

    // Ensure that it is possible to read from the memory that this function needs
    let memory_view = env.memory_view(&store);
    let Ok(mem_slice) = ptr.slice(&memory_view, ptr_len) else {
        error!(
            target: "runtime::db::db_set",
            "[WASM] [{cid}] db_set(): Failed to make slice from ptr"
        );
        return darkfi_sdk::error::DB_SET_FAILED
    };

    let mut buf = vec![0_u8; ptr_len as usize];
    if let Err(e) = mem_slice.read_slice(&mut buf) {
        error!(
            target: "runtime::db::db_set",
            "[WASM] [{cid}] db_set(): Failed to read from memory slice: {e}"
        );
        return darkfi_sdk::error::DB_SET_FAILED
    };

    let mut buf_reader = Cursor::new(buf);

    // Decode DbHandle index
    let db_handle_index: u32 = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::db::db_set",
                "[WASM] [{cid}] db_set(): Failed to decode DbHandle: {e}"
            );
            return darkfi_sdk::error::DB_SET_FAILED
        }
    };

    let db_handle_index = db_handle_index as usize;

    // Decode key and value
    let key: Vec<u8> = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::db::db_set",
                "[WASM] [{cid}] db_set(): Failed to decode key vec: {e}"
            );
            return darkfi_sdk::error::DB_SET_FAILED
        }
    };

    let value: Vec<u8> = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::db::db_set",
                "[WASM] [{cid}] db_set(): Failed to decode value vec: {e}"
            );
            return darkfi_sdk::error::DB_SET_FAILED
        }
    };

    // Make sure we've read the entire buffer
    if buf_reader.position() != ptr_len as u64 {
        error!(
            target: "runtime::db::db_set",
            "[WASM] [{cid}] db_set(): Trailing bytes in argument stream"
        );
        return darkfi_sdk::error::DB_SET_FAILED
    }

    let db_handles = env.db_handles.borrow();

    // Check DbHandle index is within bounds
    if db_handles.len() <= db_handle_index {
        error!(
            target: "runtime::db::db_set",
            "[WASM] [{cid}] db_set(): Requested DbHandle that is out of bounds"
        );
        return darkfi_sdk::error::DB_SET_FAILED
    }

    // Retrive DbHandle using the index
    let db_handle = &db_handles[db_handle_index];

    // Validate that the DbHandle matches the contract ID
    if db_handle.contract_id != env.contract_id {
        error!(
            target: "runtime::db::db_set",
            "[WASM] [{cid}] db_set(): Unauthorized to write to DbHandle"
        );
        return darkfi_sdk::error::CALLER_ACCESS_DENIED
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
        error!(
            target: "runtime::db::db_set",
            "[WASM] [{cid}] db_set(): Couldn't insert to db_handle tree"
        );
        return darkfi_sdk::error::DB_SET_FAILED
    }

    wasm::entrypoint::SUCCESS
}

/// Remove a key from the database.
///
/// This function can be called only from the Deploy or Update [`ContractSection`].
/// Returns `SUCCESS` on success, otherwise returns an error value.
///
/// Permissions: deploy, update
pub(crate) fn db_del(mut ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, ptr_len: u32) -> i64 {
    let (env, mut store) = ctx.data_and_store_mut();
    let cid = env.contract_id;

    if let Err(e) = acl_allow(env, &[ContractSection::Deploy, ContractSection::Update]) {
        error!(
            target: "runtime::db::db_del",
            "[WASM] [{cid}] db_del(): Called in unauthorized section: {e}"
        );
        return darkfi_sdk::error::CALLER_ACCESS_DENIED
    }

    // Subtract used gas. We make deletion free.
    env.subtract_gas(&mut store, 1);

    // Ensure that it is possible to read from the memory that this function needs
    let memory_view = env.memory_view(&store);

    let Ok(mem_slice) = ptr.slice(&memory_view, ptr_len) else {
        error!(
            target: "runtime::db::db_del",
            "[WASM] [{cid}] db_del(): Failed to make slice from ptr"
        );
        return darkfi_sdk::error::DB_DEL_FAILED
    };

    let mut buf = vec![0_u8; ptr_len as usize];
    if let Err(e) = mem_slice.read_slice(&mut buf) {
        error!(
            target: "runtime::db::db_del",
            "[WASM] [{cid}] db_del(): Failed to read from memory slice: {e}"
        );
        return darkfi_sdk::error::DB_DEL_FAILED
    };

    let mut buf_reader = Cursor::new(buf);

    // Decode DbHandle index
    let db_handle_index: u32 = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::db::db_del",
                "[WASM] [{cid}] db_del(): Failed to decode DbHandle: {e}"
            );
            return darkfi_sdk::error::DB_DEL_FAILED
        }
    };
    let db_handle_index = db_handle_index as usize;

    // Decode key corresponding to the value that will be deleted
    let key: Vec<u8> = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::db::db_del",
                "[WASM] [{cid}] db_del(): Failed to decode key vec: {e}"
            );
            return darkfi_sdk::error::DB_DEL_FAILED
        }
    };

    // Make sure we've read the entire buffer
    if buf_reader.position() != ptr_len as u64 {
        error!(
            target: "runtime::db::db_del",
            "[WASM] [{cid}] db_del(): Trailing bytes in argument stream"
        );
        return darkfi_sdk::error::DB_DEL_FAILED
    }

    let db_handles = env.db_handles.borrow();

    if db_handles.len() <= db_handle_index {
        error!(
            target: "runtime::db::db_del",
            "[WASM] [{cid}] db_del(): Requested DbHandle that is out of bounds"
        );
        return darkfi_sdk::error::DB_DEL_FAILED
    }

    // Retrive DbHandle using the index
    let db_handle = &db_handles[db_handle_index];

    // Validate that the DbHandle matches the contract ID
    if db_handle.contract_id != cid {
        error!(
            target: "runtime::db::db_del",
            "[WASM] [{cid}] db_del(): Unauthorized to write to DbHandle"
        );
        return darkfi_sdk::error::CALLER_ACCESS_DENIED
    }

    // Remove key-value pair from the database corresponding to this contract
    if env.blockchain.lock().unwrap().overlay.lock().unwrap().remove(&db_handle.tree, &key).is_err()
    {
        error!(
            target: "runtime::db::db_del",
            "[WASM] [{cid}] db_del(): Couldn't remove key from db_handle tree"
        );
        return darkfi_sdk::error::DB_DEL_FAILED
    }

    wasm::entrypoint::SUCCESS
}

/// Reads a value by key from the key-value store.
///
/// This function can be called from the Deploy, Exec, or Metadata [`ContractSection`].
///
/// On success, returns the length of the `objects` Vector in the environment.
/// Otherwise, returns an error code.
///
/// Permissions: deploy, metadata, exec
pub(crate) fn db_get(mut ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, ptr_len: u32) -> i64 {
    let (env, mut store) = ctx.data_and_store_mut();
    let cid = env.contract_id;

    if let Err(e) =
        acl_allow(env, &[ContractSection::Deploy, ContractSection::Metadata, ContractSection::Exec])
    {
        error!(
            target: "runtime::db::db_get",
            "[WASM] [{cid}] db_get(): Called in unauthorized section: {e}"
        );
        return darkfi_sdk::error::CALLER_ACCESS_DENIED
    }

    // Subtract used gas. Reading is free.
    env.subtract_gas(&mut store, 1);

    // Ensure that it is possible to read memory
    let memory_view = env.memory_view(&store);
    let Ok(mem_slice) = ptr.slice(&memory_view, ptr_len) else {
        error!(
            target: "runtime::db::db_get",
            "[WASM] [{cid}] db_get(): Failed to make slice from ptr"
        );
        return darkfi_sdk::error::DB_GET_FAILED
    };

    let mut buf = vec![0_u8; ptr_len as usize];
    if let Err(e) = mem_slice.read_slice(&mut buf) {
        error!(
            target: "runtime::db::db_get",
            "[WASM] [{cid}] db_get(): Failed to read from memory slice: {e}"
        );
        return darkfi_sdk::error::DB_GET_FAILED
    };

    let mut buf_reader = Cursor::new(buf);

    // Decode DbHandle index
    let db_handle_index: u32 = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::db::db_get",
                "[WASM] [{cid}] db_get(): Failed to decode DbHandle: {e}"
            );
            return darkfi_sdk::error::DB_GET_FAILED
        }
    };

    let db_handle_index = db_handle_index as usize;

    // Decode key for key-value pair that we wish to retrieve
    let key: Vec<u8> = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::db::db_get",
                "[WASM] [{cid}] db_get(): Failed to decode key from vec: {e}"
            );
            return darkfi_sdk::error::DB_GET_FAILED
        }
    };

    // Make sure there are no trailing bytes in the buffer. This means we've used all data that was
    // supplied.
    if buf_reader.position() != ptr_len as u64 {
        error!(
            target: "runtime::db::db_get",
            "[WASM] [{cid}] db_get(): Trailing bytes in argument stream"
        );
        return darkfi_sdk::error::DB_GET_FAILED
    }

    let db_handles = env.db_handles.borrow();

    // Ensure that the index is within bounds
    if db_handles.len() <= db_handle_index {
        error!(
            target: "runtime::db::db_get",
            "[WASM] [{cid}] db_get(): Requested DbHandle that is out of bounds"
        );
        return darkfi_sdk::error::DB_GET_FAILED
    }

    // Get DbHandle using db_handle_index
    let db_handle = &db_handles[db_handle_index];

    // Retrieve data using the `key`
    let ret =
        match env.blockchain.lock().unwrap().overlay.lock().unwrap().get(&db_handle.tree, &key) {
            Ok(v) => v,
            Err(e) => {
                error!(
                    target: "runtime::db::db_get",
                    "[WASM] [{cid}] db_get(): Internal error getting from tree: {e}"
                );
                return darkfi_sdk::error::DB_GET_FAILED
            }
        };
    drop(db_handles);

    // Return special error if the data is empty
    let Some(return_data) = ret else {
        debug!(
            target: "runtime::db::db_get",
            "[WASM] [{cid}] db_get(): Return data is empty"
        );
        return darkfi_sdk::error::DB_GET_EMPTY
    };

    if return_data.len() > u32::MAX as usize {
        return darkfi_sdk::error::DATA_TOO_LARGE
    }

    // Subtract used gas. Here we count the length of the data read from db.
    env.subtract_gas(&mut store, return_data.len() as u64);

    // Copy the data (Vec<u8>) to the VM by pushing it to the objects Vector.
    let mut objects = env.objects.borrow_mut();
    if objects.len() == u32::MAX as usize {
        return darkfi_sdk::error::DATA_TOO_LARGE
    }

    // Return the length of the objects Vector.
    // This is the location of the data that was retrieved and pushed
    objects.push(return_data.to_vec());
    (objects.len() - 1) as i64
}

/// Check if a database contains a given key.
///
/// Returns `1` if the key is found.
/// Returns `0` if the key is not found and there are no errors.
/// Otherwise, returns an error code.
///
/// Permissions: deploy, metadata, exec
pub(crate) fn db_contains_key(mut ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, ptr_len: u32) -> i64 {
    let (env, mut store) = ctx.data_and_store_mut();
    let cid = env.contract_id;

    if let Err(e) =
        acl_allow(env, &[ContractSection::Deploy, ContractSection::Metadata, ContractSection::Exec])
    {
        error!(
            target: "runtime::db::db_contains_key",
            "[WASM] [{cid}] db_contains_key(): Called in unauthorized section: {e}"
        );
        return darkfi_sdk::error::CALLER_ACCESS_DENIED
    }

    // Subtract used gas. Reading is free.
    env.subtract_gas(&mut store, 1);

    // Ensure memory is readable
    let memory_view = env.memory_view(&store);
    let Ok(mem_slice) = ptr.slice(&memory_view, ptr_len) else {
        error!(
            target: "runtime::db::db_contains_key",
            "[WASM] [{cid}] db_contains_key(): Failed to make slice from ptr"
        );
        return darkfi_sdk::error::DB_CONTAINS_KEY_FAILED
    };

    let mut buf = vec![0_u8; ptr_len as usize];
    if let Err(e) = mem_slice.read_slice(&mut buf) {
        error!(
            target: "runtime::db::db_contains_key",
            "[WASM] [{cid}] db_contains_key(): Failed to read from memory slice: {e}"
        );
        return darkfi_sdk::error::DB_CONTAINS_KEY_FAILED
    };

    let mut buf_reader = Cursor::new(buf);

    // Decode DbHandle index
    let db_handle_index: u32 = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::db::db_contains_key",
                "[WASM] [{cid}] db_contains_key(): Failed to decode DbHandle: {e}"
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
                target: "runtime::db::db_contains_key",
                "[WASM] [{cid}] db_contains_key(): Failed to decode key vec: {e}"
            );
            return darkfi_sdk::error::DB_CONTAINS_KEY_FAILED
        }
    };

    // Make sure there are no trailing bytes in the buffer.
    // This means we've used all data that was supplied.
    if buf_reader.position() != ptr_len as u64 {
        error!(
            target: "runtime::db::db_contains_key",
            "[WASM] [{cid}] db_contains_key(): Trailing bytes in argument stream"
        );
        return darkfi_sdk::error::DB_CONTAINS_KEY_FAILED
    }

    let db_handles = env.db_handles.borrow();

    // Ensure DbHandle index is within bounds
    if db_handles.len() <= db_handle_index {
        error!(
            target: "runtime::db::db_contains_key",
            "[WASM] [{cid}] db_contains_key(): Requested DbHandle that is out of bounds"
        );
        return darkfi_sdk::error::DB_CONTAINS_KEY_FAILED
    }

    // Retrieve DbHandle using the index
    let db_handle = &db_handles[db_handle_index];

    // Lookup key parameter in the database
    match env.blockchain.lock().unwrap().overlay.lock().unwrap().contains_key(&db_handle.tree, &key)
    {
        Ok(v) => i64::from(v), // <- 0=false, 1=true. Convert bool to i64.
        Err(e) => {
            error!(
                target: "runtime::db::db_contains_key",
                "[WASM] [{cid}] db_contains_key(): sled.tree.contains_key failed: {e}"
            );
            darkfi_sdk::error::DB_CONTAINS_KEY_FAILED
        }
    }
}

/// Given a zkas circuit, create a VerifyingKey and insert them both into the db.
///
/// This function can only be called from the Deploy [`ContractSection`].
/// Returns `SUCCESS` on success, otherwise returns an error code.
///
/// Permissions: deploy
pub(crate) fn zkas_db_set(mut ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, ptr_len: u32) -> i64 {
    let (env, mut store) = ctx.data_and_store_mut();
    let cid = env.contract_id;

    if let Err(e) = acl_allow(env, &[ContractSection::Deploy]) {
        error!(
            target: "runtime::db::zkas_db_set",
            "[WASM] [{cid}] zkas_db_set(): Called in unauthorized section: {e}"
        );
        return darkfi_sdk::error::CALLER_ACCESS_DENIED
    }

    let memory_view = env.memory_view(&store);

    // Ensure that the memory is readable
    let Ok(mem_slice) = ptr.slice(&memory_view, ptr_len) else {
        error!(
            target: "runtime::db::zkas_db_set",
            "[WASM] [{cid}] zkas_db_set(): Failed to make slice from ptr"
        );
        return darkfi_sdk::error::DB_SET_FAILED
    };

    let mut buf = vec![0u8; ptr_len as usize];
    if let Err(e) = mem_slice.read_slice(&mut buf) {
        error!(
            target: "runtime::db::zkas_db_set",
            "[WASM] [{cid}] zkas_db_set(): Failed to read from memory slice: {e}"
        );
        return darkfi_sdk::error::DB_SET_FAILED
    };

    // Deserialize the ZkBinary bytes from the buffer
    let zkbin_bytes: Vec<u8> = match deserialize(&buf) {
        Ok(zkbin) => zkbin,
        Err(e) => {
            error!(
                target: "runtime::db::zkas_db_set",
                "[WASM] [{cid}] zkas_db_set(): Could not deserialize bytes from buffer: {e}"
            );
            return darkfi_sdk::error::DB_SET_FAILED
        }
    };

    // Validate the bytes by decoding them into the ZkBinary format
    let zkbin = match ZkBinary::decode(&zkbin_bytes, false) {
        Ok(zkbin) => zkbin,
        Err(e) => {
            error!(
                target: "runtime::db::zkas_db_set",
                "[WASM] [{cid}] zkas_db_set(): Invalid zkas bincode passed to function: {e}"
            );
            return darkfi_sdk::error::DB_SET_FAILED
        }
    };

    // Subtract used gas. We count 100 gas per opcode, witness, and literal.
    // This is likely bad.
    // TODO: This should be better-priced.
    let gas_cost =
        (zkbin.literals.len() + zkbin.witnesses.len() + zkbin.opcodes.len()) as u64 * 100;
    env.subtract_gas(&mut store, gas_cost);

    // Because of `Runtime::Deploy`, we should be sure that the zkas db is index zero.
    let db_handles = env.db_handles.borrow();
    let db_handle = &db_handles[0];
    // Redundant check
    if db_handle.contract_id != cid {
        error!(
            target: "runtime::db::zkas_db_set",
            "[WASM] [{cid}] zkas_db_set(): Internal error, zkas db at index 0 incorrect"
        );
        return darkfi_sdk::error::DB_SET_FAILED
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
                    debug!(
                        target: "runtime::db::zkas_db_set",
                        "[WASM] [{cid}] zkas_db_set(): Existing zkas bincode is the same. Skipping."
                    );
                    return wasm::entrypoint::SUCCESS
                }
            }
        }
        Err(e) => {
            error!(
                target: "runtime::db::zkas_db_set",
                "[WASM] [{cid}] zkas_db_set(): Internal error getting from tree: {e}"
            );
            return darkfi_sdk::error::DB_SET_FAILED
        }
    };

    // We didn't find any existing bincode, so let's create a new VerifyingKey and write it all.
    info!(
        target: "runtime::db::zkas_db_set",
        "[WASM] [{cid}] zkas_db_set(): Creating VerifyingKey for {} zkas circuit",
        zkbin.namespace,
    );

    let witnesses = match empty_witnesses(&zkbin) {
        Ok(w) => w,
        Err(e) => {
            error!(
                target: "runtime::db::zkas_db_set",
                "[WASM] [{cid}] zkas_db_set(): Failed to create empty witnesses: {e}"
            );
            return darkfi_sdk::error::DB_SET_FAILED
        }
    };

    // Construct the circuit and build the VerifyingKey
    let circuit = ZkCircuit::new(witnesses, &zkbin);
    let vk = VerifyingKey::build(zkbin.k, &circuit);
    let mut vk_buf = vec![];
    if let Err(e) = vk.write(&mut vk_buf) {
        error!(
            target: "runtime::db::zkas_db_set",
            "[WASM] [{cid}] zkas_db_set(): Failed to serialize VerifyingKey: {e}"
        );
        return darkfi_sdk::error::DB_SET_FAILED
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
        error!(
            target: "runtime::db::zkas_db_set",
            "[WASM] [{cid}] zkas_db_set(): Couldn't insert to db_handle tree"
        );
        return darkfi_sdk::error::DB_SET_FAILED
    }
    drop(db_handles);

    // Subtract used gas. Here we count the bytes written into the db.
    env.subtract_gas(&mut store, (key.len() + value.len()) as u64);

    wasm::entrypoint::SUCCESS
}
