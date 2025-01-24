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

use core::{mem::size_of, slice::from_raw_parts};

use crate::crypto::ContractId;

/// Success exit code for a contract
pub const SUCCESS: i64 = 0;

#[macro_export]
macro_rules! define_contract {
    (
        init: $init_func:ident,
        exec: $exec_func:ident,
        apply: $apply_func:ident,
        metadata: $metadata_func:ident
    ) => {
        /// # Safety
        #[no_mangle]
        pub unsafe extern "C" fn __initialize(input: *mut u8) -> i64 {
            let (contract_id, instruction_data) = $crate::wasm::entrypoint::deserialize(input);

            match $init_func(contract_id, &instruction_data) {
                Ok(()) => $crate::wasm::entrypoint::SUCCESS,
                Err(e) => e.into(),
            }
        }
        #[no_mangle]
        pub unsafe extern "C" fn __entrypoint(input: *mut u8) -> i64 {
            let (contract_id, instruction_data) = $crate::wasm::entrypoint::deserialize(input);

            match $exec_func(contract_id, &instruction_data) {
                Ok(()) => $crate::wasm::entrypoint::SUCCESS,
                Err(e) => e.into(),
            }
        }
        #[no_mangle]
        pub unsafe extern "C" fn __update(input: *mut u8) -> i64 {
            let (contract_id, update_data) = $crate::wasm::entrypoint::deserialize(input);

            match $apply_func(contract_id, &update_data) {
                Ok(()) => $crate::wasm::entrypoint::SUCCESS,
                Err(e) => e.into(),
            }
        }
        #[no_mangle]
        pub unsafe extern "C" fn __metadata(input: *mut u8) -> i64 {
            let (contract_id, instruction_data) = $crate::wasm::entrypoint::deserialize(input);

            match $metadata_func(contract_id, &instruction_data) {
                Ok(()) => $crate::wasm::entrypoint::SUCCESS,
                Err(e) => e.into(),
            }
        }
    };
}

/// Deserialize a given payload in `entrypoint`
/// The return values from this are the input values for the above defined functions.
/// # Safety
pub unsafe fn deserialize<'a>(input: *mut u8) -> (ContractId, &'a [u8]) {
    let mut offset: usize = 0;

    let contract_id_len = 32;
    let contract_id_slice = { from_raw_parts(input.add(offset), contract_id_len) };
    offset += contract_id_len;

    let instruction_data_len = *(input.add(offset) as *const u64) as usize;
    offset += size_of::<u64>();
    let instruction_data = { from_raw_parts(input.add(offset), instruction_data_len) };

    let contract_id = ContractId::from_bytes(contract_id_slice.try_into().unwrap());
    // We unwrap here because if this panics, something's wrong in the runtime:
    let contract_id = contract_id.unwrap();

    (contract_id, instruction_data)
}
