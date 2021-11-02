use pasta_curves::{arithmetic::FieldExt, pallas};

pub struct Nullifier(pallas::Base);

impl Nullifier {
    pub fn from_bytes(bytes: &[u8; 32]) -> Self {
        pallas::Base::from_bytes(bytes).map(Nullifier).unwrap()
    }

    pub fn to_bytes(self) -> [u8; 32] {
        self.0.to_bytes()
    }

    pub(crate) fn inner(&self) -> pallas::Base {
        self.0
    }
}
