use std::{io, iter};

use halo2_gadgets::primitives::sinsemilla::HashDomain;
use incrementalmerkletree::{Altitude, Hashable};
use lazy_static::lazy_static;
use pasta_curves::{
    arithmetic::FieldExt,
    group::ff::{PrimeField, PrimeFieldBits},
    pallas,
};
use serde::{
    de::{Deserializer, Error},
    ser::Serializer,
    Deserialize, Serialize,
};
use subtle::{ConstantTimeEq, CtOption};

use crate::{
    crypto::constants::{
        sinsemilla::{i2lebsp_k, MERKLE_CRH_PERSONALIZATION},
        L_ORCHARD_MERKLE, MERKLE_DEPTH_ORCHARD,
    },
    error::Result,
    serial::{Decodable, Encodable},
};

lazy_static! {
    static ref UNCOMMITTED_ORCHARD: pallas::Base = pallas::Base::from_u64(2);
    static ref EMPTY_ROOTS: Vec<MerkleNode> = {
        iter::empty()
            .chain(Some(MerkleNode::empty_leaf()))
            .chain((0..MERKLE_DEPTH_ORCHARD).scan(MerkleNode::empty_leaf(), |state, l| {
                let l = l as u8;
                *state = MerkleNode::combine(l.into(), state, state);
                Some(state.clone())
            }))
            .collect()
    };
}

#[derive(Debug, Clone, Eq)]
pub struct MerkleNode(pub pallas::Base);

impl MerkleNode {
    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.to_repr()
    }

    pub fn from_bytes(bytes: &[u8; 32]) -> CtOption<Self> {
        pallas::Base::from_repr(*bytes).map(MerkleNode)
    }
}

impl Serialize for MerkleNode {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        self.to_bytes().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for MerkleNode {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let parsed = <[u8; 32]>::deserialize(deserializer)?;
        <Option<_>>::from(Self::from_bytes(&parsed)).ok_or_else(|| {
            Error::custom("Attempted to deserialize a non-canonical representation of a Pallas base field element")
        })
    }
}

impl std::cmp::PartialEq for MerkleNode {
    fn eq(&self, other: &Self) -> bool {
        self.0.ct_eq(&other.0).into()
    }
}

impl std::hash::Hash for MerkleNode {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        <Option<pallas::Base>>::from(self.0).map(|b| b.to_bytes()).hash(state)
    }
}

impl Hashable for MerkleNode {
    fn empty_leaf() -> Self {
        MerkleNode(*UNCOMMITTED_ORCHARD)
    }

    /// Implements `MerkleCRH^Orchard` as defined in
    /// <https://zips.z.cash/protocol/protocol.pdf#orchardmerklecrh>
    ///
    /// The layer with 2^n nodes is called "layer n":
    ///      - leaves are at layer MERKLE_DEPTH_ORCHARD = 32;
    ///      - the root is at layer 0.
    /// `l` is MERKLE_DEPTH_ORCHARD - layer - 1.
    ///      - when hashing two leaves, we produce a node on the layer above the leaves, i.e. layer
    ///        = 31, l = 0
    ///      - when hashing to the final root, we produce the anchor with layer = 0, l = 31.
    fn combine(altitude: Altitude, left: &Self, right: &Self) -> Self {
        // MerkleCRH Sinsemilla hash domain.
        let domain = HashDomain::new(MERKLE_CRH_PERSONALIZATION);

        MerkleNode(
            domain
                .hash(
                    iter::empty()
                        .chain(i2lebsp_k(altitude.into()).iter().copied())
                        .chain(left.0.to_le_bits().iter().by_val().take(L_ORCHARD_MERKLE))
                        .chain(right.0.to_le_bits().iter().by_val().take(L_ORCHARD_MERKLE)),
                )
                .unwrap_or(pallas::Base::zero()),
        )
    }

    fn empty_root(altitude: Altitude) -> Self {
        EMPTY_ROOTS[<usize>::from(altitude)].clone()
    }
}

impl Encodable for MerkleNode {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        self.0.encode(&mut s)
    }
}

impl Decodable for MerkleNode {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        Ok(Self(Decodable::decode(&mut d)?))
    }
}
