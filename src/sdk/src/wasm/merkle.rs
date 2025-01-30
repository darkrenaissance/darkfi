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

use darkfi_serial::Encodable;

use crate::{
    crypto::MerkleNode,
    error::{ContractError, GenericResult},
    pasta::pallas,
    wasm::db::DbHandle,
};

/// Add given elements into a Merkle tree. Used for inclusion proofs.
///
/// * `db_info` is a handle for a database where the Merkle tree is stored.
/// * `db_roots` is a handle for a database where all the new Merkle roots are stored.
/// * `root_key` is the serialized key pointing to the latest Merkle root in `db_info`
/// * `tree_key` is the serialized key pointing to the Merkle tree in `db_info`.
/// * `elements` are the items we want to add to the Merkle tree.
///
/// There are 2 databases:
///
/// * `db_info` stores general metadata or info.
/// * `db_roots` stores a log of all the merkle roots.
///
/// Inside `db_info` we store:
///
/// * The [latest root hash:32] under `root_key`.
/// * The incremental merkle tree under `tree_key`.
///
/// Inside `db_roots` we store:
///
/// * All [merkle root:32]s as keys. The value is the current [tx_hash:32][call_idx:1].
///   If no new values are added, then the root key is updated to the current (tx_hash, call_idx).
pub fn merkle_add(
    db_info: DbHandle,
    db_roots: DbHandle,
    root_key: &[u8],
    tree_key: &[u8],
    elements: &[MerkleNode],
) -> GenericResult<()> {
    let mut buf = vec![];
    let mut len = 0;
    len += db_info.encode(&mut buf)?;
    len += db_roots.encode(&mut buf)?;
    len += root_key.to_vec().encode(&mut buf)?;
    len += tree_key.to_vec().encode(&mut buf)?;
    len += elements.to_vec().encode(&mut buf)?;

    match unsafe { merkle_add_(buf.as_ptr(), len as u32) } {
        0 => Ok(()),
        -1 => Err(ContractError::CallerAccessDenied),
        -2 => Err(ContractError::DbSetFailed),
        _ => unreachable!(),
    }
}

/// Add given elements into a sparse Merkle tree. Used for exclusion proofs.
///
/// * `db_info` is a handle for a database where the latest root is stored.
/// * `db_smt` is a handle for a database where all the actual tree is stored.
/// * `db_roots` is a handle for a database where all the new roots are stored.
/// * `root_key` is the serialized key pointing to the latest Merkle root in `db_info`
/// * `elements` are the items we want to add to the tree.
///
/// There are 2 databases:
///
/// * `db_info` stores general metadata or info.
/// * `db_roots` stores a log of all the merkle roots.
///
/// Inside `db_info` we store:
///
/// * The [latest root hash:32] under `root_key`.
///
/// Inside `db_roots` we store:
///
/// * All [merkle root:32]s as keys. The value is the current [tx_hash:32][call_idx:1].
///   If no new values are added, then the root key is updated to the current (tx_hash, call_idx).
pub fn sparse_merkle_insert_batch(
    db_info: DbHandle,
    db_smt: DbHandle,
    db_roots: DbHandle,
    root_key: &[u8],
    elements: &[pallas::Base],
) -> GenericResult<()> {
    let mut buf = vec![];
    let mut len = 0;
    len += db_info.encode(&mut buf)?;
    len += db_smt.encode(&mut buf)?;
    len += db_roots.encode(&mut buf)?;
    len += root_key.to_vec().encode(&mut buf)?;
    len += elements.to_vec().encode(&mut buf)?;

    match unsafe { sparse_merkle_insert_batch_(buf.as_ptr(), len as u32) } {
        0 => Ok(()),
        -1 => Err(ContractError::CallerAccessDenied),
        -2 => Err(ContractError::DbSetFailed),
        _ => unreachable!(),
    }
}

extern "C" {
    fn merkle_add_(ptr: *const u8, len: u32) -> i64;
    fn sparse_merkle_insert_batch_(ptr: *const u8, len: u32) -> i64;
}
