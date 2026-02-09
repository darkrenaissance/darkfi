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

use darkfi_sdk::crypto::constants::{MERKLE_DEPTH_ORCHARD, SPARSE_MERKLE_DEPTH};
use darkfi_serial::{async_trait, SerialDecodable, SerialEncodable};

use crate::zkas::{Opcode, VarType, ZkBinary};

/// Fixed fee for verifying Schnorr signatures using the Pallas elliptic curve
pub const PALLAS_SCHNORR_SIGNATURE_FEE: u64 = 1000;

/// Calculate the gas use for verifying a given zkas circuit.
/// This function assumes that the zkbin was properly decoded.
pub fn circuit_gas_use(zkbin: &ZkBinary) -> u64 {
    let mut accumulator: u64 = 0;

    // Constants each with a cost of 10
    accumulator = accumulator.saturating_add(10u64.saturating_mul(zkbin.constants.len() as u64));

    // Literals each with a cost of 10 (for now there's only 1 type of literal)
    accumulator = accumulator.saturating_add(10u64.saturating_mul(zkbin.literals.len() as u64));

    // Witnesses have cost by type
    for witness in &zkbin.witnesses {
        let cost = match witness {
            VarType::Dummy => unreachable!(),
            VarType::EcPoint => 20,
            VarType::EcFixedPoint => unreachable!(),
            VarType::EcFixedPointShort => unreachable!(),
            VarType::EcFixedPointBase => unreachable!(),
            VarType::EcNiPoint => 20,
            VarType::Base => 10,
            VarType::BaseArray => unreachable!(),
            VarType::Scalar => 20,
            VarType::ScalarArray => unreachable!(),
            VarType::MerklePath => 10 * MERKLE_DEPTH_ORCHARD as u64,
            VarType::SparseMerklePath => 10 * SPARSE_MERKLE_DEPTH as u64,
            VarType::Uint32 => 10,
            VarType::Uint64 => 10,
            VarType::Any => 10,
        };

        accumulator = accumulator.saturating_add(cost);
    }

    // Opcodes depending on how heavy they are
    for opcode in &zkbin.opcodes {
        let cost = match opcode.0 {
            Opcode::Noop => unreachable!(),
            Opcode::EcAdd => 30,
            Opcode::EcMul => 30,
            Opcode::EcMulBase => 30,
            Opcode::EcMulShort => 30,
            Opcode::EcMulVarBase => 30,
            Opcode::EcGetX => 5,
            Opcode::EcGetY => 5,
            Opcode::PoseidonHash => {
                20u64.saturating_add(10u64.saturating_mul(opcode.1.len() as u64))
            }
            Opcode::MerkleRoot => 10 * MERKLE_DEPTH_ORCHARD as u64,
            Opcode::SparseMerkleRoot => 10 * SPARSE_MERKLE_DEPTH as u64,
            Opcode::BaseAdd => 15,
            Opcode::BaseMul => 15,
            Opcode::BaseSub => 15,
            Opcode::WitnessBase => 10,
            Opcode::RangeCheck => 60,
            Opcode::LessThanStrict => 100,
            Opcode::LessThanLoose => 100,
            Opcode::BoolCheck => 20,
            Opcode::CondSelect => 10,
            Opcode::ZeroCondSelect => 10,
            Opcode::ConstrainEqualBase => 10,
            Opcode::ConstrainEqualPoint => 20,
            Opcode::ConstrainInstance => 10,
            Opcode::DebugPrint => 100,
        };

        accumulator = accumulator.saturating_add(cost);
    }

    accumulator
}

/// Auxiliary struct representing the full gas usage breakdown of a transaction.
///
/// This data is used for accounting of fees, providing details relating to
/// resource consumption across different transactions.
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
    /// Calculates the total gas used by summing all individual gas usage fields.
    pub fn total_gas_used(&self) -> u64 {
        self.wasm
            .saturating_add(self.zk_circuits)
            .saturating_add(self.signatures)
            .saturating_add(self.deployments)
    }
}

/// Implements custom debug trait to include [`GasData::total_gas_used`].
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

/// Auxiliary function to compute the corresponding fee value
/// for the provided gas.
///
/// Currently we simply divide the gas value by 100.
pub fn compute_fee(gas: &u64) -> u64 {
    gas / 100
}
