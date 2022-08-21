use std::io;

use halo2_gadgets::poseidon::primitives as poseidon;
use pasta_curves::{group::ff::PrimeField, pallas};

use crate::{
    crypto::keypair::SecretKey,
    util::serial::{Decodable, Encodable, ReadExt, WriteExt},
    Result,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Nullifier(pub pallas::Base);

impl Nullifier {
    pub fn new(secret: SecretKey, serial: pallas::Base) -> Self {
        let nullifier = [secret.0, serial];
        let nullifier =
            poseidon::Hash::<_, poseidon::P128Pow5T3, poseidon::ConstantLength<2>, 3, 2>::init()
                .hash(nullifier);
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

impl Encodable for Nullifier {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        s.write_slice(&self.to_bytes()[..])?;
        Ok(32)
    }
}

impl Decodable for Nullifier {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        let mut bytes = [0u8; 32];
        d.read_slice(&mut bytes)?;
        let result = Self::from_bytes(bytes);
        Ok(result)
    }
}
