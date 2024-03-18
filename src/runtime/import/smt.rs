use std::io::Cursor;

use darkfi_sdk::crypto::{
    pasta_prelude::*,
    smt::{PoseidonFp, SparseMerkleTree, StorageAdapter, EMPTY_NODES_FP, SMT_FP_DEPTH},
};
use darkfi_serial::Decodable;
use halo2_proofs::pasta::pallas;
use log::error;
use num_bigint::BigUint;
use wasmer::{FunctionEnvMut, WasmPtr};

use super::acl::acl_allow;
use crate::runtime::vm_runtime::{ContractSection, Env};

pub struct SledStorage<'a> {
    overlay: &'a mut sled_overlay::SledDbOverlay,
    tree_key: &'a [u8],
}

impl<'a> StorageAdapter for SledStorage<'a> {
    type Value = pallas::Base;

    fn put(&mut self, key: BigUint, value: pallas::Base) -> bool {
        if self.overlay.insert(self.tree_key, &key.to_bytes_le(), &value.to_repr()).is_err() {
            error!(
                target: "runtime::smt::SledStorage::put",
                "[WASM] sparse_merkle_insert_batch(): inserting key {:?}, value {:?} into DB tree: {:?}",
                key, value, self.tree_key
            );
            return false
        }
        true
    }
    fn get(&self, key: &BigUint) -> Option<pallas::Base> {
        let Ok(value) = self.overlay.get(self.tree_key, &key.to_bytes_le()) else {
            error!(
                target: "runtime::smt::SledStorage::get",
                "[WASM] sparse_merkle_insert_batch(): fetching key {:?} from DB tree: {:?}",
                key, self.tree_key
            );
            return None
        };
        let Some(value) = value else { return None };
        let mut repr = [0; 32];
        repr.copy_from_slice(&value);
        let value = pallas::Base::from_repr(repr);
        if value.is_none().into() {
            None
        } else {
            Some(value.unwrap())
        }
    }
}

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
    // - nullifiers (as Vec<pallas::Base>)
    let mut buf_reader = Cursor::new(buf);
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

    let db_handles = env.db_handles.borrow();
    let n_dbs = db_handles.len();

    if n_dbs <= db_smt_index {
        error!(
            target: "runtime::smt::sparse_merkle_insert_batch",
            "[WASM] [{}] sparse_merkle_insert_batch(): Requested DbHandle that is out of bounds", cid,
        );
        return darkfi_sdk::error::INTERNAL_ERROR
    }
    let db_smt = &db_handles[db_smt_index];

    // Make sure that the contract owns the dbs it wants to write to
    if db_smt.contract_id != env.contract_id {
        error!(
            target: "runtime::smt::sparse_merkle_insert_batch",
            "[WASM] [{}] sparse_merkle_insert_batch(): Unauthorized to write to DbHandle", cid,
        );
        return darkfi_sdk::error::CALLER_ACCESS_DENIED
    }

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

    // Subtract used gas.
    // Here we count:
    // * The number of nullifiers we inserted into the DB
    drop(overlay);
    drop(lock);
    drop(db_handles);
    let spent_gas = leaves_len * 32;
    env.subtract_gas(&mut store, spent_gas as u64);

    darkfi_sdk::entrypoint::SUCCESS
}
