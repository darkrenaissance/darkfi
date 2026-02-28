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

use darkfi_serial::Decodable;
use tracing::{debug, error};
use wasmer::{FunctionEnvMut, WasmPtr};

use crate::runtime::{
    import::acl::acl_allow,
    vm_runtime::{ContractSection, Env},
};

use super::util::wasm_mem_read;

/// Reads a value by key from the on-chain key-value store.
///
/// On success, returns the length of the `objects` Vector in the environment.
/// Otherwise, returns an error code.
///
/// ## Permissions
/// * `ContractSection::Deploy`
/// * `ContractSection::Metadata`
/// * `ContractSection::Exec`
pub(crate) fn db_get(mut ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, ptr_len: u32) -> i64 {
    let (env, mut store) = ctx.data_and_store_mut();
    let cid = env.contract_id;

    // Enforce function ACL
    if let Err(e) =
        acl_allow(env, &[ContractSection::Deploy, ContractSection::Metadata, ContractSection::Exec])
    {
        error!(
            target: "runtime::db::db_get",
            "[WASM] [{cid}] db_get(): Called in unauthorized section: {e}",
        );
        return darkfi_sdk::error::CALLER_ACCESS_DENIED
    }

    // Subtract used gas. Reading is free.
    env.subtract_gas(&mut store, 1);

    // Get the wasm memory reader
    let mut buf_reader = match wasm_mem_read(env, &store, ptr, ptr_len) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::db::db_get",
                "[WASM] [{cid}] db_get(): Failed to read wasm memory: {e}",
            );
            return darkfi_sdk::error::DB_GET_FAILED
        }
    };

    // Decode DbHandle index
    let db_handle_index: u32 = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::db::db_get",
                "[WASM] [{cid}] db_get(): Failed to decode DbHandle: {e}",
            );
            return darkfi_sdk::error::DB_GET_FAILED
        }
    };

    let db_handle_index = db_handle_index as usize;

    // Decode key for key-value pair that we wish to retrieve
    let key: Vec<u8> = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::db::db_get",
                "[WASM] [{cid}] db_get(): Failed to decode key from vec: {e}",
            );
            return darkfi_sdk::error::DB_GET_FAILED
        }
    };

    // Make sure there are no trailing bytes in the buffer.
    if buf_reader.position() != ptr_len as u64 {
        error!(
            target: "runtime::db::db_get",
            "[WASM] [{cid}] db_get(): Trailing bytes in argument stream",
        );
        return darkfi_sdk::error::DB_GET_FAILED
    }

    let db_handles = env.db_handles.borrow();

    // Ensure that the index is within bounds
    if db_handles.len() <= db_handle_index {
        error!(
            target: "runtime::db::db_get",
            "[WASM] [{cid}] db_get(): Requested DbHandle that is out of bounds",
        );
        return darkfi_sdk::error::DB_GET_FAILED
    }

    // Get DbHandle using db_handle_index
    let db_handle = &db_handles[db_handle_index];

    // Retrieve data using the `key`
    let ret =
        match env.blockchain.lock().unwrap().overlay.lock().unwrap().get(&db_handle.tree, &key) {
            Ok(v) => v,
            Err(e) => {
                error!(
                    target: "runtime::db::db_get",
                    "[WASM] [{cid}] db_get(): Internal error getting from tree: {e}",
                );
                return darkfi_sdk::error::DB_GET_FAILED
            }
        };
    drop(db_handles);

    // Return special error if the data is empty
    let Some(return_data) = ret else {
        debug!(
            target: "runtime::db::db_get",
            "[WASM] [{cid}] db_get(): Return data is empty",
        );
        return darkfi_sdk::error::DB_GET_EMPTY
    };

    if return_data.len() > u32::MAX as usize {
        return darkfi_sdk::error::DATA_TOO_LARGE
    }

    // Subtract used gas. Here we count the length of the data read from db.
    env.subtract_gas(&mut store, return_data.len() as u64);

    // Copy the data (Vec<u8>) to the VM by pushing it to the objects Vector.
    let mut objects = env.objects.borrow_mut();
    if objects.len() == u32::MAX as usize {
        return darkfi_sdk::error::DATA_TOO_LARGE
    }

    // Return the length of the objects Vector.
    // This is the location of the data that was retrieved and pushed
    objects.push(return_data.to_vec());
    (objects.len() - 1) as i64
}
