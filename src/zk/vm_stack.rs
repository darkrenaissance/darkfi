//! Stack type abstractions
use halo2_gadgets::{
    ecc::{chip::EccChip, FixedPoint, Point},
    utilities::CellValue,
};
use pasta_curves::pallas;

use crate::crypto::{constants::OrchardFixedBases, merkle_node::MerkleNode};

#[allow(clippy::large_enum_variant)]
#[derive(Clone)]
pub enum Stack {
    Var(Witness),
    Cell(CellValue<pallas::Base>),
}

#[derive(Clone)]
pub enum Witness {
    EcPoint(Option<Point<pallas::Affine, EccChip<OrchardFixedBases>>>),
    EcFixedPoint(Option<FixedPoint<pallas::Affine, EccChip<OrchardFixedBases>>>),
    Base(Option<pallas::Base>),
    Scalar(Option<pallas::Scalar>),
    MerklePath(Option<Vec<MerkleNode>>),
    Uint32(Option<u32>),
    Uint64(Option<u64>),
}

impl From<Stack> for Point<pallas::Affine, EccChip<OrchardFixedBases>> {
    fn from(value: Stack) -> Self {
        match value {
            Stack::Var(Witness::EcPoint(v)) => v.unwrap(),
            _ => unimplemented!(),
        }
    }
}

impl From<Stack> for FixedPoint<pallas::Affine, EccChip<OrchardFixedBases>> {
    fn from(value: Stack) -> Self {
        match value {
            Stack::Var(Witness::EcFixedPoint(v)) => v.unwrap(),
            _ => unimplemented!(),
        }
    }
}

impl From<Stack> for std::option::Option<pallas::Scalar> {
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
            Stack::Var(Witness::Uint32(v)) => v,
            _ => unimplemented!(),
        }
    }
}

impl From<Stack> for std::option::Option<[pallas::Base; 32]> {
    fn from(value: Stack) -> Self {
        match value {
            Stack::Var(Witness::MerklePath(v)) => {
                if let Some(path) = v {
                    let ret: Vec<pallas::Base> = path.iter().map(|x| x.0).collect();
                    return Some(ret.try_into().unwrap())
                }

                None
            }
            _ => unimplemented!(),
        }
    }
}
