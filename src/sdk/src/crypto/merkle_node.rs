use core::{fmt, str::FromStr};
use std::{io, iter};

use darkfi_serial::{SerialDecodable, SerialEncodable};
use halo2_gadgets::sinsemilla::primitives::HashDomain;
use incrementalmerkletree::{Altitude, Hashable};
use lazy_static::lazy_static;
use pasta_curves::{
    group::ff::{PrimeField, PrimeFieldBits},
    pallas,
};
use subtle::{Choice, ConditionallySelectable};

use crate::crypto::constants::{
    sinsemilla::{i2lebsp_k, L_ORCHARD_MERKLE, MERKLE_CRH_PERSONALIZATION},
    MERKLE_DEPTH_ORCHARD,
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

/// The `MerkleNode` is represented as a base field element.
#[repr(C)]
#[derive(Debug, Clone, Copy, Ord, PartialOrd, Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct MerkleNode(pallas::Base);

impl MerkleNode {
    /// Reference the raw inner base field element
    pub fn inner(&self) -> pallas::Base {
        self.0
    }

    /// Try to create a `MerkleNode` type from the given 32 bytes.
    /// Returns `Some` if the bytes fit in the base field, and `None` if not.
    pub fn from_bytes(bytes: [u8; 32]) -> Option<Self> {
        let n = pallas::Base::from_repr(bytes);
        match bool::from(n.is_some()) {
            true => Some(Self(n.unwrap())),
            false => None,
        }
    }

    /// Convert the `MerkleNode` type into 32 raw bytes
    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.to_repr()
    }
}

impl From<pallas::Base> for MerkleNode {
    fn from(x: pallas::Base) -> Self {
        Self(x)
    }
}

impl fmt::Display for MerkleNode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", bs58::encode(self.to_bytes()).into_string())
    }
}

impl FromStr for MerkleNode {
    type Err = io::Error;

    /// Tries to decode a base58 string into a `MerkleNode` type.
    /// This string is the same string received by calling `MerkleNode::to_string()`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bytes = match bs58::decode(s).into_vec() {
            Ok(v) => v,
            Err(e) => return Err(io::Error::new(io::ErrorKind::Other, e)),
        };

        if bytes.len() != 32 {
            return Err(io::Error::new(io::ErrorKind::Other, "Length of decoded bytes is not 32"))
        }

        if let Some(merkle_node) = Self::from_bytes(bytes.try_into().unwrap()) {
            return Ok(merkle_node)
        }

        Err(io::Error::new(io::ErrorKind::Other, "Invalid bytes for MerkleNode"))
    }
}

impl ConditionallySelectable for MerkleNode {
    fn conditional_select(a: &Self, b: &Self, choice: Choice) -> Self {
        Self(pallas::Base::conditional_select(&a.0, &b.0, choice))
    }
}

impl Hashable for MerkleNode {
    fn empty_leaf() -> Self {
        Self(*UNCOMMITTED_ORCHARD)
    }

    /// Implements `MerkleCRH^Orchard` as defined in
    /// <https://zips.z.cash/protocol/protocol.pdf#orchardmerklecrh>
    ///
    /// The layer with 2^n nodes is called "layer n":
    ///     - leaves are at layer MERKLE_DEPTH_ORCHARD = 32;
    ///     - the root is at layer 0.
    /// `l` is MERKLE_DEPTH_ORCHARD - layer - 1.
    ///     - when hashing two leaves, we produce a node on the layer
    ///       above the the leaves, i.e. layer = 31, l = 0
    ///     - when hashing to the final root, we produce the anchor
    ///       with layer = 0, l = 31.
    fn combine(altitude: Altitude, left: &Self, right: &Self) -> Self {
        // MerkleCRH Sinsemilla hash domain.
        let domain = HashDomain::new(MERKLE_CRH_PERSONALIZATION);

        Self(
            domain
                .hash(
                    iter::empty()
                        .chain(i2lebsp_k(altitude.into()).iter().copied())
                        .chain(left.inner().to_le_bits().iter().by_vals().take(L_ORCHARD_MERKLE))
                        .chain(right.inner().to_le_bits().iter().by_vals().take(L_ORCHARD_MERKLE)),
                )
                .unwrap_or(pallas::Base::zero()),
        )
    }

    fn empty_root(altitude: Altitude) -> Self {
        EMPTY_ROOTS[<usize>::from(altitude)]
    }
}
