pub mod address;
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
pub mod types;
pub mod util;

pub use mint_proof::MintRevealedValues;
pub use proof::Proof;
pub use spend_proof::SpendRevealedValues;

use keypair::SecretKey;

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct OwnCoin {
    pub coin: coin::Coin,
    pub note: note::Note,
    pub secret: SecretKey,
    pub nullifier: nullifier::Nullifier,
}

pub type OwnCoins = Vec<OwnCoin>;
