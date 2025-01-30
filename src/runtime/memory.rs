/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
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

impl MemoryManipulation for MemoryView<'_> {
    fn write_slice(&self, value_slice: &[u8], mem_offset: u32) -> Result<()> {
        // Prepare WasmPtr
        let ptr: WasmPtr<u8> = WasmPtr::new(mem_offset);

        // Write to the slice
        let slice = ptr.slice(self, value_slice.len() as u32)?;

        Ok(slice.write_slice(value_slice)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    use darkfi_serial::Decodable;
    use wasmer::{Memory, MemoryType, Store};

    #[test]
    fn test_memoryview_writeslice() {
        let mut store = Store::default();
        let m = Memory::new(&mut store, MemoryType::new(1, None, false)).unwrap();
        let value: [u8; 3] = [3, 0, 1];
        let view = m.view(&store);
        let res = view.write_slice(&value, 0);
        assert!(res.is_ok());
        let ptr: WasmPtr<u8> = WasmPtr::new(0);
        let slice = ptr.slice(&view, value.len() as u32);
        let mut buf: [u8; 3] = [0; 3];
        let _ = slice.expect("err").read_slice(&mut buf);
        assert_eq!(buf, value);
    }

    #[test]
    fn test_memoryview_allread() -> Result<()> {
        let mut store = Store::default();
        let m = Memory::new(&mut store, MemoryType::new(1, None, false)).unwrap();
        let value: [u8; 3] = [3, 0, 1];
        let view = m.view(&store);
        let res = view.write_slice(&value, 0);
        assert!(res.is_ok());
        let ptr: WasmPtr<u8> = WasmPtr::new(0);
        let slice = ptr.slice(&view, value.len() as u32)?;
        let mut buf: [u8; 3] = [0; 3];
        let _ = slice.read_slice(&mut buf);
        let mut buf_reader = Cursor::new(buf);
        let ret: [u8; 3] = Decodable::decode(&mut buf_reader)?;
        assert!(buf.len() == (slice.len() as usize));
        assert_eq!(ret, value);
        assert!(buf_reader.position() == slice.len());
        Ok(())
    }
}
