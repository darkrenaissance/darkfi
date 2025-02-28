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

use std::{collections::HashSet, sync::Arc};

use async_trait::async_trait;
use darkfi::{
    geode::MAX_CHUNK_SIZE,
    impl_p2p_message,
    net::{
        ChannelPtr, Message, MessageSubscription, P2pPtr, ProtocolBase, ProtocolBasePtr,
        ProtocolJobsManager, ProtocolJobsManagerPtr,
    },
    Error, Result,
};
use darkfi_serial::{SerialDecodable, SerialEncodable};
use log::{debug, error};
use smol::{fs::File, io::AsyncReadExt, Executor};
use url::Url;

use super::Fud;

/// Message representing a new file on the network
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct FudFilePut {
    pub file_hash: blake3::Hash,
    pub chunk_hashes: Vec<blake3::Hash>,
}
impl_p2p_message!(FudFilePut, "FudFilePut", 0);

/// Message representing a new chunk on the network
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct FudChunkPut {
    pub chunk_hash: blake3::Hash,
}
impl_p2p_message!(FudChunkPut, "FudChunkPut", 0);

/// Message representing a new route for a file on the network
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct FudFileRoute {
    pub file_hash: blake3::Hash,
    pub chunk_hashes: Vec<blake3::Hash>,
    pub peer: Url,
}
impl_p2p_message!(FudFileRoute, "FudFileRoute", 0);

/// Message representing a new route for a chunk on the network
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct FudChunkRoute {
    pub chunk_hash: blake3::Hash,
    pub peer: Url,
}
impl_p2p_message!(FudChunkRoute, "FudChunkRoute", 0);

/// Message representing a file request from the network
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct FudFileRequest {
    pub file_hash: blake3::Hash,
}
impl_p2p_message!(FudFileRequest, "FudFileRequest", 0);

/// Message representing a file reply from the network
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct FudFileReply {
    pub chunk_hashes: Vec<blake3::Hash>,
}
impl_p2p_message!(FudFileReply, "FudFileReply", 0);

/// Message representing a chunk request from the network
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct FudChunkRequest {
    pub chunk_hash: blake3::Hash,
}
impl_p2p_message!(FudChunkRequest, "FudChunkRequest", 0);

/// Message representing a chunk reply from the network
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct FudChunkReply {
    // TODO: This sould be a chunk-sized array, but then we need padding?
    pub chunk: Vec<u8>,
}
impl_p2p_message!(FudChunkReply, "FudChunkReply", 0);

/// Message representing a chunk reply when a file is not found
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct FudFileNotFound;
impl_p2p_message!(FudFileNotFound, "FudFileNotFound", 0);

/// Message representing a chunk reply when a chunk is not found
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct FudChunkNotFound;
impl_p2p_message!(FudChunkNotFound, "FudChunkNotFound", 0);

/// P2P protocol implementation for fud.
pub struct ProtocolFud {
    channel: ChannelPtr,
    file_put_sub: MessageSubscription<FudFilePut>,
    chunk_put_sub: MessageSubscription<FudChunkPut>,
    file_route_sub: MessageSubscription<FudFileRoute>,
    chunk_route_sub: MessageSubscription<FudChunkRoute>,
    file_request_sub: MessageSubscription<FudFileRequest>,
    chunk_request_sub: MessageSubscription<FudChunkRequest>,
    fud: Arc<Fud>,
    p2p: P2pPtr,
    jobsman: ProtocolJobsManagerPtr,
}

impl ProtocolFud {
    pub async fn init(fud: Arc<Fud>, channel: ChannelPtr, p2p: P2pPtr) -> Result<ProtocolBasePtr> {
        debug!(
            target: "fud::proto::ProtocolFud::init()",
            "Adding ProtocolFud to the protocol registry"
        );

        let msg_subsystem = channel.message_subsystem();
        msg_subsystem.add_dispatch::<FudFilePut>().await;
        msg_subsystem.add_dispatch::<FudChunkPut>().await;
        msg_subsystem.add_dispatch::<FudFileRoute>().await;
        msg_subsystem.add_dispatch::<FudChunkRoute>().await;
        msg_subsystem.add_dispatch::<FudFileRequest>().await;
        msg_subsystem.add_dispatch::<FudChunkRequest>().await;

        let file_put_sub = channel.subscribe_msg::<FudFilePut>().await?;
        let chunk_put_sub = channel.subscribe_msg::<FudChunkPut>().await?;
        let file_route_sub = channel.subscribe_msg::<FudFileRoute>().await?;
        let chunk_route_sub = channel.subscribe_msg::<FudChunkRoute>().await?;
        let file_request_sub = channel.subscribe_msg::<FudFileRequest>().await?;
        let chunk_request_sub = channel.subscribe_msg::<FudChunkRequest>().await?;

        Ok(Arc::new(Self {
            channel: channel.clone(),
            file_put_sub,
            chunk_put_sub,
            file_route_sub,
            chunk_route_sub,
            file_request_sub,
            chunk_request_sub,
            fud,
            p2p,
            jobsman: ProtocolJobsManager::new("ProtocolFud", channel.clone()),
        }))
    }

    async fn handle_fud_file_put(self: Arc<Self>) -> Result<()> {
        debug!(target: "fud::ProtocolFud::handle_fud_file_put()", "START");

        loop {
            let fud_file = match self.file_put_sub.receive().await {
                Ok(v) => v,
                Err(e) => {
                    error!(
                        target: "fud::ProtocolFud::handle_fud_file_put()",
                        "recv fail: {}", e,
                    );
                    continue
                }
            };

            // TODO: This approach is naive and optimistic. Needs to be fixed.
            let mut metadata_lock = self.fud.metadata_router.write().await;
            let file_route = metadata_lock.get_mut(&fud_file.file_hash);
            match file_route {
                Some(peers) => {
                    peers.insert(self.channel.address().clone());
                }
                None => {
                    let mut peers = HashSet::new();
                    peers.insert(self.channel.address().clone());
                    metadata_lock.insert(fud_file.file_hash, peers);
                }
            }
            drop(metadata_lock);

            let mut chunks_lock = self.fud.chunks_router.write().await;
            for chunk in &fud_file.chunk_hashes {
                let chunk_route = chunks_lock.get_mut(chunk);
                match chunk_route {
                    Some(peers) => {
                        peers.insert(self.channel.address().clone());
                    }
                    None => {
                        let mut peers = HashSet::new();
                        peers.insert(self.channel.address().clone());
                        chunks_lock.insert(*chunk, peers);
                    }
                }
            }
            drop(chunks_lock);

            // Relay this knowledge of the new route
            let route = FudFileRoute {
                file_hash: fud_file.file_hash,
                chunk_hashes: fud_file.chunk_hashes.clone(),
                peer: self.channel.address().clone(),
            };

            self.p2p.broadcast_with_exclude(&route, &[self.channel.address().clone()]).await;
        }
    }

    async fn handle_fud_chunk_put(self: Arc<Self>) -> Result<()> {
        debug!(target: "fud::ProtocolFud::handle_fud_chunk_put()", "START");

        loop {
            let fud_chunk = match self.chunk_put_sub.receive().await {
                Ok(v) => v,
                Err(e) => {
                    error!(
                        target: "fud::ProtocolFud::handle_fud_chunk_put()",
                        "recv fail: {}", e,
                    );
                    continue
                }
            };

            // TODO: This approach is naive and optimistic. Needs to be fixed.
            let mut chunks_lock = self.fud.chunks_router.write().await;
            let chunk_route = chunks_lock.get_mut(&fud_chunk.chunk_hash);
            match chunk_route {
                Some(peers) => {
                    peers.insert(self.channel.address().clone());
                }
                None => {
                    let mut peers = HashSet::new();
                    peers.insert(self.channel.address().clone());
                    chunks_lock.insert(fud_chunk.chunk_hash, peers);
                }
            }
            drop(chunks_lock);

            // Relay this knowledge of the new route
            let route = FudChunkRoute {
                chunk_hash: fud_chunk.chunk_hash,
                peer: self.channel.address().clone(),
            };

            self.p2p.broadcast_with_exclude(&route, &[self.channel.address().clone()]).await;
        }
    }

    async fn handle_fud_file_route(self: Arc<Self>) -> Result<()> {
        debug!(target: "fud::ProtocolFud::handle_fud_file_route()", "START");

        loop {
            let fud_file = match self.file_route_sub.receive().await {
                Ok(v) => v,
                Err(e) => {
                    error!(
                        target: "fud::ProtocolFud::handle_fud_file_route()",
                        "recv fail: {}", e,
                    );
                    continue
                }
            };

            // TODO: This approach is naive and optimistic. Needs to be fixed.
            let mut metadata_lock = self.fud.metadata_router.write().await;
            let file_route = metadata_lock.get_mut(&fud_file.file_hash);
            match file_route {
                Some(peers) => {
                    peers.insert(fud_file.peer.clone());
                }
                None => {
                    let mut peers = HashSet::new();
                    peers.insert(fud_file.peer.clone());
                    metadata_lock.insert(fud_file.file_hash, peers);
                }
            }
            drop(metadata_lock);

            let mut chunks_lock = self.fud.chunks_router.write().await;
            for chunk in &fud_file.chunk_hashes {
                let chunk_route = chunks_lock.get_mut(chunk);
                match chunk_route {
                    Some(peers) => {
                        peers.insert(fud_file.peer.clone());
                    }
                    None => {
                        let mut peers = HashSet::new();
                        peers.insert(fud_file.peer.clone());
                        chunks_lock.insert(*chunk, peers);
                    }
                }
            }
            drop(chunks_lock);

            // Relay this knowledge of the new route
            let route = FudFileRoute {
                file_hash: fud_file.file_hash,
                chunk_hashes: fud_file.chunk_hashes.clone(),
                peer: fud_file.peer.clone(),
            };

            self.p2p
                .broadcast_with_exclude(
                    &route,
                    &[self.channel.address().clone(), fud_file.peer.clone()],
                )
                .await;
        }
    }

    async fn handle_fud_chunk_route(self: Arc<Self>) -> Result<()> {
        debug!(target: "fud::ProtocolFud::handle_fud_chunk_route()", "START");

        loop {
            let fud_chunk = match self.chunk_route_sub.receive().await {
                Ok(v) => v,
                Err(e) => {
                    error!(
                        target: "fud::ProtocolFud::handle_fud_chunk_put()",
                        "recv fail: {}", e,
                    );
                    continue
                }
            };

            // TODO: This approach is naive and optimistic. Needs to be fixed.
            let mut chunks_lock = self.fud.chunks_router.write().await;
            let chunk_route = chunks_lock.get_mut(&fud_chunk.chunk_hash);
            match chunk_route {
                Some(peers) => {
                    peers.insert(fud_chunk.peer.clone());
                }
                None => {
                    let mut peers = HashSet::new();
                    peers.insert(fud_chunk.peer.clone());
                    chunks_lock.insert(fud_chunk.chunk_hash, peers);
                }
            }
            drop(chunks_lock);

            // Relay this knowledge of the new route
            let route =
                FudChunkRoute { chunk_hash: fud_chunk.chunk_hash, peer: fud_chunk.peer.clone() };

            self.p2p
                .broadcast_with_exclude(
                    &route,
                    &[self.channel.address().clone(), fud_chunk.peer.clone()],
                )
                .await;
        }
    }

    async fn handle_fud_file_request(self: Arc<Self>) -> Result<()> {
        debug!(target: "fud::ProtocolFud::handle_fud_file_request()", "START");

        loop {
            let file_request = match self.file_request_sub.receive().await {
                Ok(v) => v,
                Err(e) => {
                    error!(
                        target: "fud::ProtocolFud::handle_fud_file_request()",
                        "recv fail: {}", e,
                    );
                    continue
                }
            };

            let chunked_file = match self.fud.geode.get(&file_request.file_hash).await {
                Ok(v) => v,
                Err(Error::GeodeNeedsGc) => {
                    // TODO: Run GC
                    continue
                }

                Err(Error::GeodeFileNotFound) => match self.channel.send(&FudFileNotFound).await {
                    Ok(()) => continue,
                    Err(_e) => continue,
                },

                Err(_e) => continue,
            };

            let file_reply = FudFileReply {
                chunk_hashes: chunked_file.iter().map(|(chunk, _)| *chunk).collect(),
            };

            match self.channel.send(&file_reply).await {
                Ok(()) => continue,
                Err(_e) => continue,
            }
        }
    }

    async fn handle_fud_chunk_request(self: Arc<Self>) -> Result<()> {
        debug!(target: "fud::ProtocolFud::handle_fud_chunk_request()", "START");

        loop {
            let chunk_request = match self.chunk_request_sub.receive().await {
                Ok(v) => v,
                Err(e) => {
                    error!(
                        target: "fud::ProtocolFud::handle_fud_chunk_request()",
                        "recv fail: {}", e,
                    );
                    continue
                }
            };

            let chunk_path = match self.fud.geode.get_chunk(&chunk_request.chunk_hash).await {
                Ok(v) => v,
                Err(Error::GeodeNeedsGc) => {
                    // TODO: Run GC
                    continue
                }

                Err(Error::GeodeChunkNotFound) => {
                    match self.channel.send(&FudChunkNotFound).await {
                        Ok(()) => continue,
                        Err(_e) => continue,
                    }
                }

                Err(_e) => continue,
            };

            // The consistency should already be checked in Geode, so we're
            // fine not checking and unwrapping here.
            let mut buf = [0u8; MAX_CHUNK_SIZE];
            let mut chunk_fd = File::open(&chunk_path).await.unwrap();
            let bytes_read = chunk_fd.read(&mut buf).await.unwrap();
            let chunk_slice = &buf[..bytes_read];

            let reply = FudChunkReply { chunk: chunk_slice.to_vec() };
            match self.channel.send(&reply).await {
                Ok(()) => continue,
                Err(_e) => continue,
            }
        }
    }
}

#[async_trait]
impl ProtocolBase for ProtocolFud {
    async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        debug!(target: "fud::ProtocolFud::start()", "START");
        self.jobsman.clone().start(executor.clone());
        self.jobsman.clone().spawn(self.clone().handle_fud_file_put(), executor.clone()).await;
        self.jobsman.clone().spawn(self.clone().handle_fud_chunk_put(), executor.clone()).await;
        self.jobsman.clone().spawn(self.clone().handle_fud_file_route(), executor.clone()).await;
        self.jobsman.clone().spawn(self.clone().handle_fud_chunk_route(), executor.clone()).await;
        self.jobsman.clone().spawn(self.clone().handle_fud_file_request(), executor.clone()).await;
        self.jobsman.clone().spawn(self.clone().handle_fud_chunk_request(), executor.clone()).await;
        debug!(target: "fud::ProtocolFud::start()", "END");
        Ok(())
    }

    fn name(&self) -> &'static str {
        "ProtocolFud"
    }
}
