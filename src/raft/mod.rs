use std::{io, net::SocketAddr};

use crate::{
    util::serial::{serialize, Decodable, Encodable, SerialDecodable, SerialEncodable, VarInt},
    Result,
};

mod datastore;
mod p2p;
mod raft;

use datastore::DataStore;
use p2p::ProtocolRaft;
pub use raft::Raft;

#[derive(PartialEq, Eq, Debug)]
pub enum Role {
    Follower,
    Candidate,
    Leader,
}

#[derive(SerialDecodable, SerialEncodable, Clone, Debug)]
pub struct VoteRequest {
    node_id: NodeId,
    current_term: u64,
    log_length: u64,
    last_term: u64,
}

#[derive(SerialDecodable, SerialEncodable, Clone, Debug)]
pub struct VoteResponse {
    node_id: NodeId,
    current_term: u64,
    ok: bool,
}

#[derive(SerialDecodable, SerialEncodable, Clone, Debug)]
pub struct LogRequest {
    leader_id: NodeId,
    current_term: u64,
    prefix_len: u64,
    prefix_term: u64,
    commit_length: u64,
    suffix: Logs,
}

#[derive(SerialDecodable, SerialEncodable, Clone, Debug)]
pub struct BroadcastMsgRequest(Vec<u8>);

#[derive(SerialDecodable, SerialEncodable, Clone, Debug)]
pub struct LogResponse {
    node_id: NodeId,
    current_term: u64,
    ack: u64,
    ok: bool,
}

impl VoteResponse {
    pub fn set_ok(&mut self, ok: bool) {
        self.ok = ok;
    }
}

#[derive(Clone, Debug, SerialDecodable, SerialEncodable)]
pub struct Log {
    term: u64,
    msg: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, SerialDecodable, SerialEncodable)]
pub struct NodeId(pub Vec<u8>);

impl From<SocketAddr> for NodeId {
    fn from(addr: SocketAddr) -> Self {
        let ser = serialize(&addr);
        let hash = blake3::hash(&ser).as_bytes().to_vec();
        Self(hash)
    }
}

#[derive(Clone, Debug)]
pub struct Logs(pub Vec<Log>);

impl Logs {
    pub fn len(&self) -> u64 {
        self.0.len() as u64
    }
    pub fn push(&mut self, d: &Log) {
        self.0.push(d.clone());
    }

    pub fn slice_from(&self, start: u64) -> Self {
        Self(self.0[start as usize..].to_vec())
    }

    pub fn slice_to(&self, end: u64) -> Self {
        Self(self.0[..end as usize].to_vec())
    }

    pub fn get(&self, index: u64) -> Log {
        self.0[index as usize].clone()
    }

    pub fn to_vec(&self) -> Vec<Log> {
        self.0.clone()
    }
}

#[derive(SerialDecodable, SerialEncodable, Clone, Debug)]
pub struct NetMsg {
    id: u32,
    recipient_id: Option<NodeId>,
    method: NetMsgMethod,
    payload: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum NetMsgMethod {
    LogResponse = 0,
    LogRequest = 1,
    VoteResponse = 2,
    VoteRequest = 3,
    BroadcastRequest = 4,
}

impl Encodable for NetMsgMethod {
    fn encode<S: io::Write>(&self, s: S) -> Result<usize> {
        let len: usize = match self {
            Self::LogResponse => 0,
            Self::LogRequest => 1,
            Self::VoteResponse => 2,
            Self::VoteRequest => 3,
            Self::BroadcastRequest => 4,
        };
        (len as u8).encode(s)
    }
}

impl Decodable for NetMsgMethod {
    fn decode<D: io::Read>(d: D) -> Result<Self> {
        let com: u8 = Decodable::decode(d)?;
        Ok(match com {
            0 => Self::LogResponse,
            1 => Self::LogRequest,
            2 => Self::VoteResponse,
            3 => Self::VoteRequest,
            _ => Self::BroadcastRequest,
        })
    }
}

impl Encodable for Logs {
    fn encode<S: io::Write>(&self, s: S) -> Result<usize> {
        encode_vec(&self.0, s)
    }
}

impl Decodable for Logs {
    fn decode<D: io::Read>(d: D) -> Result<Self> {
        Ok(Self(decode_vec(d)?))
    }
}

fn encode_vec<T: Encodable, S: io::Write>(vec: &[T], mut s: S) -> Result<usize> {
    let mut len = 0;
    len += VarInt(vec.len() as u64).encode(&mut s)?;
    for c in vec.iter() {
        len += c.encode(&mut s)?;
    }
    Ok(len)
}

fn decode_vec<T: Decodable, D: io::Read>(mut d: D) -> Result<Vec<T>> {
    let len = VarInt::decode(&mut d)?.0;
    let mut ret = Vec::with_capacity(len as usize);
    for _ in 0..len {
        ret.push(Decodable::decode(&mut d)?);
    }
    Ok(ret)
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {}
}
