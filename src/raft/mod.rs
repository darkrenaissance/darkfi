use std::{collections::HashMap, io, net::SocketAddr};

use crate::{
    util::serial::{serialize, Decodable, Encodable, SerialDecodable, SerialEncodable, VarInt},
    Error, Result,
};

mod datastore;
mod protocol_raft;
mod raft;

use datastore::DataStore;
use protocol_raft::ProtocolRaft;
pub use raft::Raft;

#[derive(PartialEq, Eq, Debug)]
pub enum Role {
    Follower,
    Candidate,
    Leader,
}

#[derive(SerialDecodable, SerialEncodable, Clone, Debug)]
pub struct SyncRequest {
    logs_len: u64,
    last_term: u64,
}

#[derive(SerialDecodable, SerialEncodable, Clone, Debug)]
pub struct SyncResponse {
    logs: Logs,
    commit_length: u64,
    leader_id: NodeId,
    wipe: bool,
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
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn push(&mut self, d: &Log) {
        self.0.push(d.clone());
    }

    pub fn slice_from(&self, start: u64) -> Option<Self> {
        if self.len() >= start {
            return Some(Self(self.0[start as usize..].to_vec()))
        }
        None
    }

    pub fn slice_to(&self, end: u64) -> Self {
        for i in (0..end).rev() {
            if self.len() >= i {
                return Self(self.0[..i as usize].to_vec())
            }
        }
        Self(vec![])
    }

    pub fn get(&self, index: u64) -> Result<Log> {
        match self.0.get(index as usize) {
            Some(l) => Ok(l.clone()),
            None => Err(Error::RaftError("unable to indexing into vector".into())),
        }
    }

    pub fn to_vec(&self) -> Vec<Log> {
        self.0.clone()
    }
}

#[derive(Clone, Debug)]
pub struct MapLength(pub HashMap<NodeId, u64>);

impl MapLength {
    pub fn get(&self, key: &NodeId) -> Result<u64> {
        match self.0.get(key) {
            Some(v) => Ok(*v),
            None => Err(Error::RaftError("unable to indexing into HashMap".into())),
        }
    }

    pub fn insert(&mut self, key: &NodeId, value: u64) {
        self.0.insert(key.clone(), value);
    }
}

#[derive(SerialDecodable, SerialEncodable, Clone, Debug)]
pub struct NetMsg {
    id: u64,
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
    // this only used for listener node
    SyncRequest = 5,
    SyncResponse = 6,
}

impl Encodable for NetMsgMethod {
    fn encode<S: io::Write>(&self, s: S) -> Result<usize> {
        let len: usize = match self {
            Self::LogResponse => 0,
            Self::LogRequest => 1,
            Self::VoteResponse => 2,
            Self::VoteRequest => 3,
            Self::BroadcastRequest => 4,
            Self::SyncRequest => 5,
            Self::SyncResponse => 6,
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
            4 => Self::BroadcastRequest,
            5 => Self::SyncRequest,
            _ => Self::SyncResponse,
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
