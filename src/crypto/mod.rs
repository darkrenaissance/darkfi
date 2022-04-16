pub mod address;
pub mod coin;
pub mod constants;
pub mod diffie_hellman;
pub mod keypair;
//pub mod loader;
pub mod burn_proof;
pub mod merkle_node;
pub mod mint_proof;
pub mod note;
pub mod nullifier;
pub mod proof;
pub mod schnorr;
pub mod token_id;
pub mod token_list;
pub mod types;
pub mod util;

pub use burn_proof::BurnRevealedValues;
pub use mint_proof::MintRevealedValues;
pub use proof::Proof;

use keypair::SecretKey;

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct OwnCoin {
    pub coin: coin::Coin,
    pub note: note::Note,
    pub secret: SecretKey,
    pub nullifier: nullifier::Nullifier,
    pub leaf_position: incrementalmerkletree::Position,
}

pub type OwnCoins = Vec<OwnCoin>;
