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

use darkfi_serial::Encodable;

use crate::{crypto::MerkleNode, error::GenericResult, pasta::pallas, ContractError};

/// Check if the coins' local state contains a given Merkle root
///
/// Returns a boolean value.
pub fn coin_roots_contains(root: &MerkleNode) -> GenericResult<bool> {
    let mut buf = vec![];
    let mut len = 0;
    len += root.encode(&mut buf)?;

    let ret = unsafe { txlocal_coin_roots_contains_(buf.as_ptr(), len as u32) };

    if ret < 0 {
        return Err(ContractError::from(ret))
    }

    match ret {
        0 => Ok(false),
        1 => Ok(true),
        _ => unreachable!(),
    }
}

/// Check if new coins local state contains a given coin
///
/// Returns a boolean value.
pub fn new_coins_contains(coin: &pallas::Base) -> GenericResult<bool> {
    let mut buf = vec![];
    let mut len = 0;
    len += coin.encode(&mut buf)?;

    let ret = unsafe { txlocal_new_coins_contains_(buf.as_ptr(), len as u32) };

    if ret < 0 {
        return Err(ContractError::from(ret))
    }

    match ret {
        0 => Ok(false),
        1 => Ok(true),
        _ => unreachable!(),
    }
}

/// Append a coin to the transaction-local state
///
/// This will add it to the tx-local Merkle tree, marking the Merkle root.
pub fn append_coin(coin: &pallas::Base) -> GenericResult<()> {
    let mut buf = vec![];
    let mut len = 0;
    len += coin.encode(&mut buf)?;

    let ret = unsafe { txlocal_append_coin_(buf.as_ptr(), len as u32) };

    if ret < 0 {
        return Err(ContractError::from(ret))
    }

    Ok(())
}

extern "C" {
    fn txlocal_coin_roots_contains_(ptr: *const u8, len: u32) -> i64;
    fn txlocal_new_coins_contains_(ptr: *const u8, len: u32) -> i64;
    fn txlocal_append_coin_(ptr: *const u8, len: u32) -> i64;
}
