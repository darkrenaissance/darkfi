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
use darkfi::{
    geode::{read_until_filled, MAX_CHUNK_SIZE},
    impl_p2p_message,
    net::{
        metering::{DEFAULT_METERING_CONFIGURATION, MeteringConfiguration},
        ChannelPtr, Message, MessageSubscription, P2pPtr, ProtocolBase, ProtocolBasePtr,
        ProtocolJobsManager, ProtocolJobsManagerPtr,
    },
    Error, Result,
};
use darkfi_serial::{SerialDecodable, SerialEncodable};
use log::{debug, error, info};
use smol::{fs::File, Executor};

use super::Fud;
use crate::dht::{DhtHandler, DhtNode};

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
    pub nodes: Vec<DhtNode>,
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
pub struct FudFileNotFound;
impl_p2p_message!(FudFileNotFound, "FudFileNotFound", 0, 0, DEFAULT_METERING_CONFIGURATION);

/// Message representing a chunk reply when a chunk is not found
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct FudChunkNotFound;
impl_p2p_message!(FudChunkNotFound, "FudChunkNotFound", 0, 0, DEFAULT_METERING_CONFIGURATION);

/// Message representing a seeders reply when seeders are not found
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct FudSeedersNotFound;
impl_p2p_message!(FudSeedersNotFound, "FudSeedersNotFound", 0, 0, DEFAULT_METERING_CONFIGURATION);

/// Message representing a ping request on the network
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct FudPingRequest {}
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
impl_p2p_message!(FudFindSeedersRequest, "FudFindSeedersRequest", 0, 0, DEFAULT_METERING_CONFIGURATION);

/// Message representing a find seeders reply on the network
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct FudFindSeedersReply {
    pub nodes: Vec<DhtNode>,
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

            let reply = FudPingReply { node: self.fud.dht.node.clone() };
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

            // Chunk
            {
                let chunk_res = self.fud.geode.get_chunk(&request.key).await;

                // TODO: Run geode GC

                if let Ok(chunk_path) = chunk_res {
                    let mut buf = vec![0u8; MAX_CHUNK_SIZE];
                    let mut chunk_fd = File::open(&chunk_path).await.unwrap();
                    let bytes_read = read_until_filled(&mut chunk_fd, &mut buf).await.unwrap();
                    let chunk_slice = &buf[..bytes_read];
                    let reply = FudChunkReply { chunk: chunk_slice.to_vec() };
                    let _ = self.channel.send(&reply).await;
                    continue;
                }
            }

            // File
            {
                let file_res = self.fud.geode.get(&request.key).await;

                // TODO: Run geode GC

                if let Ok(chunked_file) = file_res {
                    let reply = FudFileReply {
                        chunk_hashes: chunked_file.iter().map(|(chunk, _)| *chunk).collect(),
                    };
                    let _ = self.channel.send(&reply).await;
                    continue;
                }
            }

            // Peers
            {
                let router = self.fud.seeders_router.read().await;
                let peers = router.get(&request.key);

                if let Some(nodes) = peers {
                    let reply = FudFindNodesReply { nodes: nodes.clone().into_iter().collect() };
                    let _ = self.channel.send(&reply).await;
                    continue;
                }
            }

            // Nodes
            let reply = FudFindNodesReply {
                nodes: self.fud.dht().find_neighbors(&request.key, self.fud.dht().k).await,
            };
            let _ = self.channel.send(&reply).await;
        }
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
            info!(target: "fud::ProtocolFud::handle_fud_find_nodes_request()", "Received FIND NODES for {}", &request.key);

            let node = self.fud.dht().get_node_from_channel(self.channel.info.id).await;
            if let Some(node) = node {
                self.fud.update_node(&node).await;
            }

            let reply = FudFindNodesReply {
                nodes: self.fud.dht().find_neighbors(&request.key, self.fud.dht().k).await,
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
            info!(target: "fud::ProtocolFud::handle_fud_find_seeders_request()", "Received FIND SEEDERS for {}", &request.key);

            let node = self.fud.dht().get_node_from_channel(self.channel.info.id).await;
            if let Some(node) = node {
                self.fud.update_node(&node).await;
            }

            let router = self.fud.seeders_router.read().await;
            let peers = router.get(&request.key);

            match peers {
                Some(nodes) => {
                    let _ = self
                        .channel
                        .send(&FudFindSeedersReply { nodes: nodes.iter().cloned().collect() })
                        .await;
                }
                None => {
                    let _ = self.channel.send(&FudSeedersNotFound {}).await;
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
            info!(target: "fud::ProtocolFud::handle_fud_announce()", "Received ANNOUNCE for {}", &request.key);

            let node = self.fud.dht().get_node_from_channel(self.channel.info.id).await;
            if let Some(node) = node {
                self.fud.update_node(&node).await;
            }

            self.fud
                .add_to_router(self.fud.seeders_router.clone(), &request.key, request.nodes.clone())
                .await;
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
