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

use darkfi_sdk::{
    crypto::{MerkleNode, MerkleTree},
    hex::AsHex,
    wasm,
};
use darkfi_serial::{serialize, Decodable, Encodable, WriteExt};
use tracing::{debug, error};
use wasmer::{FunctionEnvMut, WasmPtr};

use crate::runtime::{
    import::{acl::acl_allow, util::wasm_mem_read},
    vm_runtime::{ContractSection, Env},
};

/// Add data to an on-chain Merkle tree.
///
/// Expects:
/// * `db_info`: Handle where the Merkle tree is stored
/// * `db_roots`: Handle where all new Merkle roots are stored
/// * `root_key`: Serialized key pointing to latest root in `db_info`
/// * `tree_key`: Serialized key pointing to the Merkle tree in `db_info`
/// * `coins`: Items we want to add to the Merkle tree
///
/// ## Permissions
/// * `ContractSection::Update`
pub(crate) fn merkle_add(ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, ptr_len: u32) -> i64 {
    merkle_add_internal(ctx, ptr, ptr_len, false)
}

/// Add data to a tx-local Merkle tree.
///
/// Expects:
/// * `db_info`: Handle where the Merkle tree is stored
/// * `db_roots`: Handle where all new Merkle roots are stored
/// * `root_key`: Serialized key pointing to latest root in `db_info`
/// * `tree_key`: Serialized key pointing to the Merkle tree in `db_info`
/// * `coins`: Items we want to add to the Merkle tree
///
/// ## Permissions
/// * `ContractSection::Update`
pub(crate) fn merkle_add_local(ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, ptr_len: u32) -> i64 {
    merkle_add_internal(ctx, ptr, ptr_len, true)
}

/// Internal function for `merkle_add` which branches to either on-chain
/// or transaction-local.
pub(crate) fn merkle_add_internal(
    mut ctx: FunctionEnvMut<Env>,
    ptr: WasmPtr<u8>,
    ptr_len: u32,
    local: bool,
) -> i64 {
    let lt = if local { "merkle_add_local" } else { "merkle_add" };
    let (env, mut store) = ctx.data_and_store_mut();
    let cid = env.contract_id;

    // Enforce function ACL
    if let Err(e) = acl_allow(env, &[ContractSection::Update]) {
        error!(
            target: "runtime::merkle::{lt}",
            "[WASM] [{cid}] {lt}(): Called in unauthorized section: {e}",
        );
        return darkfi_sdk::error::CALLER_ACCESS_DENIED
    }

    // Subtract used gas. 1 for opcode, 33 for value_data.len().
    env.subtract_gas(&mut store, 34);

    // Get the wasm memory reader
    let mut buf_reader = match wasm_mem_read(env, &store, ptr, ptr_len) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::merkle::{lt}",
                "[WASM] [{cid}] {lt}(): Failed to read wasm memory: {e}",
            );
            return darkfi_sdk::error::INTERNAL_ERROR
        }
    };

    // The buffer should deserialize intto:
    // - db_info (DbHandle)
    // - db_roots (DbHandle)
    // - root_key (Vec<u8>)
    // - tree_key (Vec<u8>)
    // - coins (Vec<MerkleNode>)

    let db_info_index: u32 = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::merkle::{lt}",
                "[WASM] [{cid}] {lt}(): Failed to decode db_info DbHandle: {e}",
            );
            return darkfi_sdk::error::INTERNAL_ERROR
        }
    };

    let db_roots_index: u32 = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::merkle::{lt}",
                "[WASM] [{cid}] {lt}(): Failed to decode db_roots DbHandle: {e}",
            );
            return darkfi_sdk::error::INTERNAL_ERROR
        }
    };

    // Fetch the required db handles
    let db_info_index = db_info_index as usize;
    let db_roots_index = db_roots_index as usize;
    let db_handles = if local { env.local_db_handles.borrow() } else { env.db_handles.borrow() };
    let n_dbs = db_handles.len();

    if n_dbs <= db_info_index || n_dbs <= db_roots_index {
        error!(
            target: "runtime::merkle::{lt}",
            "[WASM] [{cid}] {lt}(): Requested DbHandle that is out of bounds",
        );
        return darkfi_sdk::error::INTERNAL_ERROR
    }

    let db_info = &db_handles[db_info_index];
    let db_roots = &db_handles[db_roots_index];

    // Make sure that the contract owns the dbs it wants to write to
    if db_info.contract_id != cid || db_roots.contract_id != cid {
        error!(
            target: "runtime::merkle::{lt}",
            "[WASM] [{cid}] {lt}(): Unauthorized write to DbHandle",
        );
        return darkfi_sdk::error::CALLER_ACCESS_DENIED
    }

    // This key represents the key in db_info where the latest root is
    let root_key: Vec<u8> = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::merkle::{lt}",
                "[WASM] [{cid}] {lt}(): Failed to decode root_key Vec: {e}",
            );
            return darkfi_sdk::error::INTERNAL_ERROR
        }
    };

    // This key represents the key in db_info where the Merkle tree is
    let tree_key: Vec<u8> = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::merkle::{lt}",
                "[WASM] [{cid}] {lt}(): Failed to decode tree_key Vec: {e}",
            );
            return darkfi_sdk::error::INTERNAL_ERROR
        }
    };

    // Coins represent the leaf(s) we're adding to the Merkle tree
    let coins: Vec<MerkleNode> = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::merkle::{lt}",
                "[WASM] [{cid}] {lt}(): Failed to decode Vec<MerkleNode>: {e}",
            );
            return darkfi_sdk::error::INTERNAL_ERROR
        }
    };

    // Make sure we've read the entire buffer
    if buf_reader.position() != ptr_len as u64 {
        error!(
            target: "runtime::merkle::{lt}",
            "[WASM] [{cid}] {lt}(): Trailing bytes in argument stream",
        );
        return darkfi_sdk::error::INTERNAL_ERROR
    }

    // Even with tx-local, we will lock the blockchain db to make sure
    // it does not change for any reason during this execution.
    let blockchain = env.blockchain.lock().unwrap();
    let mut overlay = blockchain.overlay.lock().unwrap();
    let mut tx_local_db = env.tx_local.lock();

    // Read the current Merkle tree.
    let tree_bytes = if local {
        let Some(db_cid) = tx_local_db.get(&db_info.contract_id) else {
            error!(
                target: "runtime::db::{lt}",
                "[WASM] [{cid}] {lt}(): Could not find db for {}",
                db_info.contract_id,
            );
            return darkfi_sdk::error::INTERNAL_ERROR
        };

        let Some(tree) = db_cid.get(&db_info.tree) else {
            error!(
                target: "runtime::db::{lt}",
                "[WASM] [{cid}] {lt}(): Could not find db tree for {}",
                db_info.contract_id,
            );
            return darkfi_sdk::error::INTERNAL_ERROR
        };

        tree.get(&tree_key).cloned()
    } else {
        match overlay.get(&db_info.tree, &tree_key) {
            Ok(v) => v.map(|iv| iv.to_vec()),
            Err(e) => {
                error!(
                    target: "runtime::merkle::{lt}",
                    "[WASM] [{cid}] {lt}(): Error getting from sled tree: {e}",
                );
                return darkfi_sdk::error::INTERNAL_ERROR
            }
        }
    };

    let Some(tree_bytes) = tree_bytes else {
        error!(
            target: "runtime::merkle::{lt}",
            "[WASM] [{cid}] {lt}(): Merkle tree k/v is empty",
        );
        return darkfi_sdk::error::INTERNAL_ERROR
    };

    // Deserialize the tree
    debug!(target: "runtime::merkle::{lt}", "Serialized tree: {} bytes", tree_bytes.len());
    debug!(target: "runtime::merkle::{lt}", "{}", tree_bytes.hex());

    let mut decoder = Cursor::new(&tree_bytes);
    let set_size: u32 = match Decodable::decode(&mut decoder) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::merkle::{lt}",
                "[WASM] [{cid}] {lt}(): Unable to decode set size: {e}",
            );
            return darkfi_sdk::error::INTERNAL_ERROR
        }
    };

    let mut merkle_tree: MerkleTree = match Decodable::decode(&mut decoder) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::merkle::{lt}",
                "[WASM] [{cid}] {lt}(): Unable to deserialize Merkle tree: {e}",
            );
            return darkfi_sdk::error::INTERNAL_ERROR
        }
    };

    // Here we add the new coins into the tree.
    let coins_len = coins.len();
    for coin in coins {
        merkle_tree.append(coin);
    }

    // And we serialize the tree back to bytes
    let mut merkle_tree_data = vec![];
    if merkle_tree_data.write_u32(set_size + coins_len as u32).is_err() ||
        merkle_tree.encode(&mut merkle_tree_data).is_err()
    {
        error!(
            target: "runtime::merkle::{lt}",
            "[WASM] [{cid}] {lt}(): Could not serialize modified Merkle tree",
        );
        return darkfi_sdk::error::INTERNAL_ERROR
    }

    // Apply changes
    if local {
        // We unwrap here because we already know the databases exist
        // from when we fetched the tree.
        let db_cid = tx_local_db.get_mut(&db_info.contract_id).unwrap();
        let tree = db_cid.get_mut(&db_info.tree).unwrap();
        tree.insert(tree_key, merkle_tree_data);
    } else if let Err(e) = overlay.insert(&db_info.tree, &tree_key, &merkle_tree_data) {
        error!(
            target: "runtime::merkle::{lt}",
            "[WASM] [{cid}] {lt}(): Could not insert tree to db_info: {e}",
        );
        return darkfi_sdk::error::INTERNAL_ERROR
    }

    // Here we add the Merkle root to our set of roots
    // Since each update to the tree is atomic, we only need to add the last
    // known root.
    let Some(latest_root) = merkle_tree.root(0) else {
        error!(
            target: "runtime::merkle::{lt}",
            "[WASM] [{cid}] {lt}(): Unable to read Merkle tree root",
        );
        return darkfi_sdk::error::INTERNAL_ERROR
    };

    debug!(
        target: "runtime::merkle::{lt}",
        "[WASM] [{cid}] {lt}(): Appending Merkle root to db: {latest_root:?}",
    );

    let latest_root_data = serialize(&latest_root);
    assert_eq!(latest_root_data.len(), 32);

    let mut value_data = Vec::with_capacity(32 + 1);
    env.tx_hash.inner().encode(&mut value_data).expect("Unable to serialize tx_hash");
    env.call_idx.encode(&mut value_data).expect("Unable to serialize call_idx");
    assert_eq!(value_data.len(), 32 + 1);

    if local {
        // We unwrap here because we already know the databases exist
        // from when we fetched the tree.
        let db_cid = tx_local_db.get_mut(&db_info.contract_id).unwrap();

        let info_tree = db_cid.get_mut(&db_info.tree).unwrap();
        info_tree.insert(root_key, latest_root_data.clone());

        let roots_tree = db_cid.get_mut(&db_roots.tree).unwrap();
        roots_tree.insert(latest_root_data, value_data);
    } else {
        if let Err(e) = overlay.insert(&db_roots.tree, &latest_root_data, &value_data) {
            error!(
                target: "runtime::merkle::{lt}",
                "[WASM] [{cid}] {lt}(): Could not insert to db_roots tree: {e}",
            );
            return darkfi_sdk::error::INTERNAL_ERROR
        }

        if let Err(e) = overlay.insert(&db_info.tree, &root_key, &latest_root_data) {
            error!(
                target: "runtime::merkle::{lt}",
                "[WASM] [{cid}] {lt}(): Could not insert latest root to db_info: {e}",
            );
            return darkfi_sdk::error::INTERNAL_ERROR
        }
    }

    // Subtract used gas.
    drop(tx_local_db);
    drop(overlay);
    drop(blockchain);
    drop(db_handles);
    let spent_gas = coins_len * 32;
    env.subtract_gas(&mut store, spent_gas as u64);

    wasm::entrypoint::SUCCESS
}
