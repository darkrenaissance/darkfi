use serde::{Deserialize, Serialize};
use std::io;

use super::block::BlockProposal;

use crate::{
    crypto::{keypair::PublicKey, schnorr::Signature},
    impl_vec, net,
    util::serial::{Decodable, Encodable, SerialDecodable, SerialEncodable, VarInt},
    Result,
};

/// This struct represents a tuple of the form (vote, B, id).
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, SerialDecodable, SerialEncodable)]
pub struct Vote {
    /// Node public key
    pub node_public_key: PublicKey,
    /// signed block
    pub vote: Signature,
    /// block hash to vote on
    pub block: BlockProposal,
    /// node id
    pub id: u64,
}

impl Vote {
    pub fn new(node_public_key: PublicKey, vote: Signature, block: BlockProposal, id: u64) -> Vote {
        Vote { node_public_key, vote, block, id }
    }
}

impl net::Message for Vote {
    fn name() -> &'static str {
        "vote"
    }
}

impl_vec!(Vote);
