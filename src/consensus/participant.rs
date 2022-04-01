use serde::{Deserialize, Serialize};
use std::io;

use crate::{
    impl_vec, net,
    util::serial::{Decodable, Encodable, SerialDecodable, SerialEncodable, VarInt},
    Result,
};

/// This struct represents a tuple of the form (node_id, epoch_joined, last_epoch_voted).
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, SerialEncodable, SerialDecodable)]
pub struct Participant {
    /// Node id
    pub id: u64,
    /// Epoch node joined the network
    pub joined: u64,
    /// Last epoch node voted
    pub voted: Option<u64>,
}

impl Participant {
    pub fn new(id: u64, joined: u64) -> Participant {
        Participant { id, joined, voted: None }
    }
}

impl net::Message for Participant {
    fn name() -> &'static str {
        "participant"
    }
}

impl_vec!(Participant);
