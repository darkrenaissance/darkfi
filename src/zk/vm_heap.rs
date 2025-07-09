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

//! VM heap type abstractions
use darkfi_sdk::crypto::{
    constants::{OrchardFixedBases, MERKLE_DEPTH_ORCHARD},
    smt::SMT_FP_DEPTH,
    MerkleNode,
};
use halo2_gadgets::ecc::{
    chip::EccChip, FixedPoint, FixedPointBaseField, FixedPointShort, NonIdentityPoint, Point,
    ScalarFixed,
};
use halo2_proofs::{
    circuit::{AssignedCell, Value},
    pasta::pallas,
    plonk,
};
use tracing::error;

use crate::{
    zkas::{decoder::ZkBinary, types::VarType},
    Error::ZkasDecoderError,
    Result,
};

/// These represent the witness types outside of the circuit
#[allow(clippy::large_enum_variant)]
#[derive(Clone)]
pub enum Witness {
    EcPoint(Value<pallas::Point>),
    EcNiPoint(Value<pallas::Point>),
    EcFixedPoint(Value<pallas::Point>),
    Base(Value<pallas::Base>),
    Scalar(Value<pallas::Scalar>),
    MerklePath(Value<[MerkleNode; MERKLE_DEPTH_ORCHARD]>),
    SparseMerklePath(Value<[pallas::Base; SMT_FP_DEPTH]>),
    Uint32(Value<u32>),
    Uint64(Value<u64>),
}

impl Witness {
    pub fn name(&self) -> &str {
        match self {
            Self::EcPoint(_) => "EcPoint",
            Self::EcNiPoint(_) => "EcNiPoint",
            Self::EcFixedPoint(_) => "EcFixedPoint",
            Self::Base(_) => "Base",
            Self::Scalar(_) => "Scalar",
            Self::MerklePath(_) => "MerklePath",
            Self::SparseMerklePath(_) => "SparseMerklePath",
            Self::Uint32(_) => "Uint32",
            Self::Uint64(_) => "Uint64",
        }
    }
}

/// Helper function for verifiers to generate empty witnesses for
/// a given decoded zkas binary
pub fn empty_witnesses(zkbin: &ZkBinary) -> Result<Vec<Witness>> {
    let mut ret = Vec::with_capacity(zkbin.witnesses.len());

    for witness in &zkbin.witnesses {
        match witness {
            VarType::EcPoint => ret.push(Witness::EcPoint(Value::unknown())),
            VarType::EcNiPoint => ret.push(Witness::EcNiPoint(Value::unknown())),
            VarType::EcFixedPoint => ret.push(Witness::EcFixedPoint(Value::unknown())),
            VarType::Base => ret.push(Witness::Base(Value::unknown())),
            VarType::Scalar => ret.push(Witness::Scalar(Value::unknown())),
            VarType::MerklePath => ret.push(Witness::MerklePath(Value::unknown())),
            VarType::SparseMerklePath => ret.push(Witness::SparseMerklePath(Value::unknown())),
            VarType::Uint32 => ret.push(Witness::Uint32(Value::unknown())),
            VarType::Uint64 => ret.push(Witness::Uint64(Value::unknown())),
            x => return Err(ZkasDecoderError(format!("Unsupported witness type: {x:?}"))),
        }
    }

    Ok(ret)
}

/// These represent the witness types inside the circuit
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum HeapVar {
    EcPoint(Point<pallas::Affine, EccChip<OrchardFixedBases>>),
    EcNiPoint(NonIdentityPoint<pallas::Affine, EccChip<OrchardFixedBases>>),
    EcFixedPoint(FixedPoint<pallas::Affine, EccChip<OrchardFixedBases>>),
    EcFixedPointShort(FixedPointShort<pallas::Affine, EccChip<OrchardFixedBases>>),
    EcFixedPointBase(FixedPointBaseField<pallas::Affine, EccChip<OrchardFixedBases>>),
    Base(AssignedCell<pallas::Base, pallas::Base>),
    Scalar(ScalarFixed<pallas::Affine, EccChip<OrchardFixedBases>>),
    MerklePath(Value<[pallas::Base; MERKLE_DEPTH_ORCHARD]>),
    SparseMerklePath(Value<[pallas::Base; SMT_FP_DEPTH]>),
    Uint32(Value<u32>),
    Uint64(Value<u64>),
}

macro_rules! impl_try_from {
    ($variant:ident, $fortype:ty) => {
        impl std::convert::TryFrom<HeapVar> for $fortype {
            type Error = plonk::Error;

            fn try_from(value: HeapVar) -> std::result::Result<Self, Self::Error> {
                match value {
                    HeapVar::$variant(v) => Ok(v),
                    x => {
                        error!("Expected {}, but instead got: {x:?}", stringify!($variant));
                        Err(plonk::Error::Synthesis)
                    }
                }
            }
        }
    };
}

impl_try_from!(EcPoint, Point<pallas::Affine, EccChip<OrchardFixedBases>>);
impl_try_from!(EcNiPoint, NonIdentityPoint<pallas::Affine, EccChip<OrchardFixedBases>>);
impl_try_from!(EcFixedPoint, FixedPoint<pallas::Affine, EccChip<OrchardFixedBases>>);
impl_try_from!(EcFixedPointShort, FixedPointShort<pallas::Affine, EccChip<OrchardFixedBases>>);
impl_try_from!(EcFixedPointBase, FixedPointBaseField<pallas::Affine, EccChip<OrchardFixedBases>>);
impl_try_from!(Scalar, ScalarFixed<pallas::Affine, EccChip<OrchardFixedBases>>);
impl_try_from!(Base, AssignedCell<pallas::Base, pallas::Base>);
impl_try_from!(Uint32, Value<u32>);
impl_try_from!(MerklePath, Value<[pallas::Base; MERKLE_DEPTH_ORCHARD]>);
impl_try_from!(SparseMerklePath, Value<[pallas::Base; SMT_FP_DEPTH]>);
