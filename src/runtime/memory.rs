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

use wasmer::{Array, Memory, WasmPtr};

use crate::{Error, Result};

pub trait MemoryManipulation {
    fn write(&self, mem_offset: u32, value_slice: &[u8]) -> Result<()>;
    fn read(&self, mem_offset: u32, value_len: usize) -> Option<&[u8]>;
}

impl MemoryManipulation for Memory {
    fn write(&self, mem_offset: u32, value_slice: &[u8]) -> Result<()> {
        // Prepare WasmPtr
        let target_ptr: WasmPtr<u8, Array> = WasmPtr::new(mem_offset);

        // Allocate necessary memory space on guest
        let guest_value_slice = match target_ptr.deref(self, 0, value_slice.len() as u32) {
            Some(slice) => slice,
            None => [].to_vec(),
        };

        if guest_value_slice.is_empty() {
            return Err(Error::WasmerOomError)
        }

        // Copy bytes to guest
        for i in 0..value_slice.len() {
            guest_value_slice[i].set(value_slice[i]);
        }

        Ok(())
    }

    fn read(&self, mem_offset: u32, value_len: usize) -> Option<&[u8]> {
        let memory_size = self.size().bytes().0;

        if mem_offset as usize + value_len > memory_size || mem_offset as usize >= memory_size {
            return None
        }

        let ptr = unsafe { self.view::<u8>().as_ptr().add(mem_offset as usize) as *const u8 };
        unsafe { Some(std::slice::from_raw_parts(ptr, value_len)) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasmer::{imports, wat2wasm, Instance, Module, Store};

    fn wasmer_instance() -> Instance {
        let wasm_bytes = wat2wasm(
            br#"
            (module
            (type $add_one_t (func (param i32) (result i32)))
            (func $add_one_f (type $add_one_t) (param $value i32) (result i32)
            local.get $value
            i32.const 1
            i32.add)
            (export "add_one" (func $add_one_f))
            (memory $memory (export "memory") 17))
            "#,
        )
        .unwrap();

        let store = Store::default();
        let module = Module::new(&store, wasm_bytes).unwrap();

        let import_object = imports! {};
        Instance::new(&module, &import_object).unwrap()
    }

    #[test]
    fn can_write_on_memory() {
        let wasmer_instance = wasmer_instance();

        let memory = wasmer_instance.exports.get_memory("memory").unwrap();
        let data = String::from("data_test");

        let mem_addr = 0x2220;

        memory.write(mem_addr as u32, data.as_bytes()).unwrap();

        let ptr = unsafe { memory.view::<u8>().as_ptr().add(mem_addr as usize) as *const u8 };
        let slice_raw = unsafe { std::slice::from_raw_parts(ptr, data.len()) };

        assert_eq!(data.as_bytes(), slice_raw);
    }

    #[test]
    fn can_read_from_memory() {
        let wasmer_instance = wasmer_instance();

        let memory = wasmer_instance.exports.get_memory("memory").unwrap();
        let data = String::from("data_test");

        let mem_addr = 0x2220;

        memory.write(mem_addr as u32, data.as_bytes()).unwrap();

        let slice_raw = memory.read(mem_addr as u32, data.as_bytes().len()).unwrap();

        assert_eq!(data.as_bytes(), slice_raw);
    }
}
