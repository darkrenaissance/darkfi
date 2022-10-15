use darkfi_serial::{SerialDecodable, SerialEncodable};
use pasta_curves::{group::ff::PrimeField, pallas};

#[derive(Clone, Copy, Debug, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub struct Nullifier(pallas::Base);

impl Nullifier {
    pub fn from_bytes(bytes: [u8; 32]) -> Option<Self> {
        let n = pallas::Base::from_repr(bytes);
        match bool::from(n.is_some()) {
            true => Some(Self(n.unwrap())),
            false => None,
        }
    }

    pub fn to_bytes(self) -> [u8; 32] {
        self.0.to_repr()
    }

    pub fn inner(&self) -> pallas::Base {
        self.0
    }
}

impl From<pallas::Base> for Nullifier {
    fn from(x: pallas::Base) -> Self {
        Self(x)
    }
}
