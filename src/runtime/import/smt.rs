/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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

use darkfi_sdk::{
    crypto::{
        pasta_prelude::*,
        smt::{PoseidonFp, SparseMerkleTree, StorageAdapter, EMPTY_NODES_FP, SMT_FP_DEPTH},
    },
    error::{ContractError, ContractResult},
    wasm,
};
use darkfi_serial::{serialize, Decodable, Encodable};
use halo2_proofs::pasta::pallas;
use log::{debug, error};
use num_bigint::BigUint;
use wasmer::{FunctionEnvMut, WasmPtr};

use super::acl::acl_allow;
use crate::runtime::vm_runtime::{ContractSection, Env};

/// An SMT adapter for sled overlay storage. Compatible with the WasmDb SMT adapter
pub struct SledStorage<'a> {
    overlay: &'a mut sled_overlay::SledDbOverlay,
    tree_key: &'a [u8],
}

impl<'a> StorageAdapter for SledStorage<'a> {
    type Value = pallas::Base;

    fn put(&mut self, key: BigUint, value: pallas::Base) -> ContractResult {
        if let Err(e) = self.overlay.insert(self.tree_key, &key.to_bytes_le(), &value.to_repr()) {
            error!(
                target: "runtime::smt::SledStorage::put",
                "[WASM] SledStorage::put(): inserting key {:?}, value {:?} into DB tree: {:?}: {}",
                key, value, self.tree_key, e,
            );
            return Err(ContractError::SmtPutFailed)
        }

        Ok(())
    }

    fn get(&self, key: &BigUint) -> Option<pallas::Base> {
        let value = match self.overlay.get(self.tree_key, &key.to_bytes_le()) {
            Ok(v) => v,
            Err(e) => {
                error!(
                    target: "runtime::smt::SledStorage::get",
                    "[WASM] SledStorage::get(): Fetching key {:?} from DB tree: {:?}: {}",
                    key, self.tree_key, e,
                );
                return None
            }
        };

        let value = value?;
        let mut repr = [0; 32];
        repr.copy_from_slice(&value);

        pallas::Base::from_repr(repr).into()
    }

    fn del(&mut self, key: &BigUint) -> ContractResult {
        if let Err(e) = self.overlay.remove(self.tree_key, &key.to_bytes_le()) {
            error!(
                target: "runtime::smt::SledStorage::del",
                "[WASM] SledStorage::del(): Removing key {:?} from DB tree: {:?}: {}",
                key, self.tree_key, e,
            );
            return Err(ContractError::SmtDelFailed)
        }

        Ok(())
    }
}

/// Adds data to sparse merkle tree. The tree, database connection, and new data to add is
/// read from `ptr` at offset specified by `len`.
/// Returns `0` on success; otherwise, returns an error-code corresponding to a
/// [`ContractError`] (defined in the SDK).
/// See also the method `merkle_add` in `sdk/src/merkle.rs`.
///
/// Permissions: update
pub(crate) fn sparse_merkle_insert_batch(
    mut ctx: FunctionEnvMut<Env>,
    ptr: WasmPtr<u8>,
    len: u32,
) -> i64 {
    let (env, mut store) = ctx.data_and_store_mut();
    let cid = env.contract_id;

    // Enforce function ACL
    if let Err(e) = acl_allow(env, &[ContractSection::Update]) {
        error!(
            target: "runtime::smt::sparse_merkle_insert_batch",
            "[WASM] [{}] sparse_merkle_insert_batch(): Called in unauthorized section: {}", cid, e,
        );
        return darkfi_sdk::error::CALLER_ACCESS_DENIED
    }

    // Subtract used gas. Here we count the length read from the memory slice.
    // This makes calling the function which returns early have some (small) cost.
    env.subtract_gas(&mut store, len as u64);

    let memory_view = env.memory_view(&store);
    let Ok(mem_slice) = ptr.slice(&memory_view, len) else {
        error!(
            target: "runtime::smt::sparse_merkle_insert_batch",
            "[WASM] [{}] sparse_merkle_insert_batch(): Failed to make slice from ptr", cid,
        );
        return darkfi_sdk::error::INTERNAL_ERROR
    };

    let mut buf = vec![0_u8; len as usize];
    if let Err(e) = mem_slice.read_slice(&mut buf) {
        error!(
            target: "runtime::smt::sparse_merkle_insert_batch",
            "[WASM] [{}] sparse_merkle_insert_batch(): Failed to read from memory slice: {}", cid, e,
        );
        return darkfi_sdk::error::INTERNAL_ERROR
    };

    // The buffer should deserialize into:
    // - db_smt
    // - db_roots
    // - nullifiers (as Vec<pallas::Base>)
    let mut buf_reader = Cursor::new(buf);
    let db_info_index: u32 = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::smt::sparse_merkle_insert_batch",
                "[WASM] [{}] sparse_merkle_insert_batch(): Failed to decode db_info DbHandle: {}", cid, e,
            );
            return darkfi_sdk::error::INTERNAL_ERROR
        }
    };
    let db_info_index = db_info_index as usize;

    let db_smt_index: u32 = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
            target: "runtime::smt::sparse_merkle_insert_batch",
                "[WASM] [{}] sparse_merkle_insert_batch(): Failed to decode db_smt DbHandle: {}", cid, e,
            );
            return darkfi_sdk::error::INTERNAL_ERROR
        }
    };
    let db_smt_index = db_smt_index as usize;

    let db_roots_index: u32 = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::smt::sparse_merkle_insert_batch",
                "[WASM] [{}] sparse_merkle_insert_batch(): Failed to decode db_roots DbHandle: {}", cid, e,
            );
            return darkfi_sdk::error::INTERNAL_ERROR
        }
    };
    let db_roots_index = db_roots_index as usize;

    let db_handles = env.db_handles.borrow();
    let n_dbs = db_handles.len();

    if n_dbs <= db_info_index || n_dbs <= db_smt_index || n_dbs <= db_roots_index {
        error!(
            target: "runtime::smt::sparse_merkle_insert_batch",
            "[WASM] [{}] sparse_merkle_insert_batch(): Requested DbHandle that is out of bounds", cid,
        );
        return darkfi_sdk::error::INTERNAL_ERROR
    }
    let db_info = &db_handles[db_info_index];
    let db_smt = &db_handles[db_smt_index];
    let db_roots = &db_handles[db_roots_index];

    // Make sure that the contract owns the dbs it wants to write to
    if db_info.contract_id != env.contract_id ||
        db_smt.contract_id != env.contract_id ||
        db_roots.contract_id != env.contract_id
    {
        error!(
            target: "runtime::smt::sparse_merkle_insert_batch",
            "[WASM] [{}] sparse_merkle_insert_batch(): Unauthorized to write to DbHandle", cid,
        );
        return darkfi_sdk::error::CALLER_ACCESS_DENIED
    }

    // This `key` represents the sled key in info where the latest root is
    let root_key: Vec<u8> = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::smt::sparse_merkle_insert_batch",
                "[WASM] [{}] sparse_merkle_insert_batch(): Failed to decode key vec: {}", cid, e,
            );
            return darkfi_sdk::error::INTERNAL_ERROR
        }
    };

    // This `nullifier` represents the leaf we're adding to the Merkle tree
    let nullifiers: Vec<pallas::Base> = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::smt::sparse_merkle_insert_batch",
                "[WASM] [{}] sparse_merkle_insert_batch(): Failed to decode pallas::Base: {}", cid, e,
            );
            return darkfi_sdk::error::INTERNAL_ERROR
        }
    };

    // Make sure we've read the entire buffer
    if buf_reader.position() != (len as u64) {
        error!(
            target: "runtime::smt::sparse_merkle_insert_batch",
            "[WASM] [{}] sparse_merkle_insert_batch(): Mismatch between given length, and cursor length", cid,
        );
        return darkfi_sdk::error::INTERNAL_ERROR
    }

    let hasher = PoseidonFp::new();
    let leaves: Vec<_> = nullifiers.into_iter().map(|x| (x, x)).collect();
    // Used in gas calc
    let leaves_len = leaves.len();

    let lock = env.blockchain.lock().unwrap();
    let mut overlay = lock.overlay.lock().unwrap();
    let smt_store = SledStorage { overlay: &mut overlay, tree_key: &db_smt.tree };

    let mut smt = SparseMerkleTree::<
        SMT_FP_DEPTH,
        { SMT_FP_DEPTH + 1 },
        pallas::Base,
        PoseidonFp,
        SledStorage,
    >::new(smt_store, hasher, &EMPTY_NODES_FP);
    if let Err(e) = smt.insert_batch(leaves) {
        error!(
            target: "runtime::smt::sparse_merkle_insert_batch",
            "[WASM] [{}] sparse_merkle_insert_batch(): SMT failed to insert batch: {}", cid, e,
        );
        return darkfi_sdk::error::INTERNAL_ERROR
    };

    // Here we add the SMT root to our set of roots
    // Since each update to the tree is atomic, we only need to add the last root.
    let latest_root = smt.root();

    debug!(
        target: "runtime::smt::sparse_merkle_insert_batch",
        "[WASM] [{}] sparse_merkle_insert_batch(): Appending SMT root to db: {:?}", cid, latest_root,
    );
    let latest_root_data = serialize(&latest_root);
    assert_eq!(latest_root_data.len(), 32);

    let mut value_data = Vec::with_capacity(32 + 1);
    env.tx_hash.inner().encode(&mut value_data).expect("Unable to serialize tx_hash");
    env.call_idx.encode(&mut value_data).expect("Unable to serialize call_idx");
    assert_eq!(value_data.len(), 32 + 1);

    if overlay.insert(&db_roots.tree, &latest_root_data, &value_data).is_err() {
        error!(
            target: "runtime::smt::sparse_merkle_insert_batch",
            "[WASM] [{}] sparse_merkle_insert_batch(): Couldn't insert to db_roots tree", cid,
        );
        return darkfi_sdk::error::INTERNAL_ERROR
    }

    // Write a pointer to the latest known root
    debug!(
        target: "runtime::smt::sparse_merkle_insert_batch",
        "[WASM] [{}] sparse_merkle_insert_batch(): Replacing latest SMT root pointer", cid,
    );

    if overlay.insert(&db_info.tree, &root_key, &latest_root_data).is_err() {
        error!(
            target: "runtime::smt::sparse_merkle_insert_batch",
            "[WASM] [{}] sparse_merkle_insert_batch(): Couldn't insert latest root to db_info tree", cid,
        );
        return darkfi_sdk::error::INTERNAL_ERROR
    }

    // Subtract used gas.
    // Here we count:
    // * The number of nullifiers we inserted into the DB
    drop(overlay);
    drop(lock);
    drop(db_handles);
    let spent_gas = leaves_len * 32;
    env.subtract_gas(&mut store, spent_gas as u64);

    wasm::entrypoint::SUCCESS
}
