/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2022 Dyne.org foundation
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

//! VM stack type abstractions
use darkfi_sdk::crypto::{constants::OrchardFixedBases, MerkleNode};
use halo2_gadgets::ecc::{chip::EccChip, FixedPoint, FixedPointBaseField, FixedPointShort, Point};
use halo2_proofs::{
    circuit::{AssignedCell, Value},
    pasta::pallas,
};

use crate::zkas::{decoder::ZkBinary, types::VarType};

/// These represent the witness types outside of the circuit
#[allow(clippy::large_enum_variant)]
#[derive(Clone)]
pub enum Witness {
    EcPoint(Value<pallas::Point>),
    EcFixedPoint(Value<pallas::Point>),
    Base(Value<pallas::Base>),
    Scalar(Value<pallas::Scalar>),
    MerklePath(Value<[MerkleNode; 32]>),
    Uint32(Value<u32>),
    Uint64(Value<u64>),
}

pub enum Literal {
    Uint64(Value<u64>),
}

/// Helper function for verifiers to generate empty witnesses for
/// a given decoded zkas binary
pub fn empty_witnesses(zkbin: &ZkBinary) -> Vec<Witness> {
    let mut ret = Vec::with_capacity(zkbin.witnesses.len());

    for witness in &zkbin.witnesses {
        match witness {
            VarType::EcPoint => ret.push(Witness::EcPoint(Value::unknown())),
            VarType::EcFixedPoint => ret.push(Witness::EcFixedPoint(Value::unknown())),
            VarType::Base => ret.push(Witness::Base(Value::unknown())),
            VarType::Scalar => ret.push(Witness::Scalar(Value::unknown())),
            VarType::MerklePath => ret.push(Witness::MerklePath(Value::unknown())),
            VarType::Uint32 => ret.push(Witness::Uint32(Value::unknown())),
            VarType::Uint64 => ret.push(Witness::Uint64(Value::unknown())),
            _ => todo!("Handle this gracefully"),
        }
    }

    ret
}

/// These represent the witness types inside the circuit
#[allow(clippy::large_enum_variant)]
#[derive(Clone)]
pub enum StackVar {
    EcPoint(Point<pallas::Affine, EccChip<OrchardFixedBases>>),
    EcFixedPoint(FixedPoint<pallas::Affine, EccChip<OrchardFixedBases>>),
    EcFixedPointShort(FixedPointShort<pallas::Affine, EccChip<OrchardFixedBases>>),
    EcFixedPointBase(FixedPointBaseField<pallas::Affine, EccChip<OrchardFixedBases>>),
    Base(AssignedCell<pallas::Base, pallas::Base>),
    Scalar(Value<pallas::Scalar>),
    MerklePath(Value<[pallas::Base; 32]>),
    Uint32(Value<u32>),
    Uint64(Value<u64>),
}

// TODO: Make this not panic (try_from)
macro_rules! impl_from {
    ($variant:ident, $fortype:ty) => {
        impl From<StackVar> for $fortype {
            fn from(value: StackVar) -> Self {
                match value {
                    StackVar::$variant(v) => v,
                    _ => unreachable!(),
                }
            }
        }
    };
}

impl_from!(EcPoint, Point<pallas::Affine, EccChip<OrchardFixedBases>>);
impl_from!(EcFixedPoint, FixedPoint<pallas::Affine, EccChip<OrchardFixedBases>>);
impl_from!(EcFixedPointShort, FixedPointShort<pallas::Affine, EccChip<OrchardFixedBases>>);
impl_from!(EcFixedPointBase, FixedPointBaseField<pallas::Affine, EccChip<OrchardFixedBases>>);
impl_from!(Scalar, Value<pallas::Scalar>);
impl_from!(Base, AssignedCell<pallas::Base, pallas::Base>);
impl_from!(Uint32, Value<u32>);
impl_from!(MerklePath, Value<[pallas::Base; 32]>);
