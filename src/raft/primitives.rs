/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use std::{collections::HashMap, io};

use darkfi_serial::{Decodable, Encodable, SerialDecodable, SerialEncodable};

use crate::{Error, Result};

pub type Channel<T> = (smol::channel::Sender<T>, smol::channel::Receiver<T>);
pub type Sender = (smol::channel::Sender<NetMsg>, smol::channel::Receiver<NetMsg>);

#[derive(PartialEq, Eq, Debug, Clone)]
pub enum Role {
    Follower,
    Candidate,
    Leader,
}

#[derive(SerialDecodable, SerialEncodable, Clone, Debug)]
pub struct SyncRequest {
    pub id: u64,
    pub logs_len: u64,
    pub last_term: u64,
}

#[derive(SerialDecodable, SerialEncodable, Clone, Debug)]
pub struct SyncResponse {
    pub id: u64,
    pub logs: Logs,
    pub commit_length: u64,
    pub leader_id: NodeId,
    pub wipe: bool,
}

#[derive(SerialDecodable, SerialEncodable, Clone, Debug)]
pub struct VoteRequest {
    pub node_id: NodeId,
    pub current_term: u64,
    pub log_length: u64,
    pub last_term: u64,
}

#[derive(SerialDecodable, SerialEncodable, Clone, Debug)]
pub struct VoteResponse {
    pub node_id: NodeId,
    pub current_term: u64,
    pub ok: bool,
}

#[derive(SerialDecodable, SerialEncodable, Clone, Debug)]
pub struct LogRequest {
    pub leader_id: NodeId,
    pub current_term: u64,
    pub prefix_len: u64,
    pub prefix_term: u64,
    pub commit_length: u64,
    pub suffix: Logs,
}

#[derive(SerialDecodable, SerialEncodable, Clone, Debug)]
pub struct LogResponse {
    pub node_id: NodeId,
    pub current_term: u64,
    pub ack: u64,
    pub ok: bool,
}

#[derive(SerialDecodable, SerialEncodable, Clone, Debug)]
pub struct NodeIdMsg {
    pub id: NodeId,
}

impl VoteResponse {
    pub fn set_ok(&mut self, ok: bool) {
        self.ok = ok;
    }
}

#[derive(SerialDecodable, SerialEncodable, Clone, Debug)]
pub struct BroadcastMsgRequest(pub Vec<u8>);

#[derive(Clone, Debug, SerialDecodable, SerialEncodable)]
pub struct Log {
    pub term: u64,
    pub msg: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, SerialDecodable, SerialEncodable)]
pub struct NodeId(pub String);

#[derive(Clone, Debug, SerialDecodable, SerialEncodable)]
pub struct Logs(pub Vec<Log>);

impl Logs {
    pub fn len(&self) -> u64 {
        self.0.len() as u64
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
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
    pub id: u64,
    pub recipient_id: Option<NodeId>,
    pub method: NetMsgMethod,
    pub payload: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum NetMsgMethod {
    LogResponse = 0,
    LogRequest = 1,
    VoteResponse = 2,
    VoteRequest = 3,
    BroadcastRequest = 4,
    NodeIdMsg = 5,
}

impl Encodable for NetMsgMethod {
    fn encode<S: io::Write>(&self, s: S) -> core::result::Result<usize, io::Error> {
        let len: usize = match self {
            Self::LogResponse => 0,
            Self::LogRequest => 1,
            Self::VoteResponse => 2,
            Self::VoteRequest => 3,
            Self::BroadcastRequest => 4,
            Self::NodeIdMsg => 5,
        };
        (len as u8).encode(s)
    }
}

impl Decodable for NetMsgMethod {
    fn decode<D: io::Read>(d: D) -> core::result::Result<Self, io::Error> {
        let com: u8 = Decodable::decode(d)?;
        Ok(match com {
            0 => Self::LogResponse,
            1 => Self::LogRequest,
            2 => Self::VoteResponse,
            3 => Self::VoteRequest,
            4 => Self::BroadcastRequest,
            _ => Self::NodeIdMsg,
        })
    }
}
