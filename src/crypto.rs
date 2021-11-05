use pasta_curves::{
    arithmetic::{CurveExt, FieldExt},
    pallas,
};

use crate::constants::fixed_bases::{
    VALUE_COMMITMENT_PERSONALIZATION, VALUE_COMMITMENT_R_BYTES, VALUE_COMMITMENT_V_BYTES,
};

#[allow(non_snake_case)]
pub fn pedersen_commitment(value: u64, blind: pallas::Scalar) -> pallas::Point {
    let hasher = pallas::Point::hash_to_curve(VALUE_COMMITMENT_PERSONALIZATION);
    let V = hasher(&VALUE_COMMITMENT_V_BYTES);
    let R = hasher(&VALUE_COMMITMENT_R_BYTES);
    let value = pallas::Scalar::from_u64(value);

    V * value + R * blind
}

//////////////////////////////////
// copied from mod.rs
// todo: go through this code

pub mod coin;
pub mod constants;
pub mod diffie_hellman;
pub mod merkle;
pub mod mint_proof;
pub mod note;
pub mod nullifier;
pub mod pasta_serial;
pub mod proof;
pub mod schnorr;
pub mod spend_proof;
pub mod util;

pub(crate) use {mint_proof::MintRevealedValues, proof::Proof, spend_proof::SpendRevealedValues};

use crate::types::DrkSecretKey;

#[derive(Clone)]
pub struct OwnCoin {
    pub coin: coin::Coin,
    pub note: note::Note,
    pub secret: DrkSecretKey,
    //pub witness: merkle::IncrementalWitness<merkle_node::MerkleNode>,
    //pub witness: BridgeFrontier<merkle::MerkleHash, 32>,
    pub nullifier: nullifier::Nullifier,
}

pub type OwnCoins = Vec<OwnCoin>;
