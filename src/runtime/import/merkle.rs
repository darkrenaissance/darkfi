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

use std::io::Cursor;

use darkfi_sdk::crypto::{MerkleNode, MerkleTree};
use darkfi_serial::{serialize, Decodable, Encodable, WriteExt};
use log::{debug, error};
use wasmer::{FunctionEnvMut, WasmPtr};

use crate::runtime::vm_runtime::{ContractSection, Env};

/// Adds data to merkle tree. The tree, database connection, and new data to add is
/// read from `ptr` at offset specified by `len`.
/// Returns `0` on success; otherwise, returns a negative error-code corresponding to a
/// [`ContractError`] (defined in the SDK).
/// See also the method `merkle_add` in `sdk/src/merkle.rs`.
pub(crate) fn merkle_add(ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, len: u32) -> i32 {
    let env = ctx.data();
    match env.contract_section {
        ContractSection::Update => {
            let memory_view = env.memory_view(&ctx);

            let Ok(mem_slice) = ptr.slice(&memory_view, len) else {
                error!(target: "runtime::merkle", "Failed to make slice from ptr");
                return -2
            };

            let mut buf = vec![0_u8; len as usize];
            if let Err(e) = mem_slice.read_slice(&mut buf) {
                error!(target: "runtime::merkle", "Failed to read from memory slice: {}", e);
                return -2
            };

            // The buffer should deserialize into:
            // - db_info
            // - db_roots
            // - root_key (as Vec<u8>) (key being the name of the sled key in info_db where the latest root is)
            // - tree_key (as Vec<u8>) (key being the name of the sled key in info_db where the Merkle tree is)
            // - coins (as Vec<MerkleNode>) (the coins being added into the Merkle tree)
            let mut buf_reader = Cursor::new(buf);
            // FIXME: There's a type DbHandle=u32, but this should maybe be renamed
            let db_info_index: u32 = match Decodable::decode(&mut buf_reader) {
                Ok(v) => v,
                Err(e) => {
                    error!(target: "runtime::merkle", "Failed to decode db_info DbHandle: {}", e);
                    return -2
                }
            };
            let db_info_index = db_info_index as usize;

            let db_roots_index: u32 = match Decodable::decode(&mut buf_reader) {
                Ok(v) => v,
                Err(e) => {
                    error!(target: "runtime::merkle", "Failed to decode db_roots DbHandle: {}", e);
                    return -2
                }
            };
            let db_roots_index = db_roots_index as usize;

            let db_handles = env.db_handles.borrow();
            let n_dbs = db_handles.len();

            if n_dbs <= db_info_index || n_dbs <= db_roots_index {
                error!(target: "runtime::merkle", "Requested DbHandle that is out of bounds");
                return -2
            }
            let db_info = &db_handles[db_info_index];
            let db_roots = &db_handles[db_roots_index];

            if db_info.contract_id != env.contract_id || db_roots.contract_id != env.contract_id {
                error!(target: "runtime::merkle", "Unauthorized to write to DbHandle");
                return -2
            }

            // This `key` represents the sled key in info where the latest root is
            let root_key: Vec<u8> = match Decodable::decode(&mut buf_reader) {
                Ok(v) => v,
                Err(e) => {
                    error!(target: "runtime::merkle", "Failed to decode key vec: {}", e);
                    return -2
                }
            };

            // This `key` represents the sled database tree name
            let tree_key: Vec<u8> = match Decodable::decode(&mut buf_reader) {
                Ok(v) => v,
                Err(e) => {
                    error!(target: "runtime::merkle", "Failed to decode key vec: {}", e);
                    return -2
                }
            };

            // This `coin` represents the leaf we're adding to the Merkle tree
            let coins: Vec<MerkleNode> = match Decodable::decode(&mut buf_reader) {
                Ok(v) => v,
                Err(e) => {
                    error!(target: "runtime::merkle", "Failed to decode MerkleNode: {}", e);
                    return -2
                }
            };

            // TODO: Ensure we've read the entire buffer above.

            // Read the current tree
            let ret = match env
                .blockchain
                .lock()
                .unwrap()
                .overlay
                .lock()
                .unwrap()
                .get(&db_info.tree, &tree_key)
            {
                Ok(v) => v,
                Err(e) => {
                    error!(target: "runtime::merkle", "Internal error getting from tree: {}", e);
                    return -2
                }
            };

            let Some(return_data) = ret else {
                error!(target: "runtime::merkle", "Return data is empty");
                return -2
            };

            debug!(
                target: "runtime::merkle",
                "Serialized tree: {} bytes",
                return_data.len()
            );
            debug!(
                target: "runtime::merkle",
                "                 {:02x?}",
                return_data
            );

            let mut decoder = Cursor::new(&return_data);

            let set_size: u32 = match Decodable::decode(&mut decoder) {
                Ok(v) => v,
                Err(e) => {
                    error!(target: "runtime::merkle", "Unable to read set size: {}", e);
                    return -2
                }
            };

            let mut tree: MerkleTree = match Decodable::decode(&mut decoder) {
                Ok(v) => v,
                Err(e) => {
                    error!(target: "runtime::merkle", "Unable to deserialize tree: {}", e);
                    return -2
                }
            };

            // Here we add the new coins into the tree.
            let mut new_roots = vec![];

            for coin in coins {
                tree.append(coin);
                let Some(root) = tree.root(0) else {
                    error!(target: "runtime::merkle", "Unable to read the root of tree");
                    return -2
                };
                new_roots.push(root);
            }

            // And we serialize the tree back to bytes
            let mut tree_data = Vec::new();
            if tree_data.write_u32(set_size + new_roots.len() as u32).is_err() ||
                tree.encode(&mut tree_data).is_err()
            {
                error!(target: "runtime::merkle", "Couldn't reserialize modified tree");
                return -2
            }

            // Apply changes to overlay
            let lock = env.blockchain.lock().unwrap();
            let mut overlay = lock.overlay.lock().unwrap();
            if overlay.insert(&db_info.tree, &tree_key, &tree_data).is_err() {
                error!(target: "runtime::merkle", "Couldn't insert to db_info tree");
                return -2
            }

            // Here we add the Merkle root to our set of roots
            // TODO: We should probably make sure that this root isn't in the set
            for root in new_roots.iter() {
                // FIXME: Why were we writing the set size here?
                //let root_index: Vec<u8> = serialize(&(set_size as u32));
                //assert_eq!(root_index.len(), 4);
                debug!(target: "runtime::merkle", "Appending Merkle root to db: {:?}", root);
                let root_value: Vec<u8> = serialize(root);
                // FIXME: This assert can be used to DoS nodes from contracts
                assert_eq!(root_value.len(), 32);
                if overlay.insert(&db_roots.tree, &root_value, &[]).is_err() {
                    error!(target: "runtime::merkle", "Couldn't insert to db_roots tree");
                    return -2
                }
            }

            // Write a pointer to the latest known root
            if !new_roots.is_empty() {
                debug!(target: "runtime::merkle", "Replacing latest Merkle root pointer");
                let latest_root = serialize(new_roots.last().unwrap());
                if overlay.insert(&db_info.tree, &root_key, &latest_root).is_err() {
                    error!(target: "runtime::merkle", "Couldn't insert latest root to db_info tree");
                    return -2
                }
            }

            0
        }
        _ => -1,
    }
}
