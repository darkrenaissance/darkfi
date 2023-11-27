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

use darkfi_sdk::db::{CALLER_ACCESS_DENIED, DB_GET_FAILED};
use log::error;
use wasmer::{FunctionEnvMut, WasmPtr};

use crate::runtime::vm_runtime::{ContractSection, Env};

/// Host function for logging strings.
/// This is injected into the runtime with wasmer's `imports!` macro.
pub(crate) fn drk_log(ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, len: u32) {
    let env = ctx.data();
    let memory_view = env.memory_view(&ctx);

    match ptr.read_utf8_string(&memory_view, len) {
        Ok(msg) => {
            let mut logs = env.logs.borrow_mut();
            logs.push(msg);
            std::mem::drop(logs);
        }
        Err(_) => {
            error!(target: "runtime::util", "Failed to read UTF-8 string from VM memory");
        }
    }
}

/// Writes data to the `contract_return_data` field of [`Env`]. The data will
/// be read from `ptr` at a memory offset specified by `len`.
/// Returns `0` on success, otherwise returns a positive error code
/// corresponding to a [`ContractError`]. Note that this is in contrast to other
/// methods in this file that return negative error codes or else return positive
/// integers that correspond to success states.
pub(crate) fn set_return_data(ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, len: u32) -> i64 {
    let env = ctx.data();
    match env.contract_section {
        ContractSection::Exec | ContractSection::Metadata => {
            let memory_view = env.memory_view(&ctx);

            let Ok(slice) = ptr.slice(&memory_view, len) else {
                return darkfi_sdk::error::INTERNAL_ERROR
            };

            let Ok(return_data) = slice.read_to_vec() else {
                return darkfi_sdk::error::INTERNAL_ERROR
            };

            // This function should only ever be called once on the runtime.
            if env.contract_return_data.take().is_some() {
                return darkfi_sdk::error::SET_RETVAL_ERROR
            }
            env.contract_return_data.set(Some(return_data));
            0
        }
        _ => darkfi_sdk::error::CALLER_ACCESS_DENIED,
    }
}

/// Appends a new object to the objects store. The data for the object is read from
/// `ptr`. Returns an index corresponding to the new object's index in the objects
/// store. (This index is equal to the last index in the store.)
pub(crate) fn put_object_bytes(ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, len: u32) -> i64 {
    let env = ctx.data();
    let memory_view = env.memory_view(&ctx);

    //debug!(target: "runtime::util", "diagnostic:");
    //let pages = memory_view.size().0;
    //debug!(target: "runtime::util", "    pages: {}", pages);

    let Ok(slice) = ptr.slice(&memory_view, len) else {
        error!(target: "runtime::util", "Failed to make slice from ptr");
        return -2
    };

    let mut buf = vec![0_u8; len as usize];
    if let Err(e) = slice.read_slice(&mut buf) {
        error!(target: "runtime::util", "Failed to read from memory slice: {}", e);
        return -2
    };

    // There would be a serious problem if this is zero.
    // The number of pages is calculated as a quantity X + 1 where X >= 0
    //assert!(pages > 0);

    //debug!(target: "runtime::util", "    memory: {:02x?}", &buf[0..32]);
    //debug!(target: "runtime::util", "            {:x?}", &buf[32..64]);

    //debug!(target: "runtime::util", "    ptr location: {}", ptr.offset());

    let mut objects = env.objects.borrow_mut();
    objects.push(buf);
    let obj_idx = objects.len() - 1;

    obj_idx as i64
}

/// Retrieve an object from the object store specified by the index `idx`. The object's
/// data is written to `ptr`. Returns `0` on success and an error code otherwise.
pub(crate) fn get_object_bytes(ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, idx: u32) -> i64 {
    // Get the slice, where we will read the size of the buffer

    let env = ctx.data();
    let memory_view = env.memory_view(&ctx);

    // Get the object from env

    let objects = env.objects.borrow();
    if idx as usize >= objects.len() {
        error!(target: "runtime::util", "Tried to access object out of bounds");
        return -5
    }
    let obj = &objects[idx as usize];

    // Read N bytes from the object and write onto the ptr.

    // We need to re-read the slice, since in the first run, we just read n
    let Ok(slice) = ptr.slice(&memory_view, obj.len() as u32) else {
        error!(target: "runtime::util", "Failed to make slice from ptr");
        return -2
    };

    // Put the result in the VM
    if let Err(e) = slice.write_slice(obj) {
        error!(target: "runtime::util", "Failed to write to memory slice: {}", e);
        return -4
    };

    0
}

// Returns the size (number of bytes) of an object in the object store
// specified by index `idx`.
pub(crate) fn get_object_size(ctx: FunctionEnvMut<Env>, idx: u32) -> i64 {
    // Get the slice, where we will read the size of the buffer

    let env = ctx.data();
    //let memory_view = env.memory_view(&ctx);

    // Get the object from env

    let objects = env.objects.borrow();
    if idx as usize >= objects.len() {
        error!(target: "runtime::util", "Tried to access object out of bounds");
        return -5
    }

    let obj = &objects[idx as usize];
    obj.len() as i64
}

/// Will return current epoch number.
pub(crate) fn get_current_epoch(ctx: FunctionEnvMut<Env>) -> u64 {
    ctx.data().time_keeper.current_epoch()
}

/// Will return current slot number.
pub(crate) fn get_current_slot(ctx: FunctionEnvMut<Env>) -> u64 {
    ctx.data().time_keeper.current_slot()
}

/// Will return current runtime configured verifying slot number.
pub(crate) fn get_verifying_slot(ctx: FunctionEnvMut<Env>) -> u64 {
    ctx.data().time_keeper.verifying_slot
}

/// Will return current runtime configured verifying slot epoch number.
pub(crate) fn get_verifying_slot_epoch(ctx: FunctionEnvMut<Env>) -> u64 {
    ctx.data().time_keeper.verifying_slot_epoch()
}

/// Copies the data of requested slot from `SlotStore` into the VM by appending
/// the data to the VM's object store. On success, returns the index of the new object in
/// the object store. Otherwise, returns an error code (negative value).
pub(crate) fn get_slot(ctx: FunctionEnvMut<Env>, slot: u64) -> i64 {
    let env = ctx.data();

    if env.contract_section != ContractSection::Deploy &&
        env.contract_section != ContractSection::Exec &&
        env.contract_section != ContractSection::Metadata
    {
        error!(target: "runtime::db::db_get_slot()", "db_get_slot called in unauthorized section");
        return CALLER_ACCESS_DENIED.into()
    }

    let ret = match env.blockchain.lock().unwrap().slots.get_by_id(slot) {
        Ok(v) => v,
        Err(e) => {
            error!(target: "runtime::db::db_get_slot()", "Internal error getting from slots tree: {}", e);
            return DB_GET_FAILED.into()
        }
    };

    // Copy Vec<u8> to the VM
    let mut objects = env.objects.borrow_mut();
    objects.push(ret.to_vec());
    (objects.len() - 1) as i64
}

/// Will return current blockchain timestamp.
pub(crate) fn get_blockchain_time(ctx: FunctionEnvMut<Env>) -> u64 {
    ctx.data().time_keeper.blockchain_timestamp()
}
