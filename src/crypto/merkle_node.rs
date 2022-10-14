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
use subtle::{Choice, ConditionallySelectable};

use crate::{
    crypto::{
        coin::Coin,
        constants::{
            sinsemilla::{i2lebsp_k, L_ORCHARD_MERKLE, MERKLE_CRH_PERSONALIZATION},
            MERKLE_DEPTH_ORCHARD,
        },
    },
    serial::{Decodable, Encodable, SerialDecodable, SerialEncodable},
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

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, SerialEncodable, SerialDecodable)]
pub struct MerkleNode(pub pallas::Base);

impl MerkleNode {
    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.to_repr()
    }

    pub fn from_bytes(bytes: [u8; 32]) -> Option<Self> {
        let n = pallas::Base::from_repr(bytes);
        match bool::from(n.is_some()) {
            true => Some(Self(n.unwrap())),
            false => None,
        }
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
        Self::from_bytes(parsed).ok_or_else(|| {
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

impl Encodable for incrementalmerkletree::Position {
    fn encode<S: io::Write>(&self, mut s: S) -> core::result::Result<usize, io::Error> {
        u64::from(*self).encode(&mut s)
    }
}

impl Decodable for incrementalmerkletree::Position {
    fn decode<D: io::Read>(mut d: D) -> core::result::Result<Self, io::Error> {
        let dec: u64 = Decodable::decode(&mut d)?;
        Ok(Self::try_from(dec).unwrap())
    }
}

impl Encodable for incrementalmerkletree::bridgetree::Leaf<MerkleNode> {
    fn encode<S: io::Write>(&self, mut s: S) -> core::result::Result<usize, io::Error> {
        let mut len = 0;

        match self {
            incrementalmerkletree::bridgetree::Leaf::Left(a) => {
                len += false.encode(&mut s)?;
                len += a.encode(&mut s)?;
            }

            incrementalmerkletree::bridgetree::Leaf::Right(a, b) => {
                len += true.encode(&mut s)?;
                len += a.encode(&mut s)?;
                len += b.encode(&mut s)?;
            }
        }

        Ok(len)
    }
}

impl Decodable for incrementalmerkletree::bridgetree::Leaf<MerkleNode> {
    fn decode<D: io::Read>(mut d: D) -> core::result::Result<Self, io::Error> {
        let side: bool = Decodable::decode(&mut d)?;

        match side {
            false => {
                let a: MerkleNode = Decodable::decode(&mut d)?;
                Ok(Self::Left(a))
            }
            true => {
                let a: MerkleNode = Decodable::decode(&mut d)?;
                let b: MerkleNode = Decodable::decode(&mut d)?;
                Ok(Self::Right(a, b))
            }
        }
    }
}

impl Encodable for incrementalmerkletree::bridgetree::Checkpoint {
    fn encode<S: io::Write>(&self, mut s: S) -> core::result::Result<usize, io::Error> {
        let mut len = 0;
        len += self.bridges_len().encode(&mut s)?;
        len += self.is_witnessed().encode(&mut s)?;
        len += self.witnessed().encode(&mut s)?;
        len += self.forgotten().encode(&mut s)?;
        Ok(len)
    }
}

impl Decodable for incrementalmerkletree::bridgetree::Checkpoint {
    fn decode<D: io::Read>(mut d: D) -> core::result::Result<Self, io::Error> {
        let bridges_len = Decodable::decode(&mut d)?;
        let is_witnessed = Decodable::decode(&mut d)?;
        let witnessed = Decodable::decode(&mut d)?;
        let forgotten = Decodable::decode(&mut d)?;
        Ok(Self::from_parts(bridges_len, is_witnessed, witnessed, forgotten))
    }
}

impl Encodable for incrementalmerkletree::bridgetree::NonEmptyFrontier<MerkleNode> {
    fn encode<S: io::Write>(&self, mut s: S) -> core::result::Result<usize, io::Error> {
        let mut len = 0;
        len += self.position().encode(&mut s)?;
        len += self.leaf().encode(&mut s)?;
        len += self.ommers().to_vec().encode(&mut s)?;
        Ok(len)
    }
}

impl Decodable for incrementalmerkletree::bridgetree::NonEmptyFrontier<MerkleNode> {
    fn decode<D: io::Read>(mut d: D) -> core::result::Result<Self, io::Error> {
        let position = Decodable::decode(&mut d)?;
        let leaf = Decodable::decode(&mut d)?;
        let ommers = Decodable::decode(&mut d)?;

        match Self::from_parts(position, leaf, ommers) {
            Ok(v) => Ok(v),
            Err(_) => Err(io::Error::new(io::ErrorKind::Other, "FrontierError")),
        }
    }
}

impl Encodable for incrementalmerkletree::bridgetree::AuthFragment<MerkleNode> {
    fn encode<S: io::Write>(&self, mut s: S) -> core::result::Result<usize, io::Error> {
        let mut len = 0;
        len += self.position().encode(&mut s)?;
        len += self.altitudes_observed().encode(&mut s)?;
        len += self.values().to_vec().encode(&mut s)?;
        Ok(len)
    }
}

impl Decodable for incrementalmerkletree::bridgetree::AuthFragment<MerkleNode> {
    fn decode<D: io::Read>(mut d: D) -> core::result::Result<Self, io::Error> {
        let position = Decodable::decode(&mut d)?;
        let altitudes_observed = Decodable::decode(&mut d)?;
        let values = Decodable::decode(&mut d)?;
        Ok(Self::from_parts(position, altitudes_observed, values))
    }
}

impl Encodable for incrementalmerkletree::bridgetree::MerkleBridge<MerkleNode> {
    fn encode<S: io::Write>(&self, mut s: S) -> core::result::Result<usize, io::Error> {
        let mut len = 0;
        len += self.prior_position().encode(&mut s)?;
        len += self.auth_fragments().encode(&mut s)?;
        len += self.frontier().encode(&mut s)?;
        Ok(len)
    }
}

impl Decodable for incrementalmerkletree::bridgetree::MerkleBridge<MerkleNode> {
    fn decode<D: io::Read>(mut d: D) -> core::result::Result<Self, io::Error> {
        let prior_position = Decodable::decode(&mut d)?;
        let auth_fragments = Decodable::decode(&mut d)?;
        let frontier = Decodable::decode(&mut d)?;
        Ok(Self::from_parts(prior_position, auth_fragments, frontier))
    }
}

impl Encodable for incrementalmerkletree::bridgetree::BridgeTree<MerkleNode, 32> {
    fn encode<S: io::Write>(&self, mut s: S) -> core::result::Result<usize, io::Error> {
        let mut len = 0;
        len += self.prior_bridges().to_vec().encode(&mut s)?;
        len += self.current_bridge().encode(&mut s)?;
        len += self.witnessed_indices().encode(&mut s)?;
        len += self.checkpoints().to_vec().encode(&mut s)?;
        len += self.max_checkpoints().encode(&mut s)?;
        Ok(len)
    }
}

impl Decodable for incrementalmerkletree::bridgetree::BridgeTree<MerkleNode, 32> {
    fn decode<D: io::Read>(mut d: D) -> core::result::Result<Self, io::Error> {
        let prior_bridges = Decodable::decode(&mut d)?;
        let current_bridge = Decodable::decode(&mut d)?;
        let saved = Decodable::decode(&mut d)?;
        let checkpoints = Decodable::decode(&mut d)?;
        let max_checkpoints = Decodable::decode(&mut d)?;
        match Self::from_parts(prior_bridges, current_bridge, saved, checkpoints, max_checkpoints) {
            Ok(v) => Ok(v),
            Err(_) => Err(io::Error::new(io::ErrorKind::Other, "BridgeTreeError")),
        }
    }
}
