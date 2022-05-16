use std::{io, iter};

use halo2_gadgets::sinsemilla::primitives::HashDomain;
use incrementalmerkletree::{Altitude, Hashable};
use lazy_static::lazy_static;
use pasta_curves::{
    group::ff::{PrimeField, PrimeFieldBits},
    pallas,
};
use serde::{
    de::{Deserializer, Error},
    ser::Serializer,
    Deserialize, Serialize,
};
use subtle::{Choice, ConditionallySelectable, CtOption};

use crate::{
    crypto::{
        coin::Coin,
        constants::{
            sinsemilla::{i2lebsp_k, L_ORCHARD_MERKLE, MERKLE_CRH_PERSONALIZATION},
            MERKLE_DEPTH_ORCHARD,
        },
    },
    util::serial::{Decodable, Encodable},
    Result,
};

lazy_static! {
    static ref UNCOMMITTED_ORCHARD: pallas::Base = pallas::Base::from(2);
    static ref EMPTY_ROOTS: Vec<MerkleNode> = {
        iter::empty()
            .chain(Some(MerkleNode::empty_leaf()))
            .chain((0..MERKLE_DEPTH_ORCHARD).scan(MerkleNode::empty_leaf(), |state, l| {
                let l = l as u8;
                *state = MerkleNode::combine(l.into(), state, state);
                Some(*state)
            }))
            .collect()
    };
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct MerkleNode(pub pallas::Base);

impl MerkleNode {
    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.to_repr()
    }

    pub fn from_bytes(bytes: &[u8; 32]) -> CtOption<Self> {
        pallas::Base::from_repr(*bytes).map(MerkleNode)
    }

    pub fn from_coin(coin: &Coin) -> Self {
        MerkleNode(coin.0)
    }

    pub fn inner(&self) -> pallas::Base {
        self.0
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

impl ConditionallySelectable for MerkleNode {
    fn conditional_select(a: &Self, b: &Self, choice: Choice) -> Self {
        MerkleNode(pallas::Base::conditional_select(&a.0, &b.0, choice))
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
                        .chain(left.0.to_le_bits().iter().by_vals().take(L_ORCHARD_MERKLE))
                        .chain(right.0.to_le_bits().iter().by_vals().take(L_ORCHARD_MERKLE)),
                )
                .unwrap_or(pallas::Base::zero()),
        )
    }

    fn empty_root(altitude: Altitude) -> Self {
        EMPTY_ROOTS[<usize>::from(altitude)]
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

impl Encodable for incrementalmerkletree::Position {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        u64::from(*self).encode(&mut s)
    }
}

impl Decodable for incrementalmerkletree::Position {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        let dec: u64 = Decodable::decode(&mut d)?;
        Ok(Self::try_from(dec).unwrap())
    }
}
