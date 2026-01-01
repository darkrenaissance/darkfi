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

use super::VarType;

/// Opcodes supported by the zkas VM
#[derive(PartialEq, Eq, Hash, Copy, Clone, Debug)]
#[repr(u8)]
pub enum Opcode {
    /// Intermediate opcode for the compiler, should never appear in the result
    Noop = 0x00,

    /// Elliptic curve addition
    EcAdd = 0x01,

    /// Elliptic curve multiplication
    EcMul = 0x02,

    /// Elliptic curve multiplication with a Base field element
    EcMulBase = 0x03,

    /// Elliptic curve multiplication with a Base field element of 64bit width
    EcMulShort = 0x04,

    /// Variable Elliptic curve multiplication with a Base field element
    EcMulVarBase = 0x05,

    /// Get the x coordinate of an elliptic curve point
    EcGetX = 0x08,

    /// Get the y coordinate of an elliptic curve point
    EcGetY = 0x09,

    /// Poseidon hash of N Base field elements
    PoseidonHash = 0x10,

    /// Calculate Merkle root, given a position, Merkle path, and an element
    MerkleRoot = 0x20,

    /// Calculate sparse Merkle root, given the position, path and a member
    SparseMerkleRoot = 0x21,

    /// Base field element addition
    BaseAdd = 0x30,

    /// Base field element multiplication
    BaseMul = 0x31,

    /// Base field element subtraction
    BaseSub = 0x32,

    /// Witness an unsigned integer into a Base field element
    WitnessBase = 0x40,

    /// Range check a Base field element, given bit-width (up to 253)
    RangeCheck = 0x50,

    /// Strictly compare two Base field elements and see if a is less than b
    /// This enforces the sum of remaining bits to be zero.
    LessThanStrict = 0x51,

    /// Loosely two Base field elements and see if a is less than b
    /// This does not enforce the sum of remaining bits to be zero.
    LessThanLoose = 0x52,

    /// Check if a field element fits in a boolean (Either 0 or 1)
    BoolCheck = 0x53,

    /// Conditionally select between two base field elements given a boolean
    CondSelect = 0x60,

    /// Conditionally select between a and b (return a if a is zero, and b if a is nonzero)
    ZeroCondSelect = 0x61,

    /// Constrain equality of two Base field elements inside the circuit
    ConstrainEqualBase = 0xe0,

    /// Constrain equality of two EcPoint elements inside the circuit
    ConstrainEqualPoint = 0xe1,

    /// Constrain a Base field element to a circuit's public input
    ConstrainInstance = 0xf0,

    /// Debug a variable's value in the ZK circuit table.
    DebugPrint = 0xff,
}

impl Opcode {
    pub fn from_name(n: &str) -> Option<Self> {
        match n {
            "ec_add" => Some(Self::EcAdd),
            "ec_mul" => Some(Self::EcMul),
            "ec_mul_base" => Some(Self::EcMulBase),
            "ec_mul_short" => Some(Self::EcMulShort),
            "ec_mul_var_base" => Some(Self::EcMulVarBase),
            "ec_get_x" => Some(Self::EcGetX),
            "ec_get_y" => Some(Self::EcGetY),
            "poseidon_hash" => Some(Self::PoseidonHash),
            "merkle_root" => Some(Self::MerkleRoot),
            "sparse_merkle_root" => Some(Self::SparseMerkleRoot),
            "base_add" => Some(Self::BaseAdd),
            "base_mul" => Some(Self::BaseMul),
            "base_sub" => Some(Self::BaseSub),
            "witness_base" => Some(Self::WitnessBase),
            "range_check" => Some(Self::RangeCheck),
            "less_than_strict" => Some(Self::LessThanStrict),
            "less_than_loose" => Some(Self::LessThanLoose),
            "bool_check" => Some(Self::BoolCheck),
            "cond_select" => Some(Self::CondSelect),
            "zero_cond" => Some(Self::ZeroCondSelect),
            "constrain_equal_base" => Some(Self::ConstrainEqualBase),
            "constrain_equal_point" => Some(Self::ConstrainEqualPoint),
            "constrain_instance" => Some(Self::ConstrainInstance),
            "debug" => Some(Self::DebugPrint),
            _ => None,
        }
    }

    pub fn from_repr(b: u8) -> Option<Self> {
        match b {
            0x01 => Some(Self::EcAdd),
            0x02 => Some(Self::EcMul),
            0x03 => Some(Self::EcMulBase),
            0x04 => Some(Self::EcMulShort),
            0x05 => Some(Self::EcMulVarBase),
            0x08 => Some(Self::EcGetX),
            0x09 => Some(Self::EcGetY),
            0x10 => Some(Self::PoseidonHash),
            0x20 => Some(Self::MerkleRoot),
            0x21 => Some(Self::SparseMerkleRoot),
            0x30 => Some(Self::BaseAdd),
            0x31 => Some(Self::BaseMul),
            0x32 => Some(Self::BaseSub),
            0x40 => Some(Self::WitnessBase),
            0x50 => Some(Self::RangeCheck),
            0x51 => Some(Self::LessThanStrict),
            0x52 => Some(Self::LessThanLoose),
            0x53 => Some(Self::BoolCheck),
            0x60 => Some(Self::CondSelect),
            0x61 => Some(Self::ZeroCondSelect),
            0xe0 => Some(Self::ConstrainEqualBase),
            0xe1 => Some(Self::ConstrainEqualPoint),
            0xf0 => Some(Self::ConstrainInstance),
            0xff => Some(Self::DebugPrint),
            _ => None,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Self::Noop => "noop",
            Self::EcAdd => "ec_add",
            Self::EcMul => "ec_mul",
            Self::EcMulBase => "ec_mul_base",
            Self::EcMulShort => "ec_mul_short",
            Self::EcMulVarBase => "ec_mul_var_base",
            Self::EcGetX => "ec_get_x",
            Self::EcGetY => "ec_get_y",
            Self::PoseidonHash => "poseidon_hash",
            Self::MerkleRoot => "merkle_root",
            Self::SparseMerkleRoot => "sparse_merkle_root",
            Self::BaseAdd => "base_add",
            Self::BaseMul => "base_mul",
            Self::BaseSub => "base_sub",
            Self::WitnessBase => "witness_base",
            Self::RangeCheck => "range_check",
            Self::LessThanStrict => "less_than_strict",
            Self::LessThanLoose => "less_than_loose",
            Self::BoolCheck => "bool_check",
            Self::CondSelect => "cond_select",
            Self::ZeroCondSelect => "zero_cond",
            Self::ConstrainEqualBase => "constrain_equal_base",
            Self::ConstrainEqualPoint => "constrain_equal_point",
            Self::ConstrainInstance => "constrain_instance",
            Self::DebugPrint => "debug",
        }
    }

    /// Return a tuple of vectors of types that are accepted by a specific opcode.
    /// `r.0` is the return type(s), and `r.1` is the argument type(s).
    pub fn arg_types(&self) -> (Vec<VarType>, Vec<VarType>) {
        match self {
            Opcode::Noop => (vec![], vec![]),

            Opcode::EcAdd => (vec![VarType::EcPoint], vec![VarType::EcPoint, VarType::EcPoint]),

            Opcode::EcMul => (vec![VarType::EcPoint], vec![VarType::Scalar, VarType::EcFixedPoint]),

            Opcode::EcMulBase => {
                (vec![VarType::EcPoint], vec![VarType::Base, VarType::EcFixedPointBase])
            }

            Opcode::EcMulShort => {
                (vec![VarType::EcPoint], vec![VarType::Base, VarType::EcFixedPointShort])
            }

            Opcode::EcMulVarBase => {
                (vec![VarType::EcPoint], vec![VarType::Base, VarType::EcNiPoint])
            }

            Opcode::EcGetX => (vec![VarType::Base], vec![VarType::EcPoint]),

            Opcode::EcGetY => (vec![VarType::Base], vec![VarType::EcPoint]),

            Opcode::PoseidonHash => (vec![VarType::Base], vec![VarType::BaseArray]),

            Opcode::MerkleRoot => {
                (vec![VarType::Base], vec![VarType::Uint32, VarType::MerklePath, VarType::Base])
            }

            Opcode::SparseMerkleRoot => {
                (vec![VarType::Base], vec![VarType::Base, VarType::SparseMerklePath, VarType::Base])
            }

            Opcode::BaseAdd => (vec![VarType::Base], vec![VarType::Base, VarType::Base]),

            Opcode::BaseMul => (vec![VarType::Base], vec![VarType::Base, VarType::Base]),

            Opcode::BaseSub => (vec![VarType::Base], vec![VarType::Base, VarType::Base]),

            Opcode::WitnessBase => (vec![VarType::Base], vec![VarType::Uint64]),

            Opcode::RangeCheck => (vec![], vec![VarType::Uint64, VarType::Base]),

            Opcode::LessThanStrict => (vec![], vec![VarType::Base, VarType::Base]),

            Opcode::LessThanLoose => (vec![], vec![VarType::Base, VarType::Base]),

            Opcode::BoolCheck => (vec![], vec![VarType::Base]),

            Opcode::CondSelect => {
                (vec![VarType::Base], vec![VarType::Base, VarType::Base, VarType::Base])
            }

            Opcode::ZeroCondSelect => (vec![VarType::Base], vec![VarType::Base, VarType::Base]),

            Opcode::ConstrainEqualBase => (vec![], vec![VarType::Base, VarType::Base]),

            Opcode::ConstrainEqualPoint => (vec![], vec![VarType::EcPoint, VarType::EcPoint]),

            Opcode::ConstrainInstance => (vec![], vec![VarType::Base]),

            Opcode::DebugPrint => (vec![], vec![VarType::Any]),
        }
    }
}
