Warning: can't set `wrap_comments = true`, unstable features are only available in nightly channel.
Warning: can't set `comment_width = 100`, unstable features are only available in nightly channel.
Warning: can't set `imports_granularity = Crate`, unstable features are only available in nightly channel.
Warning: can't set `binop_separator = Back`, unstable features are only available in nightly channel.
Warning: can't set `trailing_semicolon = false`, unstable features are only available in nightly channel.
Warning: can't set `trailing_comma = Vertical`, unstable features are only available in nightly channel.
use std::io;

use pasta_curves::{arithmetic::FieldExt, pallas};

use crate::{
    serial::{Decodable, Encodable, ReadExt, WriteExt},
    Result,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Nullifier(pub(crate) pallas::Base);

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
        let result = Self::from_bytes(&bytes);
        Ok(result)
    }
}
