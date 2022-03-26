use serde::{Deserialize, Serialize};
use std::io;

use super::block::BlockProposal;

use crate::{
    crypto::{keypair::PublicKey, schnorr::Signature},
    impl_vec, net,
    util::serial::{Decodable, Encodable, VarInt},
    Result,
};

/// This struct represents a tuple of the form (vote, B, id).
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
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

impl Encodable for Vote {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += self.node_public_key.encode(&mut s)?;
        len += self.vote.encode(&mut s)?;
        len += self.block.encode(&mut s)?;
        len += self.id.encode(&mut s)?;
        Ok(len)
    }
}

impl Decodable for Vote {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        Ok(Self {
            node_public_key: Decodable::decode(&mut d)?,
            vote: Decodable::decode(&mut d)?,
            block: Decodable::decode(&mut d)?,
            id: Decodable::decode(&mut d)?,
        })
    }
}

impl_vec!(Vote);
