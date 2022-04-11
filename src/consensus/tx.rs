use std::io;

use crate::{
    impl_vec, net,
    util::serial::{Decodable, Encodable, SerialDecodable, SerialEncodable, VarInt},
    Result,
};

/// Temporary structure used to represent transactions.
#[derive(Debug, Clone, PartialEq, SerialEncodable, SerialDecodable)]
pub struct Tx {
    pub payload: String,
}

impl net::Message for Tx {
    fn name() -> &'static str {
        "tx"
    }
}

impl_vec!(Tx);
