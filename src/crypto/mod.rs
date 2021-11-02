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

/*
use crate::types::*;

#[derive(Clone)]
pub struct OwnCoin {
    pub coin: DrkCoin,
    pub note: note::Note,
    pub secret: DrkSecretKey,
    //pub witness: merkle::IncrementalWitness<merkle_node::MerkleNode>,
    pub nullifier: DrkNullifier,
}

pub type OwnCoins = Vec<OwnCoin>;
*/
