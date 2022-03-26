use serde::{Deserialize, Serialize};
use std::io;

use crate::{
    impl_vec, net,
    util::serial::{Decodable, Encodable, VarInt},
    Result,
};

#[derive(Debug, Clone, Deserialize, Serialize, Eq, Hash, PartialEq)]
pub struct Tx {
    pub hash: u32, // Change this to a proper hash type
    pub payload: String,
}

impl net::Message for Tx {
    fn name() -> &'static str {
        "tx"
    }
}

impl Encodable for Tx {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += self.hash.encode(&mut s)?;
        len += self.payload.encode(&mut s)?;
        Ok(len)
    }
}

impl Decodable for Tx {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        Ok(Self { hash: Decodable::decode(&mut d)?, payload: Decodable::decode(&mut d)? })
    }
}

impl_vec!(Tx);
