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

use log::{error, warn};

use super::{memory::MemoryManipulation, vm_runtime::Env};

/// Serialize contract payload to format accepted by the runtime entrypoint.
/// We keep the same payload as a slice of bytes, and prepend it with a
/// little-endian u64 to tell the payload's length.
pub fn serialize_payload(payload: &[u8]) -> Vec<u8> {
    let mut out = vec![];

    let len = payload.len() as u64;
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(payload);

    out
}

/// Host function for logging strings.
/// This is injected into the runtime with wasmer's `imports!` macro.
pub(crate) fn drk_log(env: &Env, ptr: u32, len: u32) {
    // DISABLED
    /*
    if let Some(bytes) = env.memory.get_ref().unwrap().read(ptr, len as usize) {
        // Piece the string together
        let msg = match String::from_utf8(bytes.to_vec()) {
            Ok(v) => v,
            Err(e) => {
                warn!(target: "wasm_runtime", "Invalid UTF-8 string: {:?}", e);
                return
            }
        };

        let mut logs = env.logs.lock().unwrap();
        logs.push(msg);
        std::mem::drop(logs);
        return
    }

    error!(target: "wasm_runtime::drk_log", "Failed to read any bytes from VM memory");
    */
}
