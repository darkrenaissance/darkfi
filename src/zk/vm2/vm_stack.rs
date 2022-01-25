//! Stack type abstractions
use halo2_gadgets::{
    ecc::{chip::EccChip, FixedPoint, Point},
    utilities::CellValue,
};
use pasta_curves::pallas;

use crate::crypto::{constants::OrchardFixedBases, merkle_node::MerkleNode};

#[derive(Clone)]
pub enum Stack {
    Var(Witness),
    Cell(CellValue<pallas::Base>),
}

#[derive(Clone)]
pub enum Witness {
    EcPoint(Point<pallas::Affine, EccChip<OrchardFixedBases>>),
    EcFixedPoint(FixedPoint<pallas::Affine, EccChip<OrchardFixedBases>>),
    Base(pallas::Base),
    Scalar(pallas::Scalar),
    MerklePath(Vec<MerkleNode>),
    Uint32(u32),
    Uint64(u64),
}

impl From<Stack> for Point<pallas::Affine, EccChip<OrchardFixedBases>> {
    fn from(value: Stack) -> Self {
        match value {
            Stack::Var(Witness::EcPoint(v)) => v,
            _ => unimplemented!(),
        }
    }
}

impl From<Stack> for FixedPoint<pallas::Affine, EccChip<OrchardFixedBases>> {
    fn from(value: Stack) -> Self {
        match value {
            Stack::Var(Witness::EcFixedPoint(v)) => v,
            _ => unimplemented!(),
        }
    }
}

impl From<Stack> for pallas::Scalar {
    fn from(value: Stack) -> Self {
        match value {
            Stack::Var(Witness::Scalar(v)) => v,
            _ => unimplemented!(),
        }
    }
}

impl From<Stack> for CellValue<pallas::Base> {
    fn from(value: Stack) -> Self {
        match value {
            Stack::Cell(v) => v,
            _ => unimplemented!(),
        }
    }
}

impl From<Stack> for std::option::Option<u32> {
    fn from(value: Stack) -> Self {
        match value {
            Stack::Var(Witness::Uint32(v)) => Some(v),
            _ => unimplemented!(),
        }
    }
}

impl From<Stack> for std::option::Option<[pallas::Base; 32]> {
    fn from(value: Stack) -> Self {
        match value {
            Stack::Var(Witness::MerklePath(v)) => {
                let ret: Vec<pallas::Base> = v.iter().map(|x| x.0).collect();
                Some(ret.try_into().unwrap())
            }
            _ => unimplemented!(),
        }
    }
}
