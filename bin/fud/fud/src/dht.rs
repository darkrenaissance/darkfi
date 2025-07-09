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

use std::sync::Arc;

use async_trait::async_trait;
use num_bigint::BigUint;
use rand::{rngs::OsRng, Rng};
use tracing::debug;
use url::Url;

use darkfi::{
    dht::{impl_dht_node_defaults, Dht, DhtHandler, DhtLookupReply, DhtNode},
    geode::hash_to_string,
    net::ChannelPtr,
    util::time::Timestamp,
    Error, Result,
};
use darkfi_sdk::crypto::schnorr::SchnorrPublic;
use darkfi_serial::{SerialDecodable, SerialEncodable};

use crate::{
    pow::VerifiableNodeData,
    proto::{
        FudAnnounce, FudFindNodesReply, FudFindNodesRequest, FudFindSeedersReply,
        FudFindSeedersRequest, FudPingReply, FudPingRequest,
    },
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

/// The values of the DHT are `Vec<FudSeeder>`, mapping resource hashes to lists of [`FudSeeder`]s
#[derive(Debug, Clone, SerialEncodable, SerialDecodable, Eq)]
pub struct FudSeeder {
    /// Resource that this seeder provides
    pub key: blake3::Hash,
    /// Seeder's node data
    pub node: FudNode,
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
        debug!(target: "fud::DhtHandler::ping()", "Sending ping to channel {}", channel.info.id);
        let msg_subsystem = channel.message_subsystem();
        msg_subsystem.add_dispatch::<FudPingReply>().await;
        let msg_subscriber = channel.subscribe_msg::<FudPingReply>().await.unwrap();

        // Send `FudPingRequest`
        let mut rng = OsRng;
        let request = FudPingRequest { random: rng.gen() };
        channel.send(&request).await?;

        // Wait for `FudPingReply`
        let reply = msg_subscriber.receive_with_timeout(self.dht().settings.timeout).await;
        msg_subscriber.unsubscribe().await;
        let reply = reply?;

        // Verify the signature
        if !reply.node.data.public_key.verify(&request.random.to_be_bytes(), &reply.sig) {
            channel.ban().await;
            return Err(Error::InvalidSignature)
        }

        // Verify PoW
        if let Err(e) = self.pow.write().await.verify_node(&reply.node.data).await {
            channel.ban().await;
            return Err(e)
        }

        Ok(reply.node.clone())
    }

    // TODO: Optimize this
    async fn on_new_node(&self, node: &FudNode) -> Result<()> {
        debug!(target: "fud::DhtHandler::on_new_node()", "New node {}", hash_to_string(&node.id()));

        // If this is the first node we know about, then bootstrap and announce our files
        if !self.dht.is_bootstrapped().await {
            let _ = self.init().await;
        }

        // Send keys that are closer to this node than we are
        let self_id = self.node_data.read().await.id();
        let channel = self.dht.get_channel(node, None).await?;
        for (key, seeders) in self.dht.hash_table.read().await.iter() {
            let node_distance = BigUint::from_bytes_be(&self.dht().distance(key, &node.id()));
            let self_distance = BigUint::from_bytes_be(&self.dht().distance(key, &self_id));
            if node_distance <= self_distance {
                let _ = channel.send(&FudAnnounce { key: *key, seeders: seeders.clone() }).await;
            }
        }
        self.dht.cleanup_channel(channel).await;

        Ok(())
    }

    async fn find_nodes(&self, node: &FudNode, key: &blake3::Hash) -> Result<Vec<FudNode>> {
        debug!(target: "fud::DhtHandler::find_nodes()", "Fetching nodes close to {} from node {}", hash_to_string(key), hash_to_string(&node.id()));

        let channel = self.dht.get_channel(node, None).await?;
        let msg_subsystem = channel.message_subsystem();
        msg_subsystem.add_dispatch::<FudFindNodesReply>().await;
        let msg_subscriber_nodes = channel.subscribe_msg::<FudFindNodesReply>().await.unwrap();

        let request = FudFindNodesRequest { key: *key };
        channel.send(&request).await?;

        let reply = msg_subscriber_nodes.receive_with_timeout(self.dht().settings.timeout).await;

        msg_subscriber_nodes.unsubscribe().await;
        self.dht.cleanup_channel(channel).await;

        Ok(reply?.nodes.clone())
    }

    async fn find_value(
        &self,
        node: &FudNode,
        key: &blake3::Hash,
    ) -> Result<DhtLookupReply<FudNode, Vec<FudSeeder>>> {
        debug!(target: "fud::DhtHandler::find_value()", "Fetching value {} from node {}", hash_to_string(key), hash_to_string(&node.id()));

        let channel = self.dht.get_channel(node, None).await?;
        let msg_subsystem = channel.message_subsystem();
        msg_subsystem.add_dispatch::<FudFindSeedersReply>().await;
        let msg_subscriber = channel.subscribe_msg::<FudFindSeedersReply>().await.unwrap();

        let request = FudFindSeedersRequest { key: *key };
        channel.send(&request).await?;

        let recv = msg_subscriber.receive_with_timeout(self.dht().settings.timeout).await;

        msg_subscriber.unsubscribe().await;
        self.dht.cleanup_channel(channel).await;

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
