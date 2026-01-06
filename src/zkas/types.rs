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

/// Heap types in bincode & vm
#[derive(Clone, Debug)]
#[repr(u8)]
pub enum HeapType {
    Var = 0x00,
    Lit = 0x01,
}

impl HeapType {
    pub fn from_repr(b: u8) -> Option<Self> {
        match b {
            0x00 => Some(Self::Var),
            0x01 => Some(Self::Lit),
            _ => None,
        }
    }
}

/// Variable types supported by the zkas VM
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum VarType {
    /// Dummy intermediate type
    Dummy = 0x00,

    /// Elliptic curve point
    EcPoint = 0x01,

    /// Elliptic curve fixed point (a constant)
    EcFixedPoint = 0x02,

    /// Elliptic curve fixed point short
    EcFixedPointShort = 0x03,

    /// Elliptic curve fixed point in base field
    EcFixedPointBase = 0x04,

    /// Elliptic curve nonidentity point
    EcNiPoint = 0x05,

    /// Base field element
    Base = 0x10,

    /// Base field element array
    BaseArray = 0x11,

    /// Scalar field element
    Scalar = 0x12,

    /// Scalar field element array
    ScalarArray = 0x13,

    /// Merkle tree path
    MerklePath = 0x20,

    /// Sparse merkle tree path
    SparseMerklePath = 0x21,

    /// Unsigned 32-bit integer
    Uint32 = 0x30,

    /// Unsigned 64-bit integer
    Uint64 = 0x31,

    /// Catch-all for any type
    Any = 0xff,
}

impl VarType {
    pub fn from_repr(b: u8) -> Option<Self> {
        match b {
            0x01 => Some(Self::EcPoint),
            0x02 => Some(Self::EcFixedPoint),
            0x03 => Some(Self::EcFixedPointShort),
            0x04 => Some(Self::EcFixedPointBase),
            0x05 => Some(Self::EcNiPoint),
            0x10 => Some(Self::Base),
            0x11 => Some(Self::BaseArray),
            0x12 => Some(Self::Scalar),
            0x13 => Some(Self::ScalarArray),
            0x20 => Some(Self::MerklePath),
            0x21 => Some(Self::SparseMerklePath),
            0x30 => Some(Self::Uint32),
            0x31 => Some(Self::Uint64),
            0xff => Some(Self::Any),
            _ => None,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Dummy => "Dummy",
            Self::EcPoint => "EcPoint",
            Self::EcFixedPoint => "EcFixedPoint",
            Self::EcFixedPointShort => "EcFixedPointShort",
            Self::EcFixedPointBase => "EcFixedPointBase",
            Self::EcNiPoint => "EcNiPoint",
            Self::Base => "Base",
            Self::BaseArray => "BaseArray",
            Self::Scalar => "Scalar",
            Self::ScalarArray => "ScalarArray",
            Self::MerklePath => "MerklePath",
            Self::SparseMerklePath => "SparseMerklePath",
            Self::Uint32 => "Uint32",
            Self::Uint64 => "Uint64",
            Self::Any => "Any",
        }
    }
}

/// Literal types supported by the zkas VM
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum LitType {
    /// Dummy intermediate type
    Dummy = 0x00,

    /// Unsigned 64-bit integer
    Uint64 = 0x01,
}

impl LitType {
    pub fn from_repr(b: u8) -> Option<Self> {
        match b {
            0x01 => Some(Self::Uint64),
            _ => None,
        }
    }

    pub fn to_vartype(&self) -> VarType {
        match self {
            Self::Dummy => VarType::Dummy,
            Self::Uint64 => VarType::Uint64,
        }
    }
}
