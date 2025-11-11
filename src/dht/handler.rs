/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
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

use std::{
    fmt::Debug,
    marker::{Send, Sync},
    sync::Arc,
};

use async_trait::async_trait;

use super::{Dht, DhtLookupReply, DhtNode};
use crate::{net::ChannelPtr, Result};

/// Trait for application-specific behaviors over a [`Dht`]
#[async_trait]
pub trait DhtHandler: Send + Sync + Sized {
    type Value: Clone + Debug;
    type Node: DhtNode;

    /// The [`Dht`] instance
    fn dht(&self) -> Arc<Dht<Self>>;

    /// Get our own node
    async fn node(&self) -> Self::Node;

    /// Send PING request, which is used to know the node data of a peer
    /// (and most importantly, its ID/key in the DHT keyspace)
    async fn ping(&self, channel: ChannelPtr) -> Result<Self::Node>;

    /// Send STORE request to instruct a peer to store a key-value pair
    async fn store(
        &self,
        channel: ChannelPtr,
        key: &blake3::Hash,
        value: &Self::Value,
    ) -> Result<()>;

    /// Send FIND NODES request to a peer to get nodes close to `key`
    async fn find_nodes(&self, channel: ChannelPtr, key: &blake3::Hash) -> Result<Vec<Self::Node>>;

    /// Send FIND VALUE request to a peer to get a value and/or nodes close to `key`
    async fn find_value(
        &self,
        channel: ChannelPtr,
        key: &blake3::Hash,
    ) -> Result<DhtLookupReply<Self::Node, Self::Value>>;

    /// Add a value to our hash table
    async fn add_value(&self, key: &blake3::Hash, value: &Self::Value);

    /// Defines how keys are printed/logged
    fn key_to_string(key: &blake3::Hash) -> String;
}
