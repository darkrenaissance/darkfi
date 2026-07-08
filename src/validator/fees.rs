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

use darkfi_serial::{async_trait, SerialDecodable, SerialEncodable};

use crate::zkas::ZkBinary;

/// Fixed fee for verifying a Schnorr signature over the Pallas curve.
pub const PALLAS_SCHNORR_VERIFY_GAS: u64 = 1850;

/// Base gas subtracted at the start of every host function call.
pub const MIN_GAS: u64 = 1;

/// Per-byte multiplier for on-chain storage reads.
pub const READ_GAS_PER_BYTE: u64 = 7;

/// Per-byte multiplier for on-chain storage writes.
pub const WRITE_GAS_PER_BYTE: u64 = 70;

/// One-time fee for inserting a new key into on-chain storage.
pub const STATE_GROWTH_GAS: u64 = 20_000;

/// Fee for initializing a new sled tree.
pub const TREE_GAS: u64 = 300;

/// Gas per `Poseidon` hash in the sparse Merkle tree.
pub const POSEIDON_HASH_GAS: u64 = 150;

/// Gas per `Sinsemilla` hash in Merkle trees.
pub const SINSEMILLA_HASH_GAS: u64 = 800;

/// Per-row gas for compiling ZK circuits.
pub const COMPILE_GAS_PER_ROW: u64 = 7800;

/// Per-row gas for verifying ZK circuits.
pub const VERIFY_GAS_PER_ROW: u64 = 80;

/// Calculate the gas use for verifying a given zkas circuit.
/// This function assumes that the zkbin was properly decoded.
pub fn circuit_gas_use(zkbin: &ZkBinary) -> u64 {
    let rows = 1u64.checked_shl(zkbin.k).unwrap_or(u64::MAX);
    VERIFY_GAS_PER_ROW.saturating_mul(rows)
}

/// Auxiliary struct representing the full gas usage breakdown of a
/// transaction.
///
/// This data is used for accounting of fees, providing details
/// relating to resource consumption across different transactions.
#[derive(Default, Clone, Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct GasData {
    /// Wasm calls gas consumption
    pub wasm: u64,
    /// ZK circuits gas consumption
    pub zk_circuits: u64,
    /// Signature fee
    pub signatures: u64,
    /// Contract deployment gas
    pub deployments: u64,
    /// Transaction paid fee
    pub paid: u64,
}

impl GasData {
    /// Calculates the total gas used by summing all individual gas
    /// usage fields.
    pub fn total_gas_used(&self) -> u64 {
        self.wasm
            .saturating_add(self.zk_circuits)
            .saturating_add(self.signatures)
            .saturating_add(self.deployments)
    }
}

/// Implements custom debug trait to include
/// [`GasData::total_gas_used`].
impl std::fmt::Debug for GasData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GasData")
            .field("total", &self.total_gas_used())
            .field("wasm", &self.wasm)
            .field("zk_circuits", &self.zk_circuits)
            .field("signatures", &self.signatures)
            .field("deployments", &self.deployments)
            .field("paid", &self.paid)
            .finish()
    }
}
