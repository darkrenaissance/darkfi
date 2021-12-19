use std::io;

use crate::{
    darkpulse::Ciphertext,
    net::messages::Message,
    serial::{Decodable, Encodable},
    Result,
};

#[derive(Clone)]
pub struct GetSlabsMessage {
    pub slabs_hash: Vec<[u8; 32]>,
}

#[derive(Clone)]
pub struct InvMessage {
    pub slabs_hash: Vec<[u8; 32]>,
}

#[derive(Clone)]
pub struct SlabMessage {
    pub nonce: [u8; 12],
    pub ciphertext: Ciphertext,
}

#[derive(Clone)]
pub struct SyncMessage {}

impl Message for SlabMessage {
    fn name() -> &'static str {
        "slab"
    }
}

impl Message for GetSlabsMessage {
    fn name() -> &'static str {
        "getslabs"
    }
}
impl Message for InvMessage {
    fn name() -> &'static str {
        "inv"
    }
}
impl Message for SyncMessage {
    fn name() -> &'static str {
        "sync"
    }
}

impl Encodable for GetSlabsMessage {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += self.slabs_hash.encode(&mut s)?;
        Ok(len)
    }
}

impl Decodable for GetSlabsMessage {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        Ok(Self { slabs_hash: Decodable::decode(&mut d)? })
    }
}

impl Encodable for SlabMessage {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += self.nonce.encode(&mut s)?;
        len += self.ciphertext.encode(&mut s)?;
        Ok(len)
    }
}

impl Decodable for SlabMessage {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        Ok(Self { nonce: Decodable::decode(&mut d)?, ciphertext: Decodable::decode(&mut d)? })
    }
}

impl Encodable for InvMessage {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += self.slabs_hash.encode(&mut s)?;
        Ok(len)
    }
}

impl Decodable for InvMessage {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        Ok(Self { slabs_hash: Decodable::decode(&mut d)? })
    }
}

impl Encodable for SyncMessage {
    fn encode<S: io::Write>(&self, _s: S) -> Result<usize> {
        Ok(0)
    }
}

impl Decodable for SyncMessage {
    fn decode<D: io::Read>(_d: D) -> Result<Self> {
        Ok(Self {})
    }
}
