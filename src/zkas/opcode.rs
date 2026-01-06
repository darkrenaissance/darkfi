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

/// Macro to define all opcodes in a single place.
/// This generates the enum definition, `from_name`, `from_repr`, `name`, and `arg_types` methods.
///
/// Format for each opcode:
/// ```text
/// [doc_comments]
/// VariantName = 0xNN, "string_name", (return_types), (arg_types)
/// ```
///
/// Note: Noop is handled specially - it's excluded from `from_repr` and `from_name`
/// as it should never appear in compiled binaries or source code.
macro_rules! define_opcodes {
    (
        // Special case for Noop which shouldn't be in from_repr/from_name
        $(#[doc = $noop_doc:literal])*
        Noop = $noop_value:literal, $noop_name:literal,
            ($($noop_ret:expr),*), ($($noop_arg:expr),*);

        // All other opcodes
        $(
            $(#[doc = $doc:literal])*
            $variant:ident = $value:literal, $name:literal,
            ($($ret:expr),*), ($($arg:expr),*)
        );* $(;)?
    ) => {
        /// Opcodes supported by the zkas VM
        #[derive(PartialEq, Eq, Hash, Copy, Clone, Debug)]
        #[repr(u8)]
        pub enum Opcode {
            $(#[doc = $noop_doc])*
            Noop = $noop_value,

            $(
                $(#[doc = $doc])*
                $variant = $value,
            )*
        }

        impl Opcode {
            /// Look up an opcode by its string name (used in source code).
            /// Note: Noop cannot be looked up by name as it's an internal compiler opcode.
            pub fn from_name(n: &str) -> Option<Self> {
                match n {
                    $($name => Some(Self::$variant),)*
                    _ => None,
                }
            }

            /// Look up an opcode by its binary representation.
            /// Note: Noop (0x00) is not valid in binary as it should never be compiled.
            pub fn from_repr(b: u8) -> Option<Self> {
                match b {
                    $($value => Some(Self::$variant),)*
                    _ => None,
                }
            }

            /// Get the string name of an opcode.
            pub fn name(&self) -> &'static str {
                match self {
                    Self::Noop => $noop_name,
                    $(Self::$variant => $name,)*
                }
            }

            /// Return a tuple of vectors of types that are accepted by a specific opcode.
            /// `r.0` is the return type(s), and `r.1` is the argument type(s).
            pub fn arg_types(&self) -> (Vec<VarType>, Vec<VarType>) {
                match self {
                    Self::Noop => (vec![$($noop_ret),*], vec![$($noop_arg),*]),
                    $(Self::$variant => (vec![$($ret),*], vec![$($arg),*]),)*
                }
            }
        }
    };
}

define_opcodes! {
    /// Intermediate opcode for the compiler, should never appear in the result
    Noop = 0x00, "noop",
        (), ();

    /// Elliptic curve addition
    EcAdd = 0x01, "ec_add",
        (VarType::EcPoint), (VarType::EcPoint, VarType::EcPoint);

    /// Elliptic curve multiplication
    EcMul = 0x02, "ec_mul",
        (VarType::EcPoint), (VarType::Scalar, VarType::EcFixedPoint);

    /// Elliptic curve multiplication with a Base field element
    EcMulBase = 0x03, "ec_mul_base",
        (VarType::EcPoint), (VarType::Base, VarType::EcFixedPointBase);

    /// Elliptic curve multiplication with a Base field element of 64bit width
    EcMulShort = 0x04, "ec_mul_short",
        (VarType::EcPoint), (VarType::Base, VarType::EcFixedPointShort);

    /// Variable Elliptic curve multiplication with a Base field element
    EcMulVarBase = 0x05, "ec_mul_var_base",
        (VarType::EcPoint), (VarType::Base, VarType::EcNiPoint);

    /// Get the x coordinate of an elliptic curve point
    EcGetX = 0x08, "ec_get_x",
        (VarType::Base), (VarType::EcPoint);

    /// Get the y coordinate of an elliptic curve point
    EcGetY = 0x09, "ec_get_y",
        (VarType::Base), (VarType::EcPoint);

    /// Poseidon hash of N Base field elements
    PoseidonHash = 0x10, "poseidon_hash",
        (VarType::Base), (VarType::BaseArray);

    /// Calculate Merkle root, given a position, Merkle path, and an element
    MerkleRoot = 0x20, "merkle_root",
        (VarType::Base), (VarType::Uint32, VarType::MerklePath, VarType::Base);

    /// Calculate sparse Merkle root, given the position, path and a member
    SparseMerkleRoot = 0x21, "sparse_merkle_root",
        (VarType::Base), (VarType::Base, VarType::SparseMerklePath, VarType::Base);

    /// Base field element addition
    BaseAdd = 0x30, "base_add",
        (VarType::Base), (VarType::Base, VarType::Base);

    /// Base field element multiplication
    BaseMul = 0x31, "base_mul",
        (VarType::Base), (VarType::Base, VarType::Base);

    /// Base field element subtraction
    BaseSub = 0x32, "base_sub",
        (VarType::Base), (VarType::Base, VarType::Base);

    /// Witness an unsigned integer into a Base field element
    WitnessBase = 0x40, "witness_base",
        (VarType::Base), (VarType::Uint64);

    /// Range check a Base field element, given bit-width (up to 253)
    RangeCheck = 0x50, "range_check",
        (), (VarType::Uint64, VarType::Base);

    /// Strictly compare two Base field elements and see if a is less than b.
    /// This enforces the sum of remaining bits to be zero.
    LessThanStrict = 0x51, "less_than_strict",
        (), (VarType::Base, VarType::Base);

    /// Loosely compare two Base field elements and see if a is less than b.
    /// This does not enforce the sum of remaining bits to be zero.
    LessThanLoose = 0x52, "less_than_loose",
        (), (VarType::Base, VarType::Base);

    /// Check if a field element fits in a boolean (Either 0 or 1)
    BoolCheck = 0x53, "bool_check",
        (), (VarType::Base);

    /// Conditionally select between two base field elements given a boolean
    CondSelect = 0x60, "cond_select",
        (VarType::Base), (VarType::Base, VarType::Base, VarType::Base);

    /// Conditionally select between a and b (return a if a is zero, and b if a is nonzero)
    ZeroCondSelect = 0x61, "zero_cond",
        (VarType::Base), (VarType::Base, VarType::Base);

    /// Constrain equality of two Base field elements inside the circuit
    ConstrainEqualBase = 0xe0, "constrain_equal_base",
        (), (VarType::Base, VarType::Base);

    /// Constrain equality of two EcPoint elements inside the circuit
    ConstrainEqualPoint = 0xe1, "constrain_equal_point",
        (), (VarType::EcPoint, VarType::EcPoint);

    /// Constrain a Base field element to a circuit's public input
    ConstrainInstance = 0xf0, "constrain_instance",
        (), (VarType::Base);

    /// Debug a variable's value in the ZK circuit table
    DebugPrint = 0xff, "debug",
        (), (VarType::Any);
}
