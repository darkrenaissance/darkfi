/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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

use log::error;
use wasmer::{FunctionEnvMut, WasmPtr};

use darkfi_sdk::crypto::pasta_prelude::PrimeField;

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
pub(crate) fn set_return_data(mut ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, len: u32) -> i64 {
    let (env, mut store) = ctx.data_and_store_mut();
    let cid = &env.contract_id;

    // Enforce function ACL
    if let Err(e) = acl_allow(env, &[ContractSection::Metadata, ContractSection::Exec]) {
        error!(
            target: "runtime::util::set_return_data",
            "[WASM] [{}] set_return_data(): Called in unauthorized section: {}", cid, e,
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

    darkfi_sdk::entrypoint::SUCCESS
}

/// Appends a new object to the [`Env`] objects store.
/// The data for the object is read from `ptr`.
///
/// Returns an index corresponding to the new object's index in the objects
/// store. (This index is equal to the last index in the store.)
pub(crate) fn put_object_bytes(mut ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, len: u32) -> i64 {
    let (env, mut store) = ctx.data_and_store_mut();
    let cid = env.contract_id;

    // Subtract used gas. Here we count the length read from the memory slice.
    env.subtract_gas(&mut store, len as u64);

    let memory_view = env.memory_view(&store);
    //debug!(target: "runtime::util", "diagnostic:");
    let pages = memory_view.size().0;
    //debug!(target: "runtime::util", "    pages: {}", pages);

    let Ok(slice) = ptr.slice(&memory_view, len) else {
        error!(
            target: "runtime::util::put_object_bytes",
            "[WASM] [{}] put_object_bytes(): Failed to make slice from ptr", cid,
        );
        return darkfi_sdk::error::INTERNAL_ERROR
    };

    let mut buf = vec![0_u8; len as usize];
    if let Err(e) = slice.read_slice(&mut buf) {
        error!(
            target: "runtime::util::put_object_bytes",
            "[WASM] [{}] put_object_bytes(): Failed to read from memory slice: {}", cid, e,
        );
        return darkfi_sdk::error::INTERNAL_ERROR
    };

    // There would be a serious problem if this is zero.
    // The number of pages is calculated as a quantity X + 1 where X >= 0
    assert!(pages > 0);

    //debug!(target: "runtime::util", "    memory: {:02x?}", &buf[0..32]);
    //debug!(target: "runtime::util", "            {:x?}", &buf[32..64]);
    //debug!(target: "runtime::util", "    ptr location: {}", ptr.offset());

    let mut objects = env.objects.borrow_mut();
    objects.push(buf);
    let obj_idx = objects.len() - 1;

    if obj_idx > u32::MAX as usize {
        return darkfi_sdk::error::DATA_TOO_LARGE
    }

    obj_idx as i64
}

/// Retrieve an object from the object store specified by the index `idx`.
/// The object's data is written to `ptr`.
///
/// Returns `SUCCESS` on success and an error code otherwise.
pub(crate) fn get_object_bytes(mut ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, idx: u32) -> i64 {
    // Get the slice, where we will read the size of the buffer
    let (env, mut store) = ctx.data_and_store_mut();
    let cid = env.contract_id;

    // Get the object from env
    let objects = env.objects.borrow();
    if idx as usize >= objects.len() {
        error!(
            target: "runtime::util::get_object_bytes",
            "[WASM] [{}] get_object_bytes(): Tried to access object out of bounds", cid,
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
            "[WASM] [{}] get_object_bytes(): Failed to make slice from ptr", cid,
        );
        return darkfi_sdk::error::INTERNAL_ERROR
    };

    // Put the result in the VM
    if let Err(e) = slice.write_slice(&obj) {
        error!(
            target: "runtime::util::get_object_bytes",
            "[WASM] [{}] get_object_bytes(): Failed to write to memory slice: {}", cid, e,
        );
        return darkfi_sdk::error::INTERNAL_ERROR
    };

    darkfi_sdk::entrypoint::SUCCESS
}

/// Returns the size (number of bytes) of an object in the object store
/// specified by index `idx`.
pub(crate) fn get_object_size(mut ctx: FunctionEnvMut<Env>, idx: u32) -> i64 {
    // Get the slice, where we will read the size of the buffer
    let (env, mut store) = ctx.data_and_store_mut();
    let cid = env.contract_id;

    // Get the object from env
    let objects = env.objects.borrow();
    if idx as usize >= objects.len() {
        error!(
            target: "runtime::util::get_object_size",
            "[WASM] [{}] get_object_size(): Tried to access object out of bounds", cid,
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

/// Will return current epoch number.
pub(crate) fn get_current_epoch(ctx: FunctionEnvMut<Env>) -> u64 {
    // TODO: Gas cost
    ctx.data().time_keeper.current_epoch()
}

/// Will return current block height number, which is equivalent
/// to current slot number.
pub(crate) fn get_current_block_height(ctx: FunctionEnvMut<Env>) -> u64 {
    // TODO: Gas cost
    ctx.data().time_keeper.current_slot()
}

/// Will return current slot number.
pub(crate) fn get_current_slot(ctx: FunctionEnvMut<Env>) -> u64 {
    // TODO: Gas cost
    ctx.data().time_keeper.current_slot()
}

/// Will return current runtime configured verifying block height number,
/// which is equivalent to verifying slot number.
pub(crate) fn get_verifying_block_height(ctx: FunctionEnvMut<Env>) -> u64 {
    // TODO: Gas cost
    ctx.data().time_keeper.verifying_block_height
}

/// Will return current runtime configured verifying block height epoch number,
/// which is equivalent to verifying slot epoch number.
pub(crate) fn get_verifying_block_height_epoch(ctx: FunctionEnvMut<Env>) -> u64 {
    // TODO: Gas cost
    ctx.data().time_keeper.verifying_block_height_epoch()
}

/// Grabs last block from the `Blockchain` overlay and then copies its
/// height, nonce and previous block hash into the VM, by appending the data
/// to the VM's object store.
///
/// On success, returns the index of the new object in the object store.
/// Otherwise, returns an error code.
pub(crate) fn get_last_block_info(mut ctx: FunctionEnvMut<Env>) -> i64 {
    let (env, mut store) = ctx.data_and_store_mut();
    let cid = &env.contract_id;

    // Enforce function ACL
    if let Err(e) = acl_allow(env, &[ContractSection::Exec]) {
        error!(
            target: "runtime::db::get_last_block_info",
            "[WASM] [{}] get_last_block_info(): Called in unauthorized section: {}", cid, e,
        );
        return darkfi_sdk::error::CALLER_ACCESS_DENIED
    }

    // Grab current last block
    let block = match env.blockchain.lock().unwrap().last_block() {
        Ok(b) => b,
        Err(e) => {
            error!(
                target: "runtime::db::get_last_block_info",
                "[WASM] [{}] get_last_block_info(): Internal error getting from blocks tree: {}", cid, e,
            );
            return darkfi_sdk::error::DB_GET_FAILED
        }
    };

    // Create the return object
    let mut ret = Vec::with_capacity(8 + 32 + blake3::OUT_LEN);
    ret.extend_from_slice(&block.header.height.to_be_bytes());
    ret.extend_from_slice(&block.header.nonce.to_repr());
    ret.extend_from_slice(block.header.previous.as_bytes());

    // Subtract used gas. Here we count the size of the object.
    env.subtract_gas(&mut store, ret.len() as u64);

    // Copy Vec<u8> to the VM
    let mut objects = env.objects.borrow_mut();
    objects.push(ret.to_vec());
    if objects.len() > u32::MAX as usize {
        return darkfi_sdk::error::DATA_TOO_LARGE
    }

    (objects.len() - 1) as i64
}

/// Copies the data of requested slot from `SlotStore` into the VM by appending
/// the data to the VM's object store.
///
/// On success, returns the index of the new object in the object store.
/// Otherwise, returns an error code.
pub(crate) fn get_slot(mut ctx: FunctionEnvMut<Env>, slot: u64) -> i64 {
    let (env, mut store) = ctx.data_and_store_mut();
    let cid = &env.contract_id;

    // Enforce function ACL
    if let Err(e) =
        acl_allow(env, &[ContractSection::Deploy, ContractSection::Metadata, ContractSection::Exec])
    {
        error!(
            target: "runtime::db::db_get_slot",
            "[WASM] [{}] get_slot({}): Called in unauthorized section: {}", cid, slot, e,
        );
        return darkfi_sdk::error::CALLER_ACCESS_DENIED
    }

    let ret = match env.blockchain.lock().unwrap().slots.get_by_id(slot) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::db::db_get_slot",
                "[WASM] [{}] db_get_slot(): Internal error getting from slots tree: {}", cid, e,
            );
            return darkfi_sdk::error::DB_GET_FAILED
        }
    };

    // Subtract used gas. Here we count the size of the object.
    env.subtract_gas(&mut store, ret.len() as u64);

    // Copy Vec<u8> to the VM
    let mut objects = env.objects.borrow_mut();
    objects.push(ret.to_vec());
    if objects.len() > u32::MAX as usize {
        return darkfi_sdk::error::DATA_TOO_LARGE
    }

    (objects.len() - 1) as i64
}

/// Will return current blockchain timestamp.
pub(crate) fn get_blockchain_time(ctx: FunctionEnvMut<Env>) -> u64 {
    // TODO: Gas cost
    ctx.data().time_keeper.blockchain_timestamp()
}
