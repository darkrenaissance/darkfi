use darkfi_serial::{SerialDecodable, SerialEncodable};
use pasta_curves::pallas;

use crate::{
    crypto::{address::Address, keypair::PublicKey},
    net,
};

/// This struct represents a tuple of the form:
/// (`public_key`, `node_address`, `last_slot_seen`,`slot_quarantined`)
#[derive(Debug, Clone, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub struct Participant {
    /// Node public key
    pub public_key: PublicKey,
    /// Node wallet address
    pub address: Address,
    /// Node current epoch competing coins public inputs
    pub coins: Vec<Vec<Vec<pallas::Base>>>,
}

impl Participant {
    pub fn new(
        public_key: PublicKey,
        address: Address,
        coins: Vec<Vec<Vec<pallas::Base>>>,
    ) -> Self {
        Self { public_key, address, coins }
    }
}

impl net::Message for Participant {
    fn name() -> &'static str {
        "participant"
    }
}
