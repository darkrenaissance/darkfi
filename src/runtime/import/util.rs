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
            error!(target: "wasm_runtime::drk_log", "Failed to read UTF-8 string from VM memory");
        }
    }
}

pub(crate) fn set_return_data(ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, len: u32) -> i32 {
    let env = ctx.data();
    match env.contract_section {
        ContractSection::Exec | ContractSection::Metadata => {
            let memory_view = env.memory_view(&ctx);

            let Ok(slice) = ptr.slice(&memory_view, len) else {
                return -2
            };

            let Ok(update_data) = slice.read_to_vec() else {
                return -2;
            };

            // This function should only ever be called once on the runtime.
            if !env.contract_return_data.take().is_none() {
                return -3
            }
            env.contract_return_data.set(Some(update_data));
            0
        }
        _ => -1,
    }
}
