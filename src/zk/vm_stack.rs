//! VM stack type abstractions
use halo2_gadgets::{
    ecc::{chip::EccChip, FixedPoint, Point},
    utilities::CellValue,
};
use pasta_curves::pallas;

use crate::crypto::{constants::OrchardFixedBases, merkle_node::MerkleNode};

/// These represent the witness types outside of the circuit
#[allow(clippy::large_enum_variant)]
#[derive(Clone)]
pub enum Witness {
    EcPoint(Option<pallas::Point>),
    EcFixedPoint(Option<pallas::Point>),
    Base(Option<pallas::Base>),
    Scalar(Option<pallas::Scalar>),
    MerklePath(Option<[MerkleNode; 32]>),
    Uint32(Option<u32>),
    Uint64(Option<u64>),
}

/// These represent the witness types inside the circuit
#[allow(clippy::large_enum_variant)]
#[derive(Clone)]
pub enum StackVar {
    EcPoint(Point<pallas::Affine, EccChip<OrchardFixedBases>>),
    EcFixedPoint(FixedPoint<pallas::Affine, EccChip<OrchardFixedBases>>),
    Base(CellValue<pallas::Base>),
    Scalar(Option<pallas::Scalar>),
    MerklePath(Option<[pallas::Base; 32]>),
    Uint32(Option<u32>),
    Uint64(Option<u64>),
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

impl From<StackVar> for std::option::Option<pallas::Scalar> {
    fn from(value: StackVar) -> Self {
        match value {
            StackVar::Scalar(v) => v,
            _ => unimplemented!(),
        }
    }
}

impl From<StackVar> for CellValue<pallas::Base> {
    fn from(value: StackVar) -> Self {
        match value {
            StackVar::Base(v) => v,
            _ => unimplemented!(),
        }
    }
}

impl From<StackVar> for std::option::Option<u32> {
    fn from(value: StackVar) -> Self {
        match value {
            StackVar::Uint32(v) => v,
            _ => unimplemented!(),
        }
    }
}

impl From<StackVar> for std::option::Option<[pallas::Base; 32]> {
    fn from(value: StackVar) -> Self {
        match value {
            StackVar::MerklePath(v) => v,
            _ => unimplemented!(),
        }
    }
}
