Warning: can't set `wrap_comments = true`, unstable features are only available in nightly channel.
Warning: can't set `comment_width = 100`, unstable features are only available in nightly channel.
Warning: can't set `imports_granularity = Crate`, unstable features are only available in nightly channel.
Warning: can't set `binop_separator = Back`, unstable features are only available in nightly channel.
Warning: can't set `trailing_semicolon = false`, unstable features are only available in nightly channel.
Warning: can't set `trailing_comma = Vertical`, unstable features are only available in nightly channel.
pub mod arith_chip;
pub mod coin;
pub mod constants;
pub mod diffie_hellman;
pub mod merkle;
pub mod merkle_node2;
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
