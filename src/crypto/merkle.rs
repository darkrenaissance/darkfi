use std::iter;

use halo2_gadgets::primitives::sinsemilla::HashDomain;
use incrementalmerkletree::{Altitude, Hashable};
use lazy_static::lazy_static;
use pasta_curves::{
    arithmetic::FieldExt,
    group::ff::{PrimeField, PrimeFieldBits},
    pallas,
};

use super::{
    coin::Coin,
    constants::{
        sinsemilla::{i2lebsp_k, MERKLE_CRH_PERSONALIZATION},
        util::gen_const_array_with_default,
    },
};

// TODO: to constants
const MERKLE_DEPTH_ORCHARD: usize = 32;

lazy_static! {
    static ref UNCOMMITTED_ORCHARD: pallas::Base = pallas::Base::from_u64(2);
    pub(crate) static ref EMPTY_ROOTS: Vec<MerkleHash> = {
        iter::empty()
            .chain(Some(MerkleHash::empty_leaf()))
            .chain((0..MERKLE_DEPTH_ORCHARD).scan(MerkleHash::empty_leaf(), |state, l| {
                let l = l as u8;
                *state = MerkleHash::combine(l.into(), state, state);
                Some(*state)
            }))
            .collect()
    };
}

#[derive(Copy, Clone, Debug)]
pub struct MerkleHash(pallas::Base);

impl MerkleHash {
    pub fn from_coin(value: &Coin) -> Self {
        MerkleHash(value.inner())
    }

    pub(crate) fn inner(&self) -> pallas::Base {
        self.0
    }

    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.to_bytes()
    }

    pub fn from_bytes(bytes: &[u8; 32]) -> Self {
        pallas::Base::from_bytes(bytes).map(MerkleHash).unwrap()
    }
}

impl Hashable for MerkleHash {
    fn empty_leaf() -> Self {
        MerkleHash(*UNCOMMITTED_ORCHARD)
    }

    fn combine(altitude: Altitude, left: &Self, right: &Self) -> Self {
        let domain = HashDomain::new(MERKLE_CRH_PERSONALIZATION);

        MerkleHash(
            domain
                .hash(
                    iter::empty()
                        .chain(i2lebsp_k(altitude.into()).iter().copied())
                        .chain(left.0.to_le_bits().iter().by_val().take(255))
                        .chain(right.0.to_le_bits().iter().by_val().take(255)),
                )
                .unwrap_or(pallas::Base::zero()),
        )
    }

    fn empty_root(altitude: Altitude) -> Self {
        EMPTY_ROOTS[<usize>::from(altitude)]
    }
}

pub struct Anchor(pallas::Base);

impl From<pallas::Base> for Anchor {
    fn from(anchor_field: pallas::Base) -> Anchor {
        Anchor(anchor_field)
    }
}

impl From<MerkleHash> for Anchor {
    fn from(anchor: MerkleHash) -> Anchor {
        Anchor(anchor.0)
    }
}

impl Anchor {
    pub fn from_bytes(bytes: [u8; 32]) -> Anchor {
        pallas::Base::from_repr(bytes).map(Anchor).unwrap()
    }

    pub fn to_bytes(self) -> [u8; 32] {
        self.0.to_repr()
    }
}

#[derive(Debug)]
pub struct MerklePath {
    position: u32,
    auth_path: [MerkleHash; MERKLE_DEPTH_ORCHARD],
}

impl MerklePath {
    pub fn new(position: u32, auth_path: [pallas::Base; MERKLE_DEPTH_ORCHARD]) -> Self {
        Self {
            position,
            auth_path: gen_const_array_with_default(MerkleHash::empty_leaf(), |i| {
                MerkleHash(auth_path[i])
            }),
        }
    }

    pub fn root(&self, coin: Coin) -> Anchor {
        self.auth_path
            .iter()
            .enumerate()
            .fold(MerkleHash::from_coin(&coin), |node, (l, sibling)| {
                let l = l as u8;
                if self.position & (1 << l) == 0 {
                    MerkleHash::combine(l.into(), &node, sibling)
                } else {
                    MerkleHash::combine(l.into(), sibling, &node)
                }
            })
            .into()
    }

    pub fn position(&self) -> u32 {
        self.position
    }

    pub fn auth_path(&self) -> [MerkleHash; MERKLE_DEPTH_ORCHARD] {
        self.auth_path
    }
}
