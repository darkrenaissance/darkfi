use std::{io, net::SocketAddr};

use borsh::{BorshDeserialize, BorshSerialize};

use darkfi::util::serial::{serialize, Decodable, Encodable, SerialDecodable, SerialEncodable};

pub mod datastore;
pub mod p2p;
pub mod raft;

pub use datastore::DataStore;
pub use p2p::ProtocolRaft;

#[derive(PartialEq, Eq)]
pub enum Role {
    Follower,
    Candidate,
    Leader,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug)]
pub struct VoteRequest {
    node_id: NodeId,
    current_term: u64,
    log_length: u64,
    last_term: u64,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug)]
pub struct VoteResponse {
    node_id: NodeId,
    current_term: u64,
    ok: bool,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug)]
pub struct LogRequest {
    leader_id: NodeId,
    current_term: u64,
    prefix_len: u64,
    prefix_term: u64,
    commit_length: u64,
    suffix: VecR<Log>,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug)]
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

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, SerialDecodable, SerialEncodable)]
pub struct Log {
    term: u64,
    msg: Vec<u8>,
}

#[derive(
    BorshSerialize,
    BorshDeserialize,
    Clone,
    Debug,
    Eq,
    PartialEq,
    Hash,
    SerialDecodable,
    SerialEncodable,
)]
pub struct NodeId(pub Vec<u8>);

impl From<SocketAddr> for NodeId {
    fn from(addr: SocketAddr) -> Self {
        let ser = serialize(&addr);
        let hash = blake3::hash(&ser).as_bytes().to_vec();
        Self(hash)
    }
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug)]
pub struct VecR<T: BorshSerialize + BorshDeserialize>(pub Vec<T>);

impl<T: BorshSerialize + BorshDeserialize + Clone> VecR<T> {
    pub fn len(&self) -> u64 {
        self.0.len() as u64
    }
    pub fn push(&mut self, d: &T) {
        self.0.push(d.clone());
    }

    pub fn slice_from(&self, start: u64) -> Self {
        Self(self.0[start as usize..].to_vec())
    }

    pub fn slice_to(&self, end: u64) -> Self {
        Self(self.0[..end as usize].to_vec())
    }

    pub fn get(&self, index: u64) -> T {
        self.0[index as usize].clone()
    }

    pub fn to_vec(&self) -> Vec<T> {
        self.0.clone()
    }
}

#[derive(
    BorshSerialize, BorshDeserialize, SerialDecodable, SerialEncodable, Clone, Debug, PartialEq, Eq,
)]
pub struct NetMsg {
    id: u64,
    recipient_id: Option<NodeId>,
    method: NetMsgMethod,
    payload: Vec<u8>,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum NetMsgMethod {
    LogResponse = 0,
    LogRequest = 1,
    VoteResponse = 2,
    VoteRequest = 3,
}

impl Encodable for NetMsgMethod {
    fn encode<S: io::Write>(&self, s: S) -> darkfi::Result<usize> {
        let len: usize = match self {
            Self::LogResponse => 0,
            Self::LogRequest => 1,
            Self::VoteResponse => 2,
            Self::VoteRequest => 3,
        };
        (len as u8).encode(s)
    }
}

impl Decodable for NetMsgMethod {
    fn decode<D: io::Read>(d: D) -> darkfi::Result<Self> {
        let com: u8 = Decodable::decode(d)?;
        Ok(match com {
            0 => Self::LogResponse,
            1 => Self::LogRequest,
            2 => Self::VoteResponse,
            _ => Self::VoteRequest,
        })
    }
}

pub fn try_from_slice_unchecked<T: BorshDeserialize>(data: &[u8]) -> Result<T, io::Error> {
    let mut data_mut = data;
    let result = T::deserialize(&mut data_mut)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {}
}
