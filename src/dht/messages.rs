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

use std::collections::{HashMap, HashSet};

use darkfi_serial::{serialize, SerialDecodable, SerialEncodable};
use rand::{rngs::OsRng, Rng};

use crate::{impl_p2p_message, net::Message};

/// This struct represents a DHT key request
#[derive(Debug, Clone, SerialDecodable, SerialEncodable)]
pub struct KeyRequest {
    /// Request id    
    pub id: blake3::Hash,
    /// Daemon id requesting the key
    pub from: blake3::Hash,
    /// Daemon id holding the key
    pub to: blake3::Hash,
    /// Key entry
    pub key: blake3::Hash,
}

impl KeyRequest {
    pub fn new(from: blake3::Hash, to: blake3::Hash, key: blake3::Hash) -> Self {
        // Generate a random id
        let n: u16 = OsRng.gen();
        let id = blake3::hash(&serialize(&n));
        Self { id, from, to, key }
    }
}
impl_p2p_message!(KeyRequest, "keyrequest");

/// This struct represents a DHT key request response
#[derive(Debug, Clone, SerialDecodable, SerialEncodable)]
pub struct KeyResponse {
    /// Response id
    pub id: blake3::Hash,
    /// Daemon id holding the key
    pub from: blake3::Hash,
    /// Daemon id holding the key
    pub to: blake3::Hash,
    /// Key entry
    pub key: blake3::Hash,
    /// Key value
    pub value: Vec<u8>,
}

impl KeyResponse {
    pub fn new(from: blake3::Hash, to: blake3::Hash, key: blake3::Hash, value: Vec<u8>) -> Self {
        // Generate a random id
        let n: u16 = OsRng.gen();
        let id = blake3::hash(&serialize(&n));
        Self { id, from, to, key, value }
    }
}
impl_p2p_message!(KeyResponse, "keyresponse");

/// This struct represents a lookup map request
#[derive(Debug, Clone, SerialDecodable, SerialEncodable)]
pub struct LookupRequest {
    /// Request id    
    pub id: blake3::Hash,
    /// Daemon id executing the request
    pub daemon: blake3::Hash,
    /// Key entry
    pub key: blake3::Hash,
    /// Request type
    pub req_type: u8, // 0 for insert, 1 for remove
}

impl LookupRequest {
    pub fn new(daemon: blake3::Hash, key: blake3::Hash, req_type: u8) -> Self {
        // Generate a random id
        let n: u16 = OsRng.gen();
        let id = blake3::hash(&serialize(&n));
        Self { id, daemon, key, req_type }
    }
}
impl_p2p_message!(LookupRequest, "lookuprequest");

/// Auxiliary structure used for lookup map syncing.
#[derive(Debug, SerialEncodable, SerialDecodable)]
pub struct LookupMapRequest {
    /// Request id
    pub id: blake3::Hash,
    /// Daemon id executing the request
    pub daemon: blake3::Hash,
}

impl LookupMapRequest {
    pub fn new(daemon: blake3::Hash) -> Self {
        // Generate a random id
        let n: u16 = OsRng.gen();
        let id = blake3::hash(&serialize(&n));
        Self { id, daemon }
    }
}
impl_p2p_message!(LookupMapRequest, "lookupmaprequest");

/// Auxiliary structure used for consensus syncing.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct LookupMapResponse {
    /// Request id
    pub id: blake3::Hash,
    /// Daemon lookup map, containing nodes that holds each key
    pub lookup: HashMap<blake3::Hash, HashSet<blake3::Hash>>,
}

impl LookupMapResponse {
    pub fn new(lookup: HashMap<blake3::Hash, HashSet<blake3::Hash>>) -> Self {
        // Generate a random id
        let n: u16 = OsRng.gen();
        let id = blake3::hash(&serialize(&n));
        Self { id, lookup }
    }
}
impl_p2p_message!(LookupMapResponse, "lookupmapresponse");
