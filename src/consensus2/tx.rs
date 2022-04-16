use std::io;

use crate::{
    impl_vec, net,
    tx::Transaction,
    util::serial::{Decodable, Encodable, SerialDecodable, SerialEncodable, VarInt},
    Result,
};

/// Temporary transaction representation.
#[derive(Debug, Clone, PartialEq, SerialEncodable, SerialDecodable)]
pub struct Tx(pub Transaction);

impl net::Message for Tx {
    fn name() -> &'static str {
        "tx"
    }
}

impl_vec!(Tx);
