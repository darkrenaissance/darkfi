use pasta_curves::{group::ff::PrimeField, pallas};

use crate::{
    crypto::{keypair::SecretKey, util::poseidon_hash},
    serial::{SerialDecodable, SerialEncodable},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub struct Nullifier(pub pallas::Base);

impl Nullifier {
    pub fn new(secret: SecretKey, serial: pallas::Base) -> Self {
        let nullifier = poseidon_hash::<2>([secret.0, serial]);
        Nullifier(nullifier)
    }

    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        pallas::Base::from_repr(bytes).map(Nullifier).unwrap()
    }

    pub fn to_bytes(self) -> [u8; 32] {
        self.0.to_repr()
    }

    pub(crate) fn inner(&self) -> pallas::Base {
        self.0
    }
}
