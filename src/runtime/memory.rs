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

use wasmer::{MemoryView, WasmPtr};

use crate::Result;

pub trait MemoryManipulation {
    fn write_slice(&self, value_slice: &[u8], mem_offset: u32) -> Result<()>;
}

impl<'a> MemoryManipulation for MemoryView<'a> {
    fn write_slice(&self, value_slice: &[u8], mem_offset: u32) -> Result<()> {
        // Prepare WasmPtr
        let ptr: WasmPtr<u8> = WasmPtr::new(mem_offset);

        // Write to the slice
        let slice = ptr.slice(self, value_slice.len() as u32)?;

        Ok(slice.write_slice(value_slice)?)
    }
}
