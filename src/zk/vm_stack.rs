//! VM stack type abstractions
use halo2_gadgets::ecc::{chip::EccChip, FixedPoint, FixedPointBaseField, FixedPointShort, Point};
use halo2_proofs::circuit::{AssignedCell, Value};
use pasta_curves::{pallas, EpAffine};

use crate::{
    crypto::{constants::OrchardFixedBases, merkle_node::MerkleNode},
    zkas::{decoder::ZkBinary, types::Type},
};

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

/// Helper function for verifiers to generate empty witnesses for
/// a given decoded zkas binary
pub fn empty_witnesses(zkbin: &ZkBinary) -> Vec<Witness> {
    let mut ret = Vec::with_capacity(zkbin.witnesses.len());

    for witness in &zkbin.witnesses {
        match witness {
            Type::EcPoint => ret.push(Witness::EcPoint(Value::unknown())),
            Type::EcFixedPoint => ret.push(Witness::EcFixedPoint(Value::unknown())),
            Type::Base => ret.push(Witness::Base(Value::unknown())),
            Type::Scalar => ret.push(Witness::Scalar(Value::unknown())),
            Type::MerklePath => ret.push(Witness::MerklePath(Value::unknown())),
            Type::Uint32 => ret.push(Witness::Uint32(Value::unknown())),
            Type::Uint64 => ret.push(Witness::Uint64(Value::unknown())),
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

impl From<StackVar> for Point<pallas::Affine, EccChip<OrchardFixedBases>> {
    fn from(value: StackVar) -> Self {
        match value {
            StackVar::EcPoint(v) => v,
            _ => unimplemented!(),
        }
    }
}

impl From<StackVar> for FixedPoint<pallas::Affine, EccChip<OrchardFixedBases>> {
    fn from(value: StackVar) -> Self {
        match value {
            StackVar::EcFixedPoint(v) => v,
            _ => unimplemented!(),
        }
    }
}

impl From<StackVar> for Value<pallas::Scalar> {
    fn from(value: StackVar) -> Self {
        match value {
            StackVar::Scalar(v) => v,
            _ => unimplemented!(),
        }
    }
}

impl From<StackVar> for AssignedCell<pallas::Base, pallas::Base> {
    fn from(value: StackVar) -> Self {
        match value {
            StackVar::Base(v) => v,
            _ => unimplemented!(),
        }
    }
}

impl From<StackVar> for Value<u32> {
    fn from(value: StackVar) -> Self {
        match value {
            StackVar::Uint32(v) => v,
            _ => unimplemented!(),
        }
    }
}

impl From<StackVar> for Value<[pallas::Base; 32]> {
    fn from(value: StackVar) -> Self {
        match value {
            StackVar::MerklePath(v) => v,
            _ => unimplemented!(),
        }
    }
}

impl From<StackVar> for FixedPointShort<EpAffine, EccChip<OrchardFixedBases>> {
    fn from(value: StackVar) -> Self {
        match value {
            StackVar::EcFixedPointShort(v) => v,
            _ => unimplemented!(),
        }
    }
}

impl From<StackVar> for FixedPointBaseField<EpAffine, EccChip<OrchardFixedBases>> {
    fn from(value: StackVar) -> Self {
        match value {
            StackVar::EcFixedPointBase(v) => v,
            _ => unimplemented!(),
        }
    }
}
