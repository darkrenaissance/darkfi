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

use darkfi_sdk::wasm;
use darkfi_serial::Decodable;
use tracing::{debug, error};
use wasmer::{FunctionEnvMut, WasmPtr};

use super::acl::acl_allow;
use crate::runtime::vm_runtime::{ContractSection, Env};

/// Host function for logging strings.
pub(crate) fn drk_log(mut ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, len: u32) {
    let (env, mut store) = ctx.data_and_store_mut();

    // Subtract used gas. Here we count the length of the string.
    env.subtract_gas(&mut store, len as u64);

    let memory_view = env.memory_view(&store);
    match ptr.read_utf8_string(&memory_view, len) {
        Ok(msg) => {
            let mut logs = env.logs.borrow_mut();
            logs.push(msg);
            std::mem::drop(logs);
        }
        Err(_) => {
            error!(
                target: "runtime::util::drk_log",
                "[WASM] [{}] drk_log(): Failed to read UTF-8 string from VM memory",
                env.contract_id,
            );
        }
    }
}

/// Writes data to the `contract_return_data` field of [`Env`].
/// The data will be read from `ptr` at a memory offset specified by `len`.
///
/// Returns `SUCCESS` on success, otherwise returns an error code corresponding
/// to a [`ContractError`].
///
/// Permissions: metadata, exec
pub(crate) fn set_return_data(mut ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, len: u32) -> i64 {
    let (env, mut store) = ctx.data_and_store_mut();
    let cid = &env.contract_id;

    // Enforce function ACL
    if let Err(e) = acl_allow(env, &[ContractSection::Metadata, ContractSection::Exec]) {
        error!(
            target: "runtime::util::set_return_data",
            "[WASM] [{cid}] set_return_data(): Called in unauthorized section: {e}"
        );
        return darkfi_sdk::error::CALLER_ACCESS_DENIED
    }

    // Subtract used gas. Here we count the length read from the memory slice.
    env.subtract_gas(&mut store, len as u64);

    let memory_view = env.memory_view(&store);
    let Ok(slice) = ptr.slice(&memory_view, len) else { return darkfi_sdk::error::INTERNAL_ERROR };
    let Ok(return_data) = slice.read_to_vec() else { return darkfi_sdk::error::INTERNAL_ERROR };

    // This function should only ever be called once on the runtime.
    if env.contract_return_data.take().is_some() {
        return darkfi_sdk::error::SET_RETVAL_ERROR
    }
    env.contract_return_data.set(Some(return_data));

    wasm::entrypoint::SUCCESS
}

/// Retrieve an object from the object store specified by the index `idx`.
/// The object's data is written to `ptr`.
///
/// Returns `SUCCESS` on success and an error code otherwise.
///
/// Permissions: deploy, metadata, exec
pub(crate) fn get_object_bytes(mut ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, idx: u32) -> i64 {
    // Get the slice, where we will read the size of the buffer
    let (env, mut store) = ctx.data_and_store_mut();
    let cid = env.contract_id;

    // Enforce function ACL
    if let Err(e) =
        acl_allow(env, &[ContractSection::Deploy, ContractSection::Metadata, ContractSection::Exec])
    {
        error!(
            target: "runtime::util::get_object_bytes",
            "[WASM] [{cid}] get_object_bytes(): Called in unauthorized section: {e}"
        );
        return darkfi_sdk::error::CALLER_ACCESS_DENIED
    }

    // Get the object from env
    let objects = env.objects.borrow();
    if idx as usize >= objects.len() {
        error!(
            target: "runtime::util::get_object_bytes",
            "[WASM] [{cid}] get_object_bytes(): Tried to access object out of bounds"
        );
        return darkfi_sdk::error::DATA_TOO_LARGE
    }
    let obj = objects[idx as usize].clone();
    drop(objects);

    if obj.len() > u32::MAX as usize {
        return darkfi_sdk::error::DATA_TOO_LARGE
    }

    // Subtract used gas. Here we count the bytes written to the memory slice
    env.subtract_gas(&mut store, obj.len() as u64);

    // Read N bytes from the object and write onto the ptr.
    let memory_view = env.memory_view(&store);
    let Ok(slice) = ptr.slice(&memory_view, obj.len() as u32) else {
        error!(
            target: "runtime::util::get_object_bytes",
            "[WASM] [{cid}] get_object_bytes(): Failed to make slice from ptr"
        );
        return darkfi_sdk::error::INTERNAL_ERROR
    };

    // Put the result in the VM
    if let Err(e) = slice.write_slice(&obj) {
        error!(
            target: "runtime::util::get_object_bytes",
            "[WASM] [{cid}] get_object_bytes(): Failed to write to memory slice: {e}"
        );
        return darkfi_sdk::error::INTERNAL_ERROR
    };

    wasm::entrypoint::SUCCESS
}

/// Returns the size (number of bytes) of an object in the object store
/// specified by index `idx`.
///
/// Permissions: deploy, metadata, exec
pub(crate) fn get_object_size(mut ctx: FunctionEnvMut<Env>, idx: u32) -> i64 {
    // Get the slice, where we will read the size of the buffer
    let (env, mut store) = ctx.data_and_store_mut();
    let cid = env.contract_id;

    // Enforce function ACL
    if let Err(e) =
        acl_allow(env, &[ContractSection::Deploy, ContractSection::Metadata, ContractSection::Exec])
    {
        error!(
            target: "runtime::util::get_object_size",
            "[WASM] [{cid}] get_object_size(): Called in unauthorized section: {e}"
        );
        return darkfi_sdk::error::CALLER_ACCESS_DENIED
    }

    // Get the object from env
    let objects = env.objects.borrow();
    if idx as usize >= objects.len() {
        error!(
            target: "runtime::util::get_object_size",
            "[WASM] [{cid}] get_object_size(): Tried to access object out of bounds"
        );
        return darkfi_sdk::error::DATA_TOO_LARGE
    }

    let obj = &objects[idx as usize];
    let obj_len = obj.len();
    drop(objects);

    if obj_len > u32::MAX as usize {
        return darkfi_sdk::error::DATA_TOO_LARGE
    }

    // Subtract used gas. Here we count the size of the object.
    // TODO: This could probably be fixed-cost
    env.subtract_gas(&mut store, obj_len as u64);

    obj_len as i64
}

/// Will return current runtime configured verifying block height number
///
/// Permissions: deploy, metadata, exec
pub(crate) fn get_verifying_block_height(mut ctx: FunctionEnvMut<Env>) -> i64 {
    let (env, mut store) = ctx.data_and_store_mut();
    let cid = env.contract_id;

    if let Err(e) =
        acl_allow(env, &[ContractSection::Deploy, ContractSection::Metadata, ContractSection::Exec])
    {
        error!(
            target: "runtime::util::get_verifying_block_height",
            "[WASM] [{cid}] get_verifying_block_height(): Called in unauthorized section: {e}"
        );
        return darkfi_sdk::error::CALLER_ACCESS_DENIED
    }

    // Subtract used gas. Here we count the size of the object.
    // u32 is 4 bytes.
    env.subtract_gas(&mut store, 4);

    env.verifying_block_height as i64
}

/// Will return currently configured block time target, in seconds
///
/// Permissions: deploy, metadata, exec
pub(crate) fn get_block_target(mut ctx: FunctionEnvMut<Env>) -> i64 {
    let (env, mut store) = ctx.data_and_store_mut();
    let cid = env.contract_id;

    if let Err(e) =
        acl_allow(env, &[ContractSection::Deploy, ContractSection::Metadata, ContractSection::Exec])
    {
        error!(
            target: "runtime::util::get_block_target",
            "[WASM] [{cid}] get_block_target(): Called in unauthorized section: {e}"
        );
        return darkfi_sdk::error::CALLER_ACCESS_DENIED
    }

    // Subtract used gas. Here we count the size of the object.
    // u32 is 4 bytes.
    env.subtract_gas(&mut store, 4);

    env.block_target as i64
}

/// Will return current runtime configured transaction hash
///
/// Permissions: deploy, metadata, exec
pub(crate) fn get_tx_hash(mut ctx: FunctionEnvMut<Env>) -> i64 {
    let (env, mut store) = ctx.data_and_store_mut();
    let cid = env.contract_id;

    if let Err(e) =
        acl_allow(env, &[ContractSection::Deploy, ContractSection::Metadata, ContractSection::Exec])
    {
        error!(
            target: "runtime::util::get_tx_hash",
            "[WASM] [{cid}] get_tx_hash(): Called in unauthorized section: {e}"
        );
        return darkfi_sdk::error::CALLER_ACCESS_DENIED
    }

    // Subtract used gas. Here we count the size of the object.
    env.subtract_gas(&mut store, 32);

    // Return the length of the objects Vector.
    // This is the location of the data that was retrieved and pushed
    let mut objects = env.objects.borrow_mut();
    objects.push(env.tx_hash.inner().to_vec());
    (objects.len() - 1) as i64
}

/// Will return current runtime configured verifying block height number
///
/// Permissions: deploy, metadata, exec
pub(crate) fn get_call_index(mut ctx: FunctionEnvMut<Env>) -> i64 {
    let (env, mut store) = ctx.data_and_store_mut();
    let cid = env.contract_id;

    if let Err(e) =
        acl_allow(env, &[ContractSection::Deploy, ContractSection::Metadata, ContractSection::Exec])
    {
        error!(
            target: "runtime::util::get_call_index",
            "[WASM] [{cid}] get_call_index(): Called in unauthorized section: {e}"
        );
        return darkfi_sdk::error::CALLER_ACCESS_DENIED
    }

    // Subtract used gas. Here we count the size of the object.
    // u8 is 1 byte.
    env.subtract_gas(&mut store, 1);

    env.call_idx as i64
}

/// Will return current blockchain timestamp,
/// defined as the last block's timestamp.
///
/// Permissions: deploy, metadata, exec
pub(crate) fn get_blockchain_time(mut ctx: FunctionEnvMut<Env>) -> i64 {
    let (env, mut store) = ctx.data_and_store_mut();
    let cid = &env.contract_id;

    if let Err(e) =
        acl_allow(env, &[ContractSection::Deploy, ContractSection::Metadata, ContractSection::Exec])
    {
        error!(
            target: "runtime::util::get_blockchain_time",
            "[WASM] [{cid}] get_blockchain_time(): Called in unauthorized section: {e}"
        );
        return darkfi_sdk::error::CALLER_ACCESS_DENIED
    }

    // Grab current last block
    let timestamp = match env.blockchain.lock().unwrap().last_block_timestamp() {
        Ok(b) => b,
        Err(e) => {
            error!(
                target: "runtime::util::get_blockchain_time",
                "[WASM] [{cid}] get_blockchain_time(): Internal error getting from blocks tree: {e}"
            );
            return darkfi_sdk::error::DB_GET_FAILED
        }
    };

    // Subtract used gas. Here we count the size of the object.
    // u64 is 8 bytes.
    env.subtract_gas(&mut store, 8);

    // Create the return object
    let mut ret = Vec::with_capacity(8);
    ret.extend_from_slice(&timestamp.inner().to_be_bytes());

    // Copy Vec<u8> to the VM
    let mut objects = env.objects.borrow_mut();
    objects.push(ret.to_vec());
    if objects.len() > u32::MAX as usize {
        return darkfi_sdk::error::DATA_TOO_LARGE
    }

    (objects.len() - 1) as i64
}

/// Grabs last block from the `Blockchain` overlay and then copies its
/// height to the VM's object store.
///
/// On success, returns the index of the new object in the object store.
/// Otherwise, returns an error code.
///
/// Permissions: deploy, metadata, exec
pub(crate) fn get_last_block_height(mut ctx: FunctionEnvMut<Env>) -> i64 {
    let (env, mut store) = ctx.data_and_store_mut();
    let cid = &env.contract_id;

    // Enforce function ACL
    if let Err(e) =
        acl_allow(env, &[ContractSection::Deploy, ContractSection::Metadata, ContractSection::Exec])
    {
        error!(
            target: "runtime::util::get_last_block_height",
            "[WASM] [{cid}] get_last_block_height(): Called in unauthorized section: {e}"
        );
        return darkfi_sdk::error::CALLER_ACCESS_DENIED
    }

    // Grab current last block height
    let height = match env.blockchain.lock().unwrap().last_block_height() {
        Ok(b) => b,
        Err(e) => {
            error!(
                target: "runtime::util::get_last_block_height",
                "[WASM] [{cid}] get_last_block_height(): Internal error getting from blocks tree: {e}"
            );
            return darkfi_sdk::error::DB_GET_FAILED
        }
    };

    // Subtract used gas. Here we count the size of the object.
    // u64 is 8 bytes.
    env.subtract_gas(&mut store, 8);

    // Create the return object
    let mut ret = Vec::with_capacity(8);
    ret.extend_from_slice(&darkfi_serial::serialize(&height));

    // Copy Vec<u8> to the VM
    let mut objects = env.objects.borrow_mut();
    objects.push(ret.to_vec());
    if objects.len() > u32::MAX as usize {
        return darkfi_sdk::error::DATA_TOO_LARGE
    }

    (objects.len() - 1) as i64
}

/// Reads a transaction by hash from the transactions store.
///
/// This function can be called from the Exec or Metadata [`ContractSection`].
///
/// On success, returns the length of the transaction bytes vector in the environment.
/// Otherwise, returns an error code.
///
/// Permissions: deploy, metadata, exec
pub(crate) fn get_tx(mut ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>) -> i64 {
    let (env, mut store) = ctx.data_and_store_mut();
    let cid = env.contract_id;

    if let Err(e) =
        acl_allow(env, &[ContractSection::Deploy, ContractSection::Metadata, ContractSection::Exec])
    {
        error!(
            target: "runtime::util::get_tx",
            "[WASM] [{cid}] get_tx(): Called in unauthorized section: {e}"
        );
        return darkfi_sdk::error::CALLER_ACCESS_DENIED
    }

    // Subtract used gas. Here we count the length of the looked-up hash.
    env.subtract_gas(&mut store, blake3::OUT_LEN as u64);

    // Ensure that it is possible to read memory
    let memory_view = env.memory_view(&store);
    let Ok(mem_slice) = ptr.slice(&memory_view, blake3::OUT_LEN as u32) else {
        error!(
            target: "runtime::util::get_tx",
            "[WASM] [{cid}] get_tx(): Failed to make slice from ptr"
        );
        return darkfi_sdk::error::DB_GET_FAILED
    };

    let mut buf = vec![0_u8; blake3::OUT_LEN];
    if let Err(e) = mem_slice.read_slice(&mut buf) {
        error!(
            target: "runtime::util::get_tx",
            "[WASM] [{cid}] get_tx(): Failed to read from memory slice: {e}"
        );
        return darkfi_sdk::error::DB_GET_FAILED
    };

    let mut buf_reader = Cursor::new(buf);

    // Decode hash bytes for transaction that we wish to retrieve
    let hash: [u8; blake3::OUT_LEN] = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::util::get_tx",
                "[WASM] [{cid}] get_tx(): Failed to decode hash from vec: {e}"
            );
            return darkfi_sdk::error::DB_GET_FAILED
        }
    };

    // Make sure there are no trailing bytes in the buffer. This means we've used all data that was
    // supplied.
    if buf_reader.position() != blake3::OUT_LEN as u64 {
        error!(
            target: "runtime::util::get_tx",
            "[WASM] [{cid}] get_tx(): Trailing bytes in argument stream"
        );
        return darkfi_sdk::error::DB_GET_FAILED
    }

    // Retrieve transaction using the `hash`
    let ret = match env.blockchain.lock().unwrap().transactions.get_raw(&hash) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::util::get_tx",
                "[WASM] [{cid}] get_tx(): Internal error getting from tree: {e}"
            );
            return darkfi_sdk::error::DB_GET_FAILED
        }
    };

    // Return special error if the data is empty
    let Some(return_data) = ret else {
        debug!(
            target: "runtime::util::get_tx",
            "[WASM] [{cid}] get_tx(): Return data is empty"
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

/// Reads a transaction location by hash from the transactions store.
///
/// This function can be called from the Exec or Metadata [`ContractSection`].
///
/// On success, returns the length of the transaction location bytes vector in
/// the environment. Otherwise, returns an error code.
///
/// Permissions: deploy, metadata, exec
pub(crate) fn get_tx_location(mut ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>) -> i64 {
    let (env, mut store) = ctx.data_and_store_mut();
    let cid = env.contract_id;

    if let Err(e) =
        acl_allow(env, &[ContractSection::Deploy, ContractSection::Metadata, ContractSection::Exec])
    {
        error!(
            target: "runtime::util::get_tx_location",
            "[WASM] [{cid}] get_tx_location(): Called in unauthorized section: {e}"
        );
        return darkfi_sdk::error::CALLER_ACCESS_DENIED
    }

    // Subtract used gas. Here we count the length of the looked-up hash.
    env.subtract_gas(&mut store, blake3::OUT_LEN as u64);

    // Ensure that it is possible to read memory
    let memory_view = env.memory_view(&store);
    let Ok(mem_slice) = ptr.slice(&memory_view, blake3::OUT_LEN as u32) else {
        error!(
            target: "runtime::util::get_tx_location",
            "[WASM] [{cid}] get_tx_location(): Failed to make slice from ptr"
        );
        return darkfi_sdk::error::DB_GET_FAILED
    };

    let mut buf = vec![0_u8; blake3::OUT_LEN];
    if let Err(e) = mem_slice.read_slice(&mut buf) {
        error!(
            target: "runtime::util::get_tx_location",
            "[WASM] [{cid}] get_tx_location(): Failed to read from memory slice: {e}"
        );
        return darkfi_sdk::error::DB_GET_FAILED
    };

    let mut buf_reader = Cursor::new(buf);

    // Decode hash bytes for transaction that we wish to retrieve
    let hash: [u8; blake3::OUT_LEN] = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::util::get_tx_location",
                "[WASM] [{cid}] get_tx_location(): Failed to decode hash from vec: {e}"
            );
            return darkfi_sdk::error::DB_GET_FAILED
        }
    };

    // Make sure there are no trailing bytes in the buffer. This means we've used all data that was
    // supplied.
    if buf_reader.position() != blake3::OUT_LEN as u64 {
        error!(
            target: "runtime::util::get_tx_location",
            "[WASM] [{cid}] get_tx_location(): Trailing bytes in argument stream"
        );
        return darkfi_sdk::error::DB_GET_FAILED
    }

    // Retrieve transaction using the `hash`
    let ret = match env.blockchain.lock().unwrap().transactions.get_location_raw(&hash) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::util::get_tx_location",
                "[WASM] [{cid}] get_tx_location(): Internal error getting from tree: {e}"
            );
            return darkfi_sdk::error::DB_GET_FAILED
        }
    };

    // Return special error if the data is empty
    let Some(return_data) = ret else {
        debug!(
            target: "runtime::util::get_tx_location",
            "[WASM] [{cid}] get_tx_location(): Return data is empty"
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
