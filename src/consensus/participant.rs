use crate::{
    crypto::{address::Address, keypair::PublicKey},
    net,
    util::serial::{SerialDecodable, SerialEncodable},
};

/// This struct represents a tuple of the form:
/// (`node_address`, `slot_joined`, `last_slot_voted`, `slot_quarantined`)
#[derive(Debug, Clone, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub struct Participant {
    /// Node public key
    pub public_key: PublicKey,
    /// Node wallet address
    pub address: Address,
    /// Slot node joined the network
    pub joined: u64,
    /// Last slot node voted
    pub voted: Option<u64>,
    /// Slot participant was quarantined by the node
    pub quarantined: Option<u64>,
}

impl Participant {
    pub fn new(public_key: PublicKey, address: Address, joined: u64) -> Self {
        Self { public_key, address, joined, voted: None, quarantined: None }
    }
}

impl net::Message for Participant {
    fn name() -> &'static str {
        "participant"
    }
}
