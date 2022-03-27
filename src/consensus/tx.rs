use serde::{Deserialize, Serialize};
use std::io;

use crate::{
    impl_vec, net,
    util::serial::{Decodable, Encodable, SerialDecodable, SerialEncodable, VarInt},
    Result,
};

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, SerialEncodable, SerialDecodable)]
pub struct Tx {
    pub hash: u32, // Change this to a proper hash type
    pub payload: String,
}

impl net::Message for Tx {
    fn name() -> &'static str {
        "tx"
    }
}

impl_vec!(Tx);
