use darkfi_serial::{SerialDecodable, SerialEncodable};
use pasta_curves::{group::ff::PrimeField, pallas};

use super::{keypair::SecretKey, note::Note, nullifier::Nullifier};

#[derive(Clone, Copy, PartialEq, Eq, Debug, SerialEncodable, SerialDecodable)]
pub struct Coin(pub pallas::Base);

impl Coin {
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        pallas::Base::from_repr(bytes).map(Coin).unwrap()
    }

    pub fn to_bytes(self) -> [u8; 32] {
        self.0.to_repr()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub struct OwnCoin {
    pub coin: Coin,
    pub note: Note,
    pub secret: SecretKey,
    pub nullifier: Nullifier,
    pub leaf_position: incrementalmerkletree::Position,
}
