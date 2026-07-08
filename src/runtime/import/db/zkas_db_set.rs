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

use darkfi_sdk::wasm;
use darkfi_serial::{deserialize, serialize, Decodable};
use tracing::{debug, error, info};
use wasmer::{FunctionEnvMut, WasmPtr};

use crate::{
    runtime::{
        import::{acl::acl_allow, util::wasm_mem_read},
        vm_runtime::{ContractSection, Env},
    },
    validator::fees::{COMPILE_GAS_PER_ROW, MIN_GAS, STATE_GROWTH_GAS, WRITE_GAS_PER_BYTE},
    zk::{empty_witnesses, VerifyingKey, ZkCircuit},
    zkas::ZkBinary,
};

/// Given a zkas circuit, create a VerifyingKey and insert them both into
/// the on-chain db.
///
/// Returns `SUCCESS` on success, otherwise returns an error code.
///
/// ## Permissions
/// * `ContractSection::Deploy`
pub(crate) fn zkas_db_set(mut ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, ptr_len: u32) -> i64 {
    let (env, mut store) = ctx.data_and_store_mut();
    let cid = env.contract_id;

    // Subtract base gas before the ACL check.
    env.subtract_gas(&mut store, MIN_GAS);

    // Enforce function ACL
    if let Err(e) = acl_allow(env, &[ContractSection::Deploy]) {
        error!(
            target: "runtime::db::zkas_db_set",
            "[WASM] [{cid}] zkas_db_set(): Called in unauthorized section: {e}",
        );
        return darkfi_sdk::error::CALLER_ACCESS_DENIED
    }

    // Get the wasm memory reader
    let mut buf_reader = match wasm_mem_read(env, &store, ptr, ptr_len) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::db::zkas_db_set",
                "[WASM] [{cid}] zkas_db_set(): Failed to read wasm memory: {e}",
            );
            return darkfi_sdk::error::DB_SET_FAILED
        }
    };

    // Deserialize the ZkBinary bytes from the buffer
    let zkbin_bytes: Vec<u8> = match Decodable::decode(&mut buf_reader) {
        Ok(zkbin) => zkbin,
        Err(e) => {
            error!(
                target: "runtime::db::zkas_db_set",
                "[WASM] [{cid}] zkas_db_set(): Could not deserialize bytes from buffer: {e}"
            );
            return darkfi_sdk::error::DB_SET_FAILED
        }
    };

    // Make sure there are no trailing bytes in the buffer.
    if buf_reader.position() != ptr_len as u64 {
        error!(
            target: "runtime::db::zkas_db_set",
            "[WASM] [{cid}] zkas_db_set(): Trailing bytes in argument stream",
        );
        return darkfi_sdk::error::DB_SET_FAILED
    }

    // Validate the bytes by decoding them into the ZkBinary format
    let zkbin = match ZkBinary::decode(&zkbin_bytes, false) {
        Ok(zkbin) => zkbin,
        Err(e) => {
            error!(
                target: "runtime::db::zkas_db_set",
                "[WASM] [{cid}] zkas_db_set(): Invalid zkas bincode passed to function: {e}"
            );
            return darkfi_sdk::error::DB_SET_FAILED
        }
    };

    // Because of `Runtime::Deploy`, we should be sure that the zkas db is index zero.
    let tree_handle = {
        let db_handles = env.db_handles.borrow();
        let db_handle = &db_handles[0];
        // Redundant check
        if db_handle.contract_id != cid {
            error!(
                target: "runtime::db::zkas_db_set",
                "[WASM] [{cid}] zkas_db_set(): Internal error, zkas db at index 0 incorrect"
            );
            return darkfi_sdk::error::DB_SET_FAILED
        }
        db_handle.tree
    };

    // Check if there is existing bincode and compare it. Return DB_SUCCESS if
    // they're the same.
    let is_new_key = match env
        .blockchain
        .lock()
        .unwrap()
        .overlay
        .lock()
        .unwrap()
        .get(&tree_handle, &serialize(&zkbin.namespace))
    {
        Ok(Some(bytes)) => {
            // We allow a panic here because this db should never be corrupted in this way.
            let (existing_zkbin, _): (Vec<u8>, Vec<u8>) =
                deserialize(&bytes).expect("deserialize tuple");

            if existing_zkbin == zkbin_bytes {
                debug!(
                    target: "runtime::db::zkas_db_set",
                    "[WASM] [{cid}] zkas_db_set(): Existing zkas bincode is the same. Skipping."
                );
                return wasm::entrypoint::SUCCESS
            }
            // Existing key, will overwrite.
            false
        }
        Ok(None) => true,
        Err(e) => {
            error!(
                target: "runtime::db::zkas_db_set",
                "[WASM] [{cid}] zkas_db_set(): Internal error getting from tree: {e}"
            );
            return darkfi_sdk::error::DB_SET_FAILED
        }
    };

    // Charge the per-row compile cost.
    let compile_gas =
        COMPILE_GAS_PER_ROW.saturating_mul(1u64.checked_shl(zkbin.k).unwrap_or(u64::MAX));
    env.subtract_gas(&mut store, compile_gas);

    // We didn't find any existing bincode, so let's create a new VerifyingKey and write it all.
    info!(
        target: "runtime::db::zkas_db_set",
        "[WASM] [{cid}] zkas_db_set(): Creating VerifyingKey for {} zkas circuit",
        zkbin.namespace,
    );

    let witnesses = match empty_witnesses(&zkbin) {
        Ok(w) => w,
        Err(e) => {
            error!(
                target: "runtime::db::zkas_db_set",
                "[WASM] [{cid}] zkas_db_set(): Failed to create empty witnesses: {e}"
            );
            return darkfi_sdk::error::DB_SET_FAILED
        }
    };

    // Construct the circuit and build the VerifyingKey
    let circuit = ZkCircuit::new(witnesses, &zkbin);
    let vk = VerifyingKey::build(zkbin.k, &circuit);
    let mut vk_buf = vec![];
    if let Err(e) = vk.write(&mut vk_buf) {
        error!(
            target: "runtime::db::zkas_db_set",
            "[WASM] [{cid}] zkas_db_set(): Failed to serialize VerifyingKey: {e}"
        );
        return darkfi_sdk::error::DB_SET_FAILED
    }

    // Insert the key-value pair into the database.
    let key = serialize(&zkbin.namespace);
    let value = serialize(&(zkbin_bytes, vk_buf));

    // Charge for bytes written. New keys also incur STATE_GROWTH_GAS.
    let bytes_written = (key.len() + value.len()) as u64;
    let mut storage_gas = bytes_written.saturating_mul(WRITE_GAS_PER_BYTE);
    if is_new_key {
        storage_gas = storage_gas.saturating_add(STATE_GROWTH_GAS);
    }
    env.subtract_gas(&mut store, storage_gas);

    if env
        .blockchain
        .lock()
        .unwrap()
        .overlay
        .lock()
        .unwrap()
        .insert(&tree_handle, &key, &value)
        .is_err()
    {
        error!(
            target: "runtime::db::zkas_db_set",
            "[WASM] [{cid}] zkas_db_set(): Couldn't insert to db_handle tree"
        );
        return darkfi_sdk::error::DB_SET_FAILED
    }

    wasm::entrypoint::SUCCESS
}
