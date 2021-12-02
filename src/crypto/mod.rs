pub mod arith_chip;
pub mod coin;
pub mod constants;
pub mod diffie_hellman;
pub mod keypair;
pub mod merkle_node;
pub mod mint_proof;
pub mod note;
pub mod nullifier;
pub mod proof;
pub mod schnorr;
pub mod spend_proof;
pub mod util;

pub(crate) use mint_proof::MintRevealedValues;
pub(crate) use proof::Proof;
pub(crate) use spend_proof::SpendRevealedValues;

use keypair::SecretKey;

#[derive(Clone)]
pub struct OwnCoin {
    pub coin: coin::Coin,
    pub note: note::Note,
    pub secret: SecretKey,
    //pub witness: merkle::IncrementalWitness<merkle_node::MerkleNode>,
    //pub witness: BridgeFrontier<merkle::MerkleHash, 32>,
    pub nullifier: nullifier::Nullifier,
}

pub type OwnCoins = Vec<OwnCoin>;
