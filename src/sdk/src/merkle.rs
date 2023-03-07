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

use darkfi_serial::Encodable;

use super::{
    crypto::MerkleNode,
    db::DbHandle,
    error::{ContractError, GenericResult},
};

/// Add given elements into a Merkle tree.
/// * `db_info` is a handle for a database where the Merkle tree is stored.
/// * `db_roots` is a handle for a database where all the new Merkle roots are stored.
/// * `key` is the serialized key for `db_info`.
/// * `elements` are the items we want to add to the Merkle tree.
pub fn merkle_add(
    db_info: DbHandle,
    db_roots: DbHandle,
    key: &[u8],
    elements: &[MerkleNode],
) -> GenericResult<()> {
    let mut buf = vec![];
    let mut len = 0;
    len += db_info.encode(&mut buf)?;
    len += db_roots.encode(&mut buf)?;
    len += key.to_vec().encode(&mut buf)?;
    len += elements.to_vec().encode(&mut buf)?;

    match unsafe { merkle_add_(buf.as_ptr(), len as u32) } {
        0 => Ok(()),
        -1 => Err(ContractError::CallerAccessDenied),
        -2 => Err(ContractError::DbSetFailed),
        _ => unreachable!(),
    }
}

extern "C" {
    fn merkle_add_(ptr: *const u8, len: u32) -> i32;
}
