use std::{collections::BTreeMap, io};

use crate::{
    impl_vec, net,
    util::serial::{Decodable, Encodable, SerialDecodable, SerialEncodable, VarInt},
    Result,
};

/// This struct represents a tuple of the form:
/// (`node_id`, `epoch_joined`, `last_epoch_voted`)
#[derive(Debug, Clone, PartialEq, SerialEncodable, SerialDecodable)]
pub struct Participant {
    // Node ID
    pub id: u64,
    /// Epoch node joined the network
    pub joined: u64,
    /// Last epoch node voted
    pub voted: Option<u64>,
}

impl Participant {
    pub fn new(id: u64, joined: u64) -> Self {
        Self { id, joined, voted: None }
    }
}

impl net::Message for Participant {
    fn name() -> &'static str {
        "participant"
    }
}

impl Encodable for BTreeMap<u64, Participant> {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += VarInt(self.len() as u64).encode(&mut s)?;
        for c in self.iter() {
            len += c.1.encode(&mut s)?;
        }
        Ok(len)
    }
}

impl Decodable for BTreeMap<u64, Participant> {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        let len = VarInt::decode(&mut d)?.0;
        let mut ret = BTreeMap::new();
        for _ in 0..len {
            let participant: Participant = Decodable::decode(&mut d)?;
            ret.insert(participant.id, participant);
        }
        Ok(ret)
    }
}

impl_vec!(Participant);
