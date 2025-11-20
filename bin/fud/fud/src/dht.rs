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

use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use rand::{rngs::OsRng, Rng};
use tinyjson::JsonValue;
use tracing::{debug, warn};
use url::Url;

use darkfi::{
    dht::{
        event::DhtEvent, impl_dht_node_defaults, Dht, DhtHandler, DhtLookupReply, DhtNode,
        HostCacheItem,
    },
    geode::hash_to_string,
    net::{
        session::{SESSION_DIRECT, SESSION_INBOUND, SESSION_MANUAL, SESSION_OUTBOUND},
        ChannelPtr,
    },
    rpc::util::json_map,
    system::timeout::timeout,
    util::time::Timestamp,
    Error, Result,
};
use darkfi_sdk::crypto::schnorr::{SchnorrPublic, Signature};
use darkfi_serial::{serialize_async, SerialDecodable, SerialEncodable};

use crate::{
    pow::VerifiableNodeData,
    proto::{
        FudAnnounce, FudNodesReply, FudNodesRequest, FudPingReply, FudPingRequest, FudSeedersReply,
        FudSeedersRequest,
    },
    util::receive_resource_msg,
    Fud,
};

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct FudNode {
    pub data: VerifiableNodeData,
    pub addresses: Vec<Url>,
}
impl_dht_node_defaults!(FudNode);

impl DhtNode for FudNode {
    fn id(&self) -> blake3::Hash {
        self.data.id()
    }
    fn addresses(&self) -> Vec<Url> {
        self.addresses.clone()
    }
}

impl From<FudNode> for JsonValue {
    fn from(node: FudNode) -> JsonValue {
        json_map([
            ("id", JsonValue::String(hash_to_string(&node.id()))),
            (
                "addresses",
                JsonValue::Array(
                    node.addresses.iter().map(|addr| JsonValue::String(addr.to_string())).collect(),
                ),
            ),
        ])
    }
}

/// The values of the DHT are `Vec<FudSeeder>`, mapping resource hashes to lists of [`FudSeeder`]s
#[derive(Debug, Clone, SerialEncodable, SerialDecodable, Eq)]
pub struct FudSeeder {
    /// Resource that this seeder provides
    pub key: blake3::Hash,
    /// Seeder's node data
    pub node: FudNode,
    /// Seeder's signature of (key || node)
    pub sig: Signature,
    /// When this [`FudSeeder`] was added to our hash table.
    /// This is not sent to other nodes.
    #[skip_serialize]
    pub timestamp: u64,
}

impl PartialEq for FudSeeder {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key && self.node.id() == other.node.id()
    }
}

impl From<FudSeeder> for JsonValue {
    fn from(seeder: FudSeeder) -> JsonValue {
        json_map([
            ("key", JsonValue::String(hash_to_string(&seeder.key))),
            ("node", seeder.node.into()),
        ])
    }
}

impl FudSeeder {
    pub async fn verify_signature(&self) -> bool {
        self.node.data.public_key.verify(
            &[self.key.as_bytes().to_vec(), serialize_async(&self.node).await].concat(),
            &self.sig,
        )
    }
}

/// [`DhtHandler`] implementation for fud
#[async_trait]
impl DhtHandler for Fud {
    type Value = Vec<FudSeeder>;
    type Node = FudNode;

    fn dht(&self) -> Arc<Dht<Self>> {
        self.dht.clone()
    }

    async fn node(&self) -> FudNode {
        FudNode {
            data: self.node_data.read().await.clone(),
            addresses: self
                .p2p
                .clone()
                .hosts()
                .external_addrs()
                .await
                .iter()
                .filter(|addr| !addr.to_string().contains("[::]"))
                .cloned()
                .collect(),
        }
    }

    async fn ping(&self, channel: ChannelPtr) -> Result<FudNode> {
        debug!(target: "fud::DhtHandler::ping()", "Sending ping to {}", channel.display_address());

        // Setup `FudPingReply` subscriber
        let msg_subscriber = channel.subscribe_msg::<FudPingReply>().await.unwrap();

        // Send `FudPingRequest`
        let mut rng = OsRng;
        let request = FudPingRequest { random: rng.gen() };
        if channel.is_stopped() {
            return Err(Error::ChannelStopped)
        }
        channel.send(&request).await?;

        // Wait for `FudPingReply`
        let reply = msg_subscriber.receive_with_timeout(self.dht.settings.timeout).await;
        msg_subscriber.unsubscribe().await;
        let reply = reply?;
        let node = &reply.node;

        // Verify the signature
        if !node.data.public_key.verify(&request.random.to_be_bytes(), &reply.sig) {
            warn!(target: "fud::DhtHandler::ping()", "Received an invalid signature while pinging {}", channel.display_address());
            self.dht
                .event_publisher
                .notify(DhtEvent::PingReceived {
                    from: channel.clone(),
                    result: Err(Error::InvalidSignature),
                })
                .await;
            self.dht.cleanup_channel(channel.clone()).await;
            channel.ban().await;
            return Err(Error::InvalidSignature)
        }

        // Verify PoW
        if let Err(e) = self.pow.write().await.verify_node(&node.data).await {
            warn!(target: "fud::DhtHandler::ping()", "Received an invalid PoW while pinging {}: {e}", channel.display_address());
            self.dht
                .event_publisher
                .notify(DhtEvent::PingReceived { from: channel.clone(), result: Err(e.clone()) })
                .await;
            self.dht.cleanup_channel(channel.clone()).await;
            channel.ban().await;
            return Err(e)
        }
        self.dht
            .event_publisher
            .notify(DhtEvent::PingReceived { from: channel.clone(), result: Ok(node.id()) })
            .await;

        if channel.session_type_id() & (SESSION_OUTBOUND | SESSION_DIRECT | SESSION_MANUAL) != 0 {
            // Wait for the other node to ping us
            let ping_timeout = Duration::from_secs(10);

            if let Err(e) = timeout(ping_timeout, self.dht.wait_fully_pinged(channel.info.id)).await
            {
                self.dht.cleanup_channel(channel).await;
                return Err(e.into())
            }

            let mut host_cache = self.dht.host_cache.write().await;

            // If we had another node id for this host in our cache, remove
            // the old one from the buckets and seeders
            if let Some(cached) = host_cache.get(channel.address()) {
                if cached.node_id != node.id() {
                    self.dht.remove_node(&cached.node_id).await;

                    for (_, seeders) in self.dht.hash_table.write().await.iter_mut() {
                        seeders.retain(|seeder| seeder.node.id() != cached.node_id);
                    }
                }
            }

            // Update host cache
            host_cache.insert(
                channel.address().clone(),
                HostCacheItem { last_ping: Timestamp::current_time(), node_id: node.id() },
            );

            drop(host_cache);

            // Update our buckets
            if !node.addresses().is_empty() {
                self.dht.update_node(&node.clone(), channel.clone()).await;
            }
        } else if channel.session_type_id() & SESSION_INBOUND != 0 {
            // If it's an inbound connection, verify that we can connect to at
            // least one of the provided external addresses.
            // This may try to create a new outbound channel and it will update
            // our buckets if successful.
            let _ = self.verify_node_tx.send(node.clone()).await;
        }

        // Update the channel cache
        self.dht.add_channel_to_cache(channel.info.id, node).await;

        Ok(node.clone())
    }

    async fn store(
        &self,
        channel: ChannelPtr,
        key: &blake3::Hash,
        value: &Vec<FudSeeder>,
    ) -> Result<()> {
        debug!(target: "fud::DhtHandler::store()", "Announcing {} to {}", hash_to_string(key), channel.display_address());

        channel.send(&FudAnnounce { key: *key, seeders: value.clone() }).await
    }

    async fn find_nodes(&self, channel: ChannelPtr, key: &blake3::Hash) -> Result<Vec<FudNode>> {
        debug!(target: "fud::DhtHandler::find_nodes()", "Fetching nodes close to {} from node {}", hash_to_string(key), channel.display_address());

        let msg_subscriber_nodes = channel.subscribe_msg::<FudNodesReply>().await.unwrap();

        let request = FudNodesRequest { key: *key };
        channel.send(&request).await?;

        let reply =
            receive_resource_msg(&msg_subscriber_nodes, *key, self.dht().settings.timeout).await;

        msg_subscriber_nodes.unsubscribe().await;

        Ok(reply?.nodes.clone())
    }

    async fn find_value(
        &self,
        channel: ChannelPtr,
        key: &blake3::Hash,
    ) -> Result<DhtLookupReply<FudNode, Vec<FudSeeder>>> {
        debug!(target: "fud::DhtHandler::find_value()", "Fetching value {} (or close nodes) from {}", hash_to_string(key), channel.display_address());

        let msg_subscriber = channel.subscribe_msg::<FudSeedersReply>().await.unwrap();

        let request = FudSeedersRequest { key: *key };
        channel.send(&request).await?;

        let recv = receive_resource_msg(&msg_subscriber, *key, self.dht().settings.timeout).await;

        msg_subscriber.unsubscribe().await;

        let rep = recv?;
        Ok(DhtLookupReply::NodesAndValue(rep.nodes.clone(), rep.seeders.clone()))
    }

    async fn add_value(&self, key: &blake3::Hash, value: &Vec<FudSeeder>) {
        let mut seeders = value.clone();

        // Remove seeders with no external addresses
        seeders.retain(|item| !item.node.addresses().is_empty());

        // Set all seeders' timestamp. They are not sent to others nodes so they default to 0.
        let timestamp = Timestamp::current_time().inner();
        for seeder in &mut seeders {
            seeder.timestamp = timestamp;
        }

        debug!(target: "fud::DhtHandler::add_value()", "Inserting {} seeders for resource {}", seeders.len(), hash_to_string(key));

        let mut seeders_write = self.dht.hash_table.write().await;
        let existing_seeders = seeders_write.get_mut(key);

        if let Some(existing_seeders) = existing_seeders {
            existing_seeders.retain(|it| !seeders.contains(it));
            existing_seeders.extend(seeders.clone());
        } else {
            let mut vec = Vec::new();
            vec.extend(seeders.clone());
            seeders_write.insert(*key, vec);
        }
    }

    fn key_to_string(key: &blake3::Hash) -> String {
        hash_to_string(key)
    }
}
