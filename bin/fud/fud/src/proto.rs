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

use std::collections::HashSet;

use async_std::sync::Arc;
use async_trait::async_trait;
use darkfi::{
    impl_p2p_message,
    net::{
        ChannelPtr, Message, MessageSubscription, P2pPtr, ProtocolBase, ProtocolBasePtr,
        ProtocolJobsManager, ProtocolJobsManagerPtr,
    },
    Result,
};
use darkfi_serial::{SerialDecodable, SerialEncodable};
use log::{debug, error};
use smol::Executor;
use url::Url;

use super::Fud;

/// Message representing a new file on the network
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct FudFilePut {
    pub file_hash: blake3::Hash,
    pub chunk_hashes: Vec<blake3::Hash>,
}
impl_p2p_message!(FudFilePut, "FudFilePut");

/// Message representing a new chunk on the network
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct FudChunkPut {
    pub chunk_hash: blake3::Hash,
}
impl_p2p_message!(FudChunkPut, "FudChunkPut");

/// Message representing a new route for a file on the network
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct FudFileRoute {
    pub file_hash: blake3::Hash,
    pub chunk_hashes: Vec<blake3::Hash>,
    pub peer: Url,
}
impl_p2p_message!(FudFileRoute, "FudFileRoute");

/// Message representing a new route for a chunk on the network
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct FudChunkRoute {
    pub chunk_hash: blake3::Hash,
    pub peer: Url,
}
impl_p2p_message!(FudChunkRoute, "FudChunkRoute");

/// P2P protocol implementation for fud.
pub struct ProtocolFud {
    channel_address: Url,
    file_put_sub: MessageSubscription<FudFilePut>,
    chunk_put_sub: MessageSubscription<FudChunkPut>,
    file_route_sub: MessageSubscription<FudFileRoute>,
    chunk_route_sub: MessageSubscription<FudChunkRoute>,
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

        let file_put_sub = channel.subscribe_msg::<FudFilePut>().await?;
        let chunk_put_sub = channel.subscribe_msg::<FudChunkPut>().await?;
        let file_route_sub = channel.subscribe_msg::<FudFileRoute>().await?;
        let chunk_route_sub = channel.subscribe_msg::<FudChunkRoute>().await?;

        Ok(Arc::new(Self {
            channel_address: channel.address().clone(),
            file_put_sub,
            chunk_put_sub,
            file_route_sub,
            chunk_route_sub,
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
                    peers.insert(self.channel_address.clone());
                }
                None => {
                    let mut peers = HashSet::new();
                    peers.insert(self.channel_address.clone());
                    metadata_lock.insert(fud_file.file_hash, peers);
                }
            }
            drop(metadata_lock);

            let mut chunks_lock = self.fud.chunks_router.write().await;
            for chunk in &fud_file.chunk_hashes {
                let chunk_route = chunks_lock.get_mut(chunk);
                match chunk_route {
                    Some(peers) => {
                        peers.insert(self.channel_address.clone());
                    }
                    None => {
                        let mut peers = HashSet::new();
                        peers.insert(self.channel_address.clone());
                        chunks_lock.insert(*chunk, peers);
                    }
                }
            }
            drop(chunks_lock);

            // Relay this knowledge of the new route
            let route = FudFileRoute {
                file_hash: fud_file.file_hash,
                chunk_hashes: fud_file.chunk_hashes.clone(),
                peer: self.channel_address.clone(),
            };

            self.p2p.broadcast_with_exclude(&route, &[self.channel_address.clone()]).await;
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
                    peers.insert(self.channel_address.clone());
                }
                None => {
                    let mut peers = HashSet::new();
                    peers.insert(self.channel_address.clone());
                    chunks_lock.insert(fud_chunk.chunk_hash, peers);
                }
            }
            drop(chunks_lock);

            // Relay this knowledge of the new route
            let route = FudChunkRoute {
                chunk_hash: fud_chunk.chunk_hash,
                peer: self.channel_address.clone(),
            };

            self.p2p.broadcast_with_exclude(&route, &[self.channel_address.clone()]).await;
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
                    &[self.channel_address.clone(), fud_file.peer.clone()],
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
                    &[self.channel_address.clone(), fud_chunk.peer.clone()],
                )
                .await;
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
        debug!(target: "fud::ProtocolFud::start()", "END");
        Ok(())
    }

    fn name(&self) -> &'static str {
        "ProtocolFud"
    }
}
