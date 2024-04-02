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

use darkfi_sdk::crypto::constants::{MERKLE_DEPTH_ORCHARD, SPARSE_MERKLE_DEPTH};

use crate::zkas::{Opcode, VarType, ZkBinary};

/// Fixed fee for verifying Schnorr signatures using the Pallas elliptic curve
pub const PALLAS_SCHNORR_SIGNATURE_FEE: u64 = 1000;

/// Calculate the gas use for verifying a given zkas circuit.
/// This function assumes that the zkbin was properly decoded.
pub fn circuit_gas_use(zkbin: &ZkBinary) -> u64 {
    let mut accumulator: u64 = 0;

    // Constants each with a cost of 10
    accumulator += 10 * zkbin.constants.len() as u64;

    // Literals each with a cost of 10 (for now there's only 1 type of literal)
    accumulator += 10 * zkbin.literals.len() as u64;

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

        accumulator += cost;
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
            Opcode::PoseidonHash => 20 + 10 * opcode.1.len() as u64,
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

        accumulator += cost;
    }

    accumulator
}
