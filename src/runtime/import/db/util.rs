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

use wasmer::{StoreMut, WasmPtr};

use crate::{runtime::vm_runtime::Env, Result};

/// Create a mem slice of the WASM VM memory given a pointer and its length,
/// and return a `Cursor` from which callers are able to read as a stream.
pub fn wasm_mem_read(
    env: &Env,
    store: &StoreMut<'_>,
    ptr: WasmPtr<u8>,
    ptr_len: u32,
) -> Result<Cursor<Vec<u8>>> {
    let memory_view = env.memory_view(&store);
    let mem_slice = ptr.slice(&memory_view, ptr_len)?;

    // Allocate a buffer and copy all the data from the pointer
    // into the buffer
    let mut buf = vec![0u8; ptr_len as usize];
    mem_slice.read_slice(&mut buf)?;

    // Once the data is copied, we'll return a Cursor over it
    Ok(Cursor::new(buf))
}
