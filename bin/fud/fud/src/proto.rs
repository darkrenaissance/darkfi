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

use async_trait::async_trait;
use log::{debug, error, info};
use smol::Executor;
use std::sync::Arc;

use darkfi::{
    dht::{DhtHandler, DhtNode, DhtRouterItem},
    geode::hash_to_string,
    impl_p2p_message,
    net::{
        metering::{MeteringConfiguration, DEFAULT_METERING_CONFIGURATION},
        ChannelPtr, Message, MessageSubscription, P2pPtr, ProtocolBase, ProtocolBasePtr,
        ProtocolJobsManager, ProtocolJobsManagerPtr,
    },
    Error, Result,
};
use darkfi_serial::{SerialDecodable, SerialEncodable};

use super::Fud;

/// Message representing a file reply from the network
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct FudFileReply {
    pub chunk_hashes: Vec<blake3::Hash>,
}
impl_p2p_message!(FudFileReply, "FudFileReply", 0, 0, DEFAULT_METERING_CONFIGURATION);

/// Message representing a node announcing a key on the network
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct FudAnnounce {
    pub key: blake3::Hash,
    pub seeders: Vec<DhtRouterItem>,
}
impl_p2p_message!(FudAnnounce, "FudAnnounce", 0, 0, DEFAULT_METERING_CONFIGURATION);

/// Message representing a chunk reply from the network
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct FudChunkReply {
    // TODO: This should be a chunk-sized array, but then we need padding?
    pub chunk: Vec<u8>,
}
impl_p2p_message!(FudChunkReply, "FudChunkReply", 0, 0, DEFAULT_METERING_CONFIGURATION);

/// Message representing a chunk reply when a file is not found
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct FudNotFound;
impl_p2p_message!(FudNotFound, "FudNotFound", 0, 0, DEFAULT_METERING_CONFIGURATION);

/// Message representing a ping request on the network
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct FudPingRequest;
impl_p2p_message!(FudPingRequest, "FudPingRequest", 0, 0, DEFAULT_METERING_CONFIGURATION);

/// Message representing a ping reply on the network
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct FudPingReply {
    pub node: DhtNode,
}
impl_p2p_message!(FudPingReply, "FudPingReply", 0, 0, DEFAULT_METERING_CONFIGURATION);

/// Message representing a find file/chunk request from the network
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct FudFindRequest {
    pub info: Option<blake3::Hash>,
    pub key: blake3::Hash,
}
impl_p2p_message!(FudFindRequest, "FudFindRequest", 0, 0, DEFAULT_METERING_CONFIGURATION);

/// Message representing a find nodes request on the network
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct FudFindNodesRequest {
    pub key: blake3::Hash,
}
impl_p2p_message!(FudFindNodesRequest, "FudFindNodesRequest", 0, 0, DEFAULT_METERING_CONFIGURATION);

/// Message representing a find nodes reply on the network
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct FudFindNodesReply {
    pub nodes: Vec<DhtNode>,
}
impl_p2p_message!(FudFindNodesReply, "FudFindNodesReply", 0, 0, DEFAULT_METERING_CONFIGURATION);

/// Message representing a find seeders request on the network
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct FudFindSeedersRequest {
    pub key: blake3::Hash,
}
impl_p2p_message!(
    FudFindSeedersRequest,
    "FudFindSeedersRequest",
    0,
    0,
    DEFAULT_METERING_CONFIGURATION
);

/// Message representing a find seeders reply on the network
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct FudFindSeedersReply {
    pub seeders: Vec<DhtRouterItem>,
}
impl_p2p_message!(FudFindSeedersReply, "FudFindSeedersReply", 0, 0, DEFAULT_METERING_CONFIGURATION);

/// P2P protocol implementation for fud.
pub struct ProtocolFud {
    channel: ChannelPtr,
    ping_request_sub: MessageSubscription<FudPingRequest>,
    find_request_sub: MessageSubscription<FudFindRequest>,
    find_nodes_request_sub: MessageSubscription<FudFindNodesRequest>,
    find_seeders_request_sub: MessageSubscription<FudFindSeedersRequest>,
    announce_sub: MessageSubscription<FudAnnounce>,
    fud: Arc<Fud>,
    jobsman: ProtocolJobsManagerPtr,
}

impl ProtocolFud {
    pub async fn init(fud: Arc<Fud>, channel: ChannelPtr, _: P2pPtr) -> Result<ProtocolBasePtr> {
        debug!(
            target: "fud::proto::ProtocolFud::init()",
            "Adding ProtocolFud to the protocol registry"
        );

        let msg_subsystem = channel.message_subsystem();
        msg_subsystem.add_dispatch::<FudPingRequest>().await;
        msg_subsystem.add_dispatch::<FudFindRequest>().await;
        msg_subsystem.add_dispatch::<FudFindNodesRequest>().await;
        msg_subsystem.add_dispatch::<FudFindSeedersRequest>().await;
        msg_subsystem.add_dispatch::<FudAnnounce>().await;

        let ping_request_sub = channel.subscribe_msg::<FudPingRequest>().await?;
        let find_request_sub = channel.subscribe_msg::<FudFindRequest>().await?;
        let find_nodes_request_sub = channel.subscribe_msg::<FudFindNodesRequest>().await?;
        let find_seeders_request_sub = channel.subscribe_msg::<FudFindSeedersRequest>().await?;
        let announce_sub = channel.subscribe_msg::<FudAnnounce>().await?;

        Ok(Arc::new(Self {
            channel: channel.clone(),
            ping_request_sub,
            find_request_sub,
            find_nodes_request_sub,
            find_seeders_request_sub,
            announce_sub,
            fud,
            jobsman: ProtocolJobsManager::new("ProtocolFud", channel.clone()),
        }))
    }

    async fn handle_fud_ping_request(self: Arc<Self>) -> Result<()> {
        debug!(target: "fud::ProtocolFud::handle_fud_ping_request()", "START");

        loop {
            let _ = match self.ping_request_sub.receive().await {
                Ok(v) => v,
                Err(Error::ChannelStopped) => continue,
                Err(e) => {
                    error!("{}", e);
                    continue
                }
            };
            info!(target: "fud::ProtocolFud::handle_fud_ping_request()", "Received PING");

            let reply = FudPingReply { node: self.fud.dht.node().await };
            match self.channel.send(&reply).await {
                Ok(()) => continue,
                Err(_e) => continue,
            }
        }
    }

    async fn handle_fud_find_request(self: Arc<Self>) -> Result<()> {
        debug!(target: "fud::ProtocolFud::handle_fud_find_request()", "START");

        loop {
            let request = match self.find_request_sub.receive().await {
                Ok(v) => v,
                Err(Error::ChannelStopped) => continue,
                Err(e) => {
                    error!("{}", e);
                    continue
                }
            };
            info!(target: "fud::ProtocolFud::handle_fud_find_request()", "Received FIND");

            let node = self.fud.dht().get_node_from_channel(self.channel.info.id).await;
            if let Some(node) = node {
                self.fud.update_node(&node).await;
            }

            if self.handle_fud_chunk_request(&request).await {
                continue;
            }

            if self.handle_fud_file_request(&request).await {
                continue;
            }

            // Request did not match anything we have
            let reply = FudNotFound {};
            info!(target: "fud::ProtocolFud::handle_fud_find_request()", "We do not have {}", hash_to_string(&request.key));
            let _ = self.channel.send(&reply).await;
        }
    }

    /// If the FudFindRequest matches a chunk we have, handle it.
    /// Returns true if the chunk was found.
    async fn handle_fud_chunk_request(&self, request: &FudFindRequest) -> bool {
        let file_hash = request.info;
        if file_hash.is_none() {
            return false;
        }
        let file_hash = file_hash.unwrap();

        let file_path = self.fud.hash_to_path(&file_hash).ok().flatten();
        if file_path.is_none() {
            return false;
        }
        let file_path = file_path.unwrap();

        let chunk = self.fud.geode.get_chunk(&request.key, &file_hash, &file_path).await;
        if let Ok(chunk) = chunk {
            // TODO: Run geode GC
            let reply = FudChunkReply { chunk };
            info!(target: "fud::ProtocolFud::handle_fud_find_request()", "Sending chunk");
            let _ = self.channel.send(&reply).await;
            return true;
        }
        false
    }

    /// If the FudFindRequest matches a file we have, handle it
    /// Returns true if the file was found.
    async fn handle_fud_file_request(&self, request: &FudFindRequest) -> bool {
        let file_path = self.fud.hash_to_path(&request.key).ok().flatten();
        if file_path.is_none() {
            return false;
        }
        let file_path = file_path.unwrap();

        let chunked_file = self.fud.geode.get(&request.key, &file_path).await.ok();
        if chunked_file.is_none() {
            return false;
        }
        let chunked_file = chunked_file.unwrap();

        // If the file has a single chunk, just reply with the chunk
        if chunked_file.len() == 1 {
            let chunk = self
                .fud
                .geode
                .get_chunk(&chunked_file.iter().next().unwrap().0, &request.key, &file_path)
                .await;
            if let Ok(chunk) = chunk {
                // TODO: Run geode GC
                let reply = FudChunkReply { chunk };
                info!(target: "fud::ProtocolFud::handle_fud_find_request()", "Sending chunk (file has a single chunk)");
                let _ = self.channel.send(&reply).await;
                return true;
            }
            return false;
        }

        // Otherwise reply with the file metadata
        let reply =
            FudFileReply { chunk_hashes: chunked_file.iter().map(|(chunk, _)| *chunk).collect() };
        info!(target: "fud::ProtocolFud::handle_fud_find_request()", "Sending file");
        let _ = self.channel.send(&reply).await;
        true
    }

    async fn handle_fud_find_nodes_request(self: Arc<Self>) -> Result<()> {
        debug!(target: "fud::ProtocolFud::handle_fud_find_nodes_request()", "START");

        loop {
            let request = match self.find_nodes_request_sub.receive().await {
                Ok(v) => v,
                Err(Error::ChannelStopped) => continue,
                Err(e) => {
                    error!("{}", e);
                    continue
                }
            };
            info!(target: "fud::ProtocolFud::handle_fud_find_nodes_request()", "Received FIND NODES for {}", hash_to_string(&request.key));

            let node = self.fud.dht().get_node_from_channel(self.channel.info.id).await;
            if let Some(node) = node {
                self.fud.update_node(&node).await;
            }

            let reply = FudFindNodesReply {
                nodes: self.fud.dht().find_neighbors(&request.key, self.fud.dht().settings.k).await,
            };
            match self.channel.send(&reply).await {
                Ok(()) => continue,
                Err(_e) => continue,
            }
        }
    }

    async fn handle_fud_find_seeders_request(self: Arc<Self>) -> Result<()> {
        debug!(target: "fud::ProtocolFud::handle_fud_find_seeders_request()", "START");

        loop {
            let request = match self.find_seeders_request_sub.receive().await {
                Ok(v) => v,
                Err(Error::ChannelStopped) => continue,
                Err(e) => {
                    error!("{}", e);
                    continue
                }
            };
            info!(target: "fud::ProtocolFud::handle_fud_find_seeders_request()", "Received FIND SEEDERS for {}", hash_to_string(&request.key));

            let node = self.fud.dht().get_node_from_channel(self.channel.info.id).await;
            if let Some(node) = node {
                self.fud.update_node(&node).await;
            }

            let router = self.fud.seeders_router.read().await;
            let peers = router.get(&request.key);

            match peers {
                Some(seeders) => {
                    let _ = self
                        .channel
                        .send(&FudFindSeedersReply { seeders: seeders.iter().cloned().collect() })
                        .await;
                }
                None => {
                    let _ = self.channel.send(&FudFindSeedersReply { seeders: vec![] }).await;
                }
            };
        }
    }

    async fn handle_fud_announce(self: Arc<Self>) -> Result<()> {
        debug!(target: "fud::ProtocolFud::handle_fud_announce()", "START");

        loop {
            let request = match self.announce_sub.receive().await {
                Ok(v) => v,
                Err(Error::ChannelStopped) => continue,
                Err(e) => {
                    error!("{}", e);
                    continue
                }
            };
            info!(target: "fud::ProtocolFud::handle_fud_announce()", "Received ANNOUNCE for {}", hash_to_string(&request.key));

            let node = self.fud.dht().get_node_from_channel(self.channel.info.id).await;
            if let Some(node) = node {
                self.fud.update_node(&node).await;
            }

            let mut seeders = vec![];

            for seeder in request.seeders.clone() {
                if seeder.node.addresses.is_empty() {
                    continue
                }
                // TODO: Verify each address
                seeders.push(seeder);
            }

            self.fud.add_to_router(self.fud.seeders_router.clone(), &request.key, seeders).await;
        }
    }
}

#[async_trait]
impl ProtocolBase for ProtocolFud {
    async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        debug!(target: "fud::ProtocolFud::start()", "START");
        self.jobsman.clone().start(executor.clone());
        self.jobsman.clone().spawn(self.clone().handle_fud_ping_request(), executor.clone()).await;
        self.jobsman.clone().spawn(self.clone().handle_fud_find_request(), executor.clone()).await;
        self.jobsman
            .clone()
            .spawn(self.clone().handle_fud_find_nodes_request(), executor.clone())
            .await;
        self.jobsman
            .clone()
            .spawn(self.clone().handle_fud_find_seeders_request(), executor.clone())
            .await;
        self.jobsman.clone().spawn(self.clone().handle_fud_announce(), executor.clone()).await;
        debug!(target: "fud::ProtocolFud::start()", "END");
        Ok(())
    }

    fn name(&self) -> &'static str {
        "ProtocolFud"
    }
}
