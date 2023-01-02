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
