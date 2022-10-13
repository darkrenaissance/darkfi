use pasta_curves::{group::ff::PrimeField, pallas};

/// The `Nullifier` is represented as a base field element.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Nullifier(pallas::Base);

impl Nullifier {
    /// Reference the raw inner base field element
    pub fn inner(&self) -> pallas::Base {
        self.0
    }

    /// Try to create a `Nullifier` type from the given 32 bytes.
    /// Returns `Some` if the bytes fit in the base field, and `None` if not.
    pub fn from_bytes(bytes: [u8; 32]) -> Option<Self> {
        let n = pallas::Base::from_repr(bytes);
        match bool::from(n.is_some()) {
            true => Some(Self(n.unwrap())),
            false => None,
        }
    }

    /// Convert the `Nullifier` type into 32 raw bytes
    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.to_repr()
    }
}

impl From<pallas::Base> for Nullifier {
    fn from(x: pallas::Base) -> Self {
        Self(x)
    }
}
