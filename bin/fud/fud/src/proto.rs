/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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
use smol::Executor;
use std::{path::StripPrefixError, sync::Arc};
use tracing::{debug, error, info, warn};

use darkfi::{
    dht::{event::DhtEvent, DhtHandler},
    geode::hash_to_string,
    impl_p2p_message,
    net::{
        metering::{MeteringConfiguration, DEFAULT_METERING_CONFIGURATION},
        session::SESSION_INBOUND,
        ChannelPtr, Message, MessageSubscription, P2pPtr, ProtocolBase, ProtocolBasePtr,
        ProtocolJobsManager, ProtocolJobsManagerPtr,
    },
    Error, Result,
};
use darkfi_sdk::crypto::schnorr::{SchnorrSecret, Signature};
use darkfi_serial::{SerialDecodable, SerialEncodable};

use crate::{
    dht::{FudNode, FudSeeder},
    Fud,
};

/// Trait for resource-specific messages.
/// Adds a method to get the resource's hash from the message.
pub trait ResourceMessage {
    fn resource_hash(&self) -> blake3::Hash;
}
macro_rules! impl_resource_msg {
    ($msg:ty, $field:ident) => {
        impl ResourceMessage for $msg {
            fn resource_hash(&self) -> blake3::Hash {
                self.$field
            }
        }
    };
    ($msg:ty) => {
        impl_resource_msg!($msg, resource);
    };
}

/// Message representing a file reply from the network
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct FudFileReply {
    pub resource: blake3::Hash,
    pub chunk_hashes: Vec<blake3::Hash>,
}
impl_p2p_message!(FudFileReply, "FudFileReply", 0, 0, DEFAULT_METERING_CONFIGURATION);
impl_resource_msg!(FudFileReply);

/// Message representing a directory reply from the network
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct FudDirectoryReply {
    pub resource: blake3::Hash,
    pub chunk_hashes: Vec<blake3::Hash>,
    pub files: Vec<(String, u64)>, // Vec of (file path, file size)
}
impl_p2p_message!(FudDirectoryReply, "FudDirectoryReply", 0, 0, DEFAULT_METERING_CONFIGURATION);
impl_resource_msg!(FudDirectoryReply);

/// Message representing a node announcing a key on the network
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct FudAnnounce {
    pub key: blake3::Hash,
    pub seeders: Vec<FudSeeder>,
}
impl_p2p_message!(FudAnnounce, "FudAnnounce", 0, 0, DEFAULT_METERING_CONFIGURATION);

/// Message representing a chunk reply from the network
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct FudChunkReply {
    pub resource: blake3::Hash,
    // TODO: This should be a chunk-sized array, but then we need padding?
    pub chunk: Vec<u8>,
}
impl_p2p_message!(FudChunkReply, "FudChunkReply", 0, 0, DEFAULT_METERING_CONFIGURATION);
impl_resource_msg!(FudChunkReply);

/// Message representing a reply when a metadata is not found
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct FudMetadataNotFound {
    pub resource: blake3::Hash,
}
impl_p2p_message!(FudMetadataNotFound, "FudMetadataNotFound", 0, 0, DEFAULT_METERING_CONFIGURATION);
impl_resource_msg!(FudMetadataNotFound);

/// Message representing a reply when a chunk is not found
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct FudChunkNotFound {
    pub resource: blake3::Hash,
    pub chunk: blake3::Hash,
}
impl_p2p_message!(FudChunkNotFound, "FudChunkNotFound", 0, 0, DEFAULT_METERING_CONFIGURATION);
impl_resource_msg!(FudChunkNotFound);

/// Message representing a ping request on the network
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct FudPingRequest {
    pub random: u64,
}
impl_p2p_message!(FudPingRequest, "FudPingRequest", 0, 0, DEFAULT_METERING_CONFIGURATION);

/// Message representing a ping reply on the network
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct FudPingReply {
    pub node: FudNode,
    pub random: u64,
    /// Signature of the random u64 from the ping request
    pub sig: Signature,
}
impl_p2p_message!(FudPingReply, "FudPingReply", 0, 0, DEFAULT_METERING_CONFIGURATION);

/// Message representing a find file/directory request from the network
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct FudMetadataRequest {
    pub resource: blake3::Hash,
}
impl_p2p_message!(FudMetadataRequest, "FudMetadataRequest", 0, 0, DEFAULT_METERING_CONFIGURATION);
impl_resource_msg!(FudMetadataRequest);

/// Message representing a find chunk request from the network
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct FudChunkRequest {
    pub resource: blake3::Hash,
    pub chunk: blake3::Hash,
}
impl_p2p_message!(FudChunkRequest, "FudChunkRequest", 0, 0, DEFAULT_METERING_CONFIGURATION);
impl_resource_msg!(FudChunkRequest);

/// Message representing a find nodes request on the network
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct FudNodesRequest {
    pub key: blake3::Hash,
}
impl_p2p_message!(FudNodesRequest, "FudNodesRequest", 0, 0, DEFAULT_METERING_CONFIGURATION);

/// Message representing a find nodes reply on the network
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct FudNodesReply {
    pub key: blake3::Hash,
    pub nodes: Vec<FudNode>,
}
impl_p2p_message!(FudNodesReply, "FudNodesReply", 0, 0, DEFAULT_METERING_CONFIGURATION);
impl_resource_msg!(FudNodesReply, key);

/// Message representing a find seeders request on the network
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct FudSeedersRequest {
    pub key: blake3::Hash,
}
impl_p2p_message!(FudSeedersRequest, "FudSeedersRequest", 0, 0, DEFAULT_METERING_CONFIGURATION);

/// Message representing a find seeders reply on the network
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct FudSeedersReply {
    pub key: blake3::Hash,
    pub seeders: Vec<FudSeeder>,
    pub nodes: Vec<FudNode>,
}
impl_p2p_message!(FudSeedersReply, "FudSeedersReply", 0, 0, DEFAULT_METERING_CONFIGURATION);
impl_resource_msg!(FudSeedersReply, key);

/// P2P protocol implementation for fud.
pub struct ProtocolFud {
    channel: ChannelPtr,
    ping_request_sub: MessageSubscription<FudPingRequest>,
    find_metadata_request_sub: MessageSubscription<FudMetadataRequest>,
    find_chunk_request_sub: MessageSubscription<FudChunkRequest>,
    find_nodes_request_sub: MessageSubscription<FudNodesRequest>,
    find_seeders_request_sub: MessageSubscription<FudSeedersRequest>,
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
        msg_subsystem.add_dispatch::<FudPingReply>().await;
        msg_subsystem.add_dispatch::<FudMetadataRequest>().await;
        msg_subsystem.add_dispatch::<FudChunkRequest>().await;
        msg_subsystem.add_dispatch::<FudChunkReply>().await;
        msg_subsystem.add_dispatch::<FudChunkNotFound>().await;
        msg_subsystem.add_dispatch::<FudFileReply>().await;
        msg_subsystem.add_dispatch::<FudDirectoryReply>().await;
        msg_subsystem.add_dispatch::<FudMetadataNotFound>().await;
        msg_subsystem.add_dispatch::<FudNodesRequest>().await;
        msg_subsystem.add_dispatch::<FudNodesReply>().await;
        msg_subsystem.add_dispatch::<FudSeedersRequest>().await;
        msg_subsystem.add_dispatch::<FudSeedersReply>().await;
        msg_subsystem.add_dispatch::<FudAnnounce>().await;

        let ping_request_sub = channel.subscribe_msg::<FudPingRequest>().await?;
        let find_metadata_request_sub = channel.subscribe_msg::<FudMetadataRequest>().await?;
        let find_chunk_request_sub = channel.subscribe_msg::<FudChunkRequest>().await?;
        let find_nodes_request_sub = channel.subscribe_msg::<FudNodesRequest>().await?;
        let find_seeders_request_sub = channel.subscribe_msg::<FudSeedersRequest>().await?;
        let announce_sub = channel.subscribe_msg::<FudAnnounce>().await?;

        Ok(Arc::new(Self {
            channel: channel.clone(),
            ping_request_sub,
            find_metadata_request_sub,
            find_chunk_request_sub,
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
            let ping_req = match self.ping_request_sub.receive().await {
                Ok(v) => v,
                Err(Error::ChannelStopped) => continue,
                Err(_) => continue,
            };
            info!(target: "fud::ProtocolFud::handle_fud_ping_request()", "Received PING REQUEST from {}", self.channel.display_address());
            self.fud.dht.update_channel(self.channel.info.id).await;

            let self_node = self.fud.node().await;
            if self_node.is_err() {
                self.channel.stop().await;
                continue
            }
            let state = self.fud.state.read().await;
            if state.is_none() {
                self.channel.stop().await;
                continue
            }

            let reply = FudPingReply {
                node: self_node.unwrap(),
                random: ping_req.random,
                sig: state.clone().unwrap().secret_key.sign(&ping_req.random.to_be_bytes()),
            };
            drop(state);

            if let Err(e) = self.channel.send(&reply).await {
                self.fud
                    .dht
                    .event_publisher
                    .notify(DhtEvent::PingSent { to: self.channel.clone(), result: Err(e) })
                    .await;
                continue;
            }
            self.fud
                .dht
                .event_publisher
                .notify(DhtEvent::PingSent { to: self.channel.clone(), result: Ok(()) })
                .await;

            // Ping the peer if this is an inbound connection
            if self.channel.session_type_id() & SESSION_INBOUND != 0 {
                let _ = self.fud.ping(self.channel.clone()).await;
            }
        }
    }

    async fn handle_fud_metadata_request(self: Arc<Self>) -> Result<()> {
        debug!(target: "fud::ProtocolFud::handle_fud_metadata_request()", "START");

        loop {
            let request = match self.find_metadata_request_sub.receive().await {
                Ok(v) => v,
                Err(Error::ChannelStopped) => continue,
                Err(_) => continue,
            };
            info!(target: "fud::ProtocolFud::handle_fud_request()", "Received METADATA REQUEST for {}", hash_to_string(&request.resource));
            self.fud.dht.update_channel(self.channel.info.id).await;

            let notfound = async || {
                let reply = FudMetadataNotFound { resource: request.resource };
                info!(target: "fud::ProtocolFud::handle_fud_metadata_request()", "We do not have the metadata of {}", hash_to_string(&request.resource));
                let _ = self.channel.send(&reply).await;
            };

            let path = self.fud.hash_to_path(&request.resource).ok().flatten();
            if path.is_none() {
                notfound().await;
                continue
            }
            let path = path.unwrap();

            let chunked_file = self.fud.geode.get(&request.resource, &path).await.ok();
            if chunked_file.is_none() {
                notfound().await;
                continue
            }
            let mut chunked_file = chunked_file.unwrap();

            // If it's a file with a single chunk, just reply with the chunk
            if chunked_file.len() == 1 && !chunked_file.is_dir() {
                let chunk_hash = chunked_file.get_chunks()[0].hash;
                let chunk = self.fud.geode.get_chunk(&mut chunked_file, &chunk_hash).await;
                if let Ok(chunk) = chunk {
                    if blake3::hash(blake3::hash(&chunk).as_bytes()) != request.resource {
                        // TODO: Run geode GC
                        notfound().await;
                        continue
                    }
                    let reply = FudChunkReply { resource: request.resource, chunk };
                    info!(target: "fud::ProtocolFud::handle_fud_metadata_request()", "Sending chunk (file has a single chunk) {}", hash_to_string(&chunk_hash));
                    let _ = self.channel.send(&reply).await;
                    continue
                }
                // We don't have the chunk, but we can still reply with the metadata
            }

            // Reply with the metadata
            match chunked_file.is_dir() {
                false => {
                    let reply = FudFileReply {
                        resource: request.resource,
                        chunk_hashes: chunked_file.get_chunks().iter().map(|c| c.hash).collect(),
                    };
                    info!(target: "fud::ProtocolFud::handle_fud_metadata_request()", "Sending file metadata {}", hash_to_string(&request.resource));
                    let _ = self.channel.send(&reply).await;
                }
                true => {
                    let files = chunked_file
                        .get_files()
                        .iter()
                        .map(|(file_path, size)| match file_path.strip_prefix(path.clone()) {
                            Ok(rel_path) => Ok((rel_path.to_string_lossy().to_string(), *size)),
                            Err(e) => Err(e),
                        })
                        .collect::<std::result::Result<Vec<_>, StripPrefixError>>();
                    if let Err(e) = files {
                        error!(target: "fud::ProtocolFud::handle_fud_metadata_request()", "Error parsing file paths before sending directory metadata: {e}");
                        notfound().await;
                        continue
                    }
                    let reply = FudDirectoryReply {
                        resource: request.resource,
                        chunk_hashes: chunked_file.get_chunks().iter().map(|c| c.hash).collect(),
                        files: files.unwrap(),
                    };
                    info!(target: "fud::ProtocolFud::handle_fud_metadata_request()", "Sending directory metadata {}", hash_to_string(&request.resource));
                    let _ = self.channel.send(&reply).await;
                }
            };
        }
    }

    async fn handle_fud_chunk_request(self: Arc<Self>) -> Result<()> {
        debug!(target: "fud::ProtocolFud::handle_fud_chunk_request()", "START");

        loop {
            let request = match self.find_chunk_request_sub.receive().await {
                Ok(v) => v,
                Err(Error::ChannelStopped) => continue,
                Err(_) => continue,
            };
            info!(target: "fud::ProtocolFud::handle_fud_chunk_request()", "Received CHUNK REQUEST for {}", hash_to_string(&request.resource));
            self.fud.dht.update_channel(self.channel.info.id).await;

            let notfound = async || {
                let reply = FudChunkNotFound { resource: request.resource, chunk: request.chunk };
                info!(target: "fud::ProtocolFud::handle_fud_chunk_request()", "We do not have chunk {} of resource {}", hash_to_string(&request.resource), hash_to_string(&request.chunk));
                let _ = self.channel.send(&reply).await;
            };

            let path = self.fud.hash_to_path(&request.resource).ok().flatten();
            if path.is_none() {
                notfound().await;
                continue
            }
            let path = path.unwrap();

            let chunked = self.fud.geode.get(&request.resource, &path).await;
            if chunked.is_err() {
                notfound().await;
                continue
            }

            let chunk = self.fud.geode.get_chunk(&mut chunked.unwrap(), &request.chunk).await;
            if let Ok(chunk) = chunk {
                if !self.fud.geode.verify_chunk(&request.chunk, &chunk) {
                    // TODO: Run geode GC
                    notfound().await;
                    continue
                }
                let reply = FudChunkReply { resource: request.resource, chunk };
                info!(target: "fud::ProtocolFud::handle_fud_chunk_request()", "Sending chunk {}", hash_to_string(&request.chunk));
                let _ = self.channel.send(&reply).await;
                continue
            }

            notfound().await;
        }
    }

    async fn handle_fud_nodes_request(self: Arc<Self>) -> Result<()> {
        debug!(target: "fud::ProtocolFud::handle_fud_nodes_request()", "START");

        loop {
            let request = match self.find_nodes_request_sub.receive().await {
                Ok(v) => v,
                Err(Error::ChannelStopped) => continue,
                Err(_) => continue,
            };
            info!(target: "fud::ProtocolFud::handle_fud_nodes_request()", "Received FIND NODES for {}", hash_to_string(&request.key));
            self.fud.dht.update_channel(self.channel.info.id).await;

            let reply = FudNodesReply {
                key: request.key,
                nodes: self.fud.dht().find_neighbors(&request.key, self.fud.dht().settings.k).await,
            };
            match self.channel.send(&reply).await {
                Ok(()) => continue,
                Err(_e) => continue,
            }
        }
    }

    async fn handle_fud_seeders_request(self: Arc<Self>) -> Result<()> {
        debug!(target: "fud::ProtocolFud::handle_fud_seeders_request()", "START");

        loop {
            let request = match self.find_seeders_request_sub.receive().await {
                Ok(v) => v,
                Err(Error::ChannelStopped) => continue,
                Err(_) => continue,
            };
            info!(target: "fud::ProtocolFud::handle_fud_seeders_request()", "Received FIND SEEDERS for {} from {:?}", hash_to_string(&request.key), self.channel);
            self.fud.dht.update_channel(self.channel.info.id).await;

            let router = self.fud.dht.hash_table.read().await;
            let peers = router.get(&request.key);

            match peers {
                Some(seeders) => {
                    let _ = self
                        .channel
                        .send(&FudSeedersReply {
                            key: request.key,
                            seeders: seeders.to_vec(),
                            nodes: self
                                .fud
                                .dht()
                                .find_neighbors(&request.key, self.fud.dht().settings.k)
                                .await,
                        })
                        .await;
                }
                None => {
                    let _ = self
                        .channel
                        .send(&FudSeedersReply {
                            key: request.key,
                            seeders: vec![],
                            nodes: self
                                .fud
                                .dht()
                                .find_neighbors(&request.key, self.fud.dht().settings.k)
                                .await,
                        })
                        .await;
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
                Err(_) => continue,
            };
            info!(target: "fud::ProtocolFud::handle_fud_announce()", "Received ANNOUNCE for {}", hash_to_string(&request.key));
            self.fud.dht.update_channel(self.channel.info.id).await;

            let mut seeders = vec![];

            for seeder in request.seeders.clone() {
                if seeder.node.addresses.is_empty() {
                    continue
                }
                if let Err(e) = self.fud.pow.write().await.verify_node(&seeder.node.data).await {
                    warn!(target: "fud::ProtocolFud::handle_fud_announce()", "Received seeder with invalid PoW: {e}");
                    continue
                }
                if !seeder.verify_signature().await {
                    warn!(target: "fud::ProtocolFud::handle_fud_announce()", "Received seeder with invalid signature");
                    continue
                }

                // TODO: Limit the number of addresses
                // TODO: Verify each address
                seeders.push(seeder);
            }

            self.fud.add_value(&request.key, &seeders).await;
        }
    }
}

#[async_trait]
impl ProtocolBase for ProtocolFud {
    async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        debug!(target: "fud::ProtocolFud::start()", "START");
        self.jobsman.clone().start(executor.clone());
        self.jobsman.clone().spawn(self.clone().handle_fud_ping_request(), executor.clone()).await;
        self.jobsman
            .clone()
            .spawn(self.clone().handle_fud_metadata_request(), executor.clone())
            .await;
        self.jobsman.clone().spawn(self.clone().handle_fud_chunk_request(), executor.clone()).await;
        self.jobsman.clone().spawn(self.clone().handle_fud_nodes_request(), executor.clone()).await;
        self.jobsman
            .clone()
            .spawn(self.clone().handle_fud_seeders_request(), executor.clone())
            .await;
        self.jobsman.clone().spawn(self.clone().handle_fud_announce(), executor.clone()).await;
        debug!(target: "fud::ProtocolFud::start()", "END");
        Ok(())
    }

    fn name(&self) -> &'static str {
        "ProtocolFud"
    }
}
