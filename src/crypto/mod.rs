pub mod constants;
pub mod diffie_hellman;
//pub mod merkle;
//pub mod merkle_node;
pub mod mint_proof;
pub mod note;
pub mod pasta_serial;
pub mod proof;
pub mod schnorr;
pub mod spend_proof;
pub mod types;
pub mod util;

#[derive(Clone)]
pub struct OwnCoin {
    pub coin: types::DrkCoin,
    pub note: note::Note,
    pub secret: types::DrkSecretKey,
    //pub witness: merkle::IncrementalWitness<merkle_node::MerkleNode>,
    pub nullifier: types::DrkNullifier,
}

pub type OwnCoins = Vec<OwnCoin>;
