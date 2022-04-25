use std::{collections::BTreeMap, io};

use crate::{
    crypto::address::Address,
    impl_vec, net,
    util::serial::{Decodable, Encodable, SerialDecodable, SerialEncodable, VarInt},
    Result,
};

/// This struct represents a tuple of the form:
/// (`node_address`, `epoch_joined`, `last_epoch_voted`)
#[derive(Debug, Clone, PartialEq, SerialEncodable, SerialDecodable)]
pub struct Participant {
    /// Node wallet address
    pub address: Address,
    /// Epoch node joined the network
    pub joined: u64,
    /// Last epoch node voted
    pub voted: Option<u64>,
}

impl Participant {
    pub fn new(address: Address, joined: u64) -> Self {
        Self { address, joined, voted: None }
    }
}

impl net::Message for Participant {
    fn name() -> &'static str {
        "participant"
    }
}

impl Encodable for BTreeMap<Address, Participant> {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += VarInt(self.len() as u64).encode(&mut s)?;
        for c in self.iter() {
            len += c.1.encode(&mut s)?;
        }
        Ok(len)
    }
}

impl Decodable for BTreeMap<Address, Participant> {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        let len = VarInt::decode(&mut d)?.0;
        let mut ret = BTreeMap::new();
        for _ in 0..len {
            let participant: Participant = Decodable::decode(&mut d)?;
            ret.insert(participant.address, participant);
        }
        Ok(ret)
    }
}

impl_vec!(Participant);
