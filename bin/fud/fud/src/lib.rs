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
use futures::{future::FutureExt, pin_mut, select};
use log::{debug, error, info, warn};
use num_bigint::BigUint;
use rand::{prelude::IteratorRandom, rngs::OsRng, seq::SliceRandom, RngCore};
use sled_overlay::sled;
use smol::{
    channel,
    fs::{File, OpenOptions},
    io::{AsyncReadExt, AsyncWriteExt},
    lock::RwLock,
};
use std::{
    collections::{HashMap, HashSet},
    io::ErrorKind,
    path::{Path, PathBuf},
    sync::Arc,
};

use darkfi::{
    dht::{Dht, DhtHandler, DhtNode, DhtRouterItem, DhtRouterPtr},
    geode::{hash_to_string, ChunkedFile, Geode},
    net::{ChannelPtr, P2pPtr},
    system::PublisherPtr,
    util::path::expand_path,
    Error, Result,
};

/// P2P protocols
pub mod proto;
use proto::{
    FudAnnounce, FudChunkReply, FudFileReply, FudFindNodesReply, FudFindNodesRequest,
    FudFindRequest, FudFindSeedersReply, FudFindSeedersRequest, FudNotFound, FudPingReply,
    FudPingRequest,
};

/// FudEvent
pub mod event;
use event::{ChunkDownloadCompleted, ChunkNotFound, FudEvent, ResourceUpdated};

/// Resource definition
pub mod resource;
use resource::{Resource, ResourceStatus};

/// JSON-RPC related methods
pub mod rpc;

/// Background tasks
pub mod tasks;
use tasks::FetchReply;

// TODO: This is not Sybil-resistant
fn generate_node_id() -> Result<blake3::Hash> {
    let mut rng = OsRng;
    let mut random_data = [0u8; 32];
    rng.fill_bytes(&mut random_data);
    let node_id = blake3::Hash::from_bytes(random_data);
    Ok(node_id)
}

/// Get or generate the node id.
/// Fetches and saves the node id from/to a file.
pub async fn get_node_id(node_id_path: &Path) -> Result<blake3::Hash> {
    match File::open(node_id_path).await {
        Ok(mut file) => {
            let mut buffer = Vec::new();
            file.read_to_end(&mut buffer).await?;
            let mut out_buf = [0u8; 32];
            bs58::decode(buffer).onto(&mut out_buf)?;
            let node_id = blake3::Hash::from_bytes(out_buf);
            Ok(node_id)
        }
        Err(e) if e.kind() == ErrorKind::NotFound => {
            let node_id = generate_node_id()?;
            let mut file = OpenOptions::new().write(true).create(true).open(node_id_path).await?;
            file.write_all(&bs58::encode(node_id.as_bytes()).into_vec()).await?;
            file.flush().await?;
            Ok(node_id)
        }
        Err(e) => Err(e.into()),
    }
}

pub struct Fud {
    /// Key -> Seeders
    seeders_router: DhtRouterPtr,

    /// Pointer to the P2P network instance
    p2p: P2pPtr,

    /// The Geode instance
    geode: Geode,

    /// Default download directory
    downloads_path: PathBuf,

    /// Chunk transfer timeout in seconds
    chunk_timeout: u64,

    /// The DHT instance
    dht: Arc<Dht>,

    /// Resources (current status of all downloads/seeds)
    resources: Arc<RwLock<HashMap<blake3::Hash, Resource>>>,

    /// Sled tree containing "resource hash -> path on the filesystem"
    path_tree: sled::Tree,

    get_tx: channel::Sender<(u16, blake3::Hash, PathBuf, Result<()>)>,
    get_rx: channel::Receiver<(u16, blake3::Hash, PathBuf, Result<()>)>,
    file_fetch_tx: channel::Sender<(Vec<DhtNode>, blake3::Hash, PathBuf, Result<()>)>,
    file_fetch_rx: channel::Receiver<(Vec<DhtNode>, blake3::Hash, PathBuf, Result<()>)>,
    file_fetch_end_tx: channel::Sender<(blake3::Hash, Result<()>)>,
    file_fetch_end_rx: channel::Receiver<(blake3::Hash, Result<()>)>,

    event_publisher: PublisherPtr<FudEvent>,
}

#[async_trait]
impl DhtHandler for Fud {
    fn dht(&self) -> Arc<Dht> {
        self.dht.clone()
    }

    async fn ping(&self, channel: ChannelPtr) -> Result<DhtNode> {
        debug!(target: "fud::DhtHandler::ping()", "Sending ping to channel {}", channel.info.id);
        let msg_subsystem = channel.message_subsystem();
        msg_subsystem.add_dispatch::<FudPingReply>().await;
        let msg_subscriber = channel.subscribe_msg::<FudPingReply>().await.unwrap();
        let request = FudPingRequest {};

        channel.send(&request).await?;

        let reply = msg_subscriber.receive_with_timeout(self.dht().settings.timeout).await?;

        msg_subscriber.unsubscribe().await;

        Ok(reply.node.clone())
    }

    // TODO: Optimize this
    async fn on_new_node(&self, node: &DhtNode) -> Result<()> {
        debug!(target: "fud::DhtHandler::on_new_node()", "New node {}", hash_to_string(&node.id));

        // If this is the first node we know about, then bootstrap
        if !self.dht().is_bootstrapped().await {
            self.dht().set_bootstrapped().await;

            // Lookup our own node id
            debug!(target: "fud::DhtHandler::on_new_node()", "DHT bootstrapping {}", hash_to_string(&self.dht().node_id));
            let _ = self.lookup_nodes(&self.dht().node_id).await;
        }

        // Send keys that are closer to this node than we are
        let self_id = self.dht().node_id;
        let channel = self.get_channel(node).await?;
        for (key, seeders) in self.seeders_router.read().await.iter() {
            let node_distance = BigUint::from_bytes_be(&self.dht().distance(key, &node.id));
            let self_distance = BigUint::from_bytes_be(&self.dht().distance(key, &self_id));
            if node_distance <= self_distance {
                let _ = channel
                    .send(&FudAnnounce {
                        key: *key,
                        seeders: seeders.clone().into_iter().collect(),
                    })
                    .await;
            }
        }

        Ok(())
    }

    async fn fetch_nodes(&self, node: &DhtNode, key: &blake3::Hash) -> Result<Vec<DhtNode>> {
        debug!(target: "fud::DhtHandler::fetch_value()", "Fetching nodes close to {} from node {}", hash_to_string(key), hash_to_string(&node.id));

        let channel = self.get_channel(node).await?;
        let msg_subsystem = channel.message_subsystem();
        msg_subsystem.add_dispatch::<FudFindNodesReply>().await;
        let msg_subscriber_nodes = channel.subscribe_msg::<FudFindNodesReply>().await.unwrap();

        let request = FudFindNodesRequest { key: *key };
        channel.send(&request).await?;

        let reply = msg_subscriber_nodes.receive_with_timeout(self.dht().settings.timeout).await?;

        msg_subscriber_nodes.unsubscribe().await;

        Ok(reply.nodes.clone())
    }
}

impl Fud {
    pub async fn new(
        p2p: P2pPtr,
        basedir: PathBuf,
        downloads_path: PathBuf,
        chunk_timeout: u64,
        dht: Arc<Dht>,
        path_tree: sled::Tree,
        event_publisher: PublisherPtr<FudEvent>,
    ) -> Result<Self> {
        let (get_tx, get_rx) = smol::channel::unbounded();
        let (file_fetch_tx, file_fetch_rx) = smol::channel::unbounded();
        let (file_fetch_end_tx, file_fetch_end_rx) = smol::channel::unbounded();

        // Hashmap used for routing
        let seeders_router = Arc::new(RwLock::new(HashMap::new()));

        info!("Instantiating Geode instance");
        let geode = Geode::new(&basedir).await?;

        info!("Instantiating DHT instance");

        let fud = Self {
            seeders_router,
            p2p,
            geode,
            downloads_path,
            chunk_timeout,
            dht,
            path_tree,
            resources: Arc::new(RwLock::new(HashMap::new())),
            get_tx,
            get_rx,
            file_fetch_tx,
            file_fetch_rx,
            file_fetch_end_tx,
            file_fetch_end_rx,
            event_publisher,
        };

        fud.init().await?;

        Ok(fud)
    }

    /// Add ourselves to `seeders_router` for the files we already have.
    /// Skipped if we have no external address.
    async fn init(&self) -> Result<()> {
        info!(target: "fud::init()", "Finding resources...");
        let mut resources_write = self.resources.write().await;
        for result in self.path_tree.iter() {
            if result.is_err() {
                continue;
            }

            // Parse hash
            let (hash, path) = result.unwrap();
            let hash_bytes: [u8; 32] = match hash.to_vec().try_into() {
                Ok(v) => v,
                Err(_) => continue,
            };
            let hash = blake3::Hash::from_bytes(hash_bytes);

            // Parse path
            let path_bytes = path.to_vec();
            let path_str = match std::str::from_utf8(&path_bytes) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let path: PathBuf = match expand_path(path_str) {
                Ok(v) => v,
                Err(_) => continue,
            };

            // Add resource
            resources_write.insert(
                hash,
                Resource {
                    hash,
                    path,
                    status: ResourceStatus::Incomplete,
                    chunks_total: 0,
                    chunks_downloaded: 0,
                },
            );
        }
        drop(resources_write);

        info!(target: "fud::init()", "Verifying resources...");
        let resources = self.verify_resources(None).await?;

        let self_node = self.dht().node().await;

        if self_node.addresses.is_empty() {
            return Ok(());
        }

        info!(target: "fud::init()", "Start seeding...");
        let self_router_items: Vec<DhtRouterItem> = vec![self_node.into()];

        for resource in resources {
            self.add_to_router(
                self.seeders_router.clone(),
                &resource.hash,
                self_router_items.clone(),
            )
            .await;
        }

        Ok(())
    }

    /// Get resource path from hash using the sled db
    pub fn hash_to_path(&self, hash: &blake3::Hash) -> Result<Option<PathBuf>> {
        if let Some(value) = self.path_tree.get(hash.as_bytes())? {
            let path: PathBuf = expand_path(std::str::from_utf8(&value)?)?;
            return Ok(Some(path));
        }

        Ok(None)
    }

    /// Verify if resources are complete and uncorrupted.
    /// If a resource is incomplete or corrupted, its status is changed to Incomplete.
    /// If a resource is complete, its status is changed to Seeding.
    /// Takes an optional list of hashes.
    /// If no hash is given (None), it verifies all resources.
    /// Returns the list of verified and uncorrupted/complete seeding resources.
    pub async fn verify_resources(
        &self,
        hashes: Option<Vec<blake3::Hash>>,
    ) -> Result<Vec<Resource>> {
        let mut resources_write = self.resources.write().await;

        let update_resource =
            async |resource: &mut Resource,
                   status: ResourceStatus,
                   chunked_file: Option<&ChunkedFile>| {
                resource.status = status;
                resource.chunks_total = match chunked_file {
                    Some(chunked_file) => chunked_file.len() as u64,
                    None => 0,
                };
                resource.chunks_downloaded = match chunked_file {
                    Some(chunked_file) => chunked_file.local_chunks() as u64,
                    None => 0,
                };

                self.event_publisher
                    .notify(FudEvent::ResourceUpdated(ResourceUpdated {
                        hash: resource.hash,
                        resource: resource.clone(),
                    }))
                    .await;
            };

        let mut seeding_resources: Vec<Resource> = vec![];
        for (_, mut resource) in resources_write.iter_mut() {
            if let Some(ref hashes_list) = hashes {
                if !hashes_list.contains(&resource.hash) {
                    continue;
                }
            }

            match resource.status {
                ResourceStatus::Seeding => {}
                ResourceStatus::Incomplete => {}
                _ => continue,
            };

            // Make sure the resource is not corrupted or incomplete
            let resource_path = match self.hash_to_path(&resource.hash) {
                Ok(Some(v)) => v,
                Ok(None) | Err(_) => {
                    update_resource(&mut resource, ResourceStatus::Incomplete, None).await;
                    continue;
                }
            };
            let chunked_file = match self.geode.get(&resource.hash, &resource_path).await {
                Ok(v) => v,
                Err(_) => {
                    update_resource(&mut resource, ResourceStatus::Incomplete, None).await;
                    continue;
                }
            };
            if !chunked_file.is_complete() {
                update_resource(&mut resource, ResourceStatus::Incomplete, Some(&chunked_file))
                    .await;
                continue;
            }

            update_resource(&mut resource, ResourceStatus::Seeding, Some(&chunked_file)).await;
            seeding_resources.push(resource.clone());
        }

        Ok(seeding_resources)
    }

    /// Query `nodes` to find the seeders for `key`
    async fn fetch_seeders(
        &self,
        nodes: &Vec<DhtNode>,
        key: &blake3::Hash,
    ) -> HashSet<DhtRouterItem> {
        let mut seeders: HashSet<DhtRouterItem> = HashSet::new();

        for node in nodes {
            let channel = match self.get_channel(node).await {
                Ok(channel) => channel,
                Err(e) => {
                    warn!(target: "fud::fetch_seeders()", "Could not get a channel for node {}: {}", hash_to_string(&node.id), e);
                    continue;
                }
            };
            let msg_subsystem = channel.message_subsystem();
            msg_subsystem.add_dispatch::<FudFindSeedersReply>().await;

            let msg_subscriber = match channel.subscribe_msg::<FudFindSeedersReply>().await {
                Ok(msg_subscriber) => msg_subscriber,
                Err(e) => {
                    warn!(target: "fud::fetch_seeders()", "Error subscribing to msg: {}", e);
                    continue;
                }
            };

            let send_res = channel.send(&FudFindSeedersRequest { key: *key }).await;
            if let Err(e) = send_res {
                warn!(target: "fud::fetch_seeders()", "Error while sending FudFindSeedersRequest: {}", e);
                msg_subscriber.unsubscribe().await;
                continue;
            }

            let reply = match msg_subscriber.receive_with_timeout(self.dht().settings.timeout).await
            {
                Ok(reply) => reply,
                Err(e) => {
                    warn!(target: "fud::fetch_seeders()", "Error waiting for reply: {}", e);
                    msg_subscriber.unsubscribe().await;
                    continue;
                }
            };

            msg_subscriber.unsubscribe().await;

            seeders.extend(reply.seeders.clone());
        }

        info!(target: "fud::fetch_seeders()", "Found {} seeders for {}", seeders.len(), hash_to_string(key));
        seeders
    }

    /// Fetch chunks for a file from `seeders`
    async fn fetch_chunks(
        &self,
        file_path: &PathBuf,
        file_hash: &blake3::Hash,
        chunk_hashes: &HashSet<blake3::Hash>,
        seeders: &HashSet<DhtRouterItem>,
    ) -> Result<()> {
        let mut remaining_chunks = chunk_hashes.clone();
        let mut shuffled_seeders = {
            let mut vec: Vec<_> = seeders.iter().cloned().collect();
            vec.shuffle(&mut OsRng);
            vec
        };

        while let Some(seeder) = shuffled_seeders.pop() {
            let channel = match self.get_channel(&seeder.node).await {
                Ok(channel) => channel,
                Err(e) => {
                    warn!(target: "fud::fetch_chunks()", "Could not get a channel for node {}: {}", hash_to_string(&seeder.node.id), e);
                    continue;
                }
            };
            let mut chunks_to_query = remaining_chunks.clone();
            info!("Requesting chunks from seeder {}", hash_to_string(&seeder.node.id));
            loop {
                let msg_subsystem = channel.message_subsystem();
                msg_subsystem.add_dispatch::<FudChunkReply>().await;
                msg_subsystem.add_dispatch::<FudNotFound>().await;
                let msg_subscriber_chunk = channel.subscribe_msg::<FudChunkReply>().await.unwrap();
                let msg_subscriber_notfound = channel.subscribe_msg::<FudNotFound>().await.unwrap();

                // Select a chunk to request
                let mut chunk_hash: Option<blake3::Hash> = None;
                if let Some(&random_chunk) = chunks_to_query.iter().choose(&mut OsRng) {
                    chunk_hash = Some(random_chunk);
                }

                if chunk_hash.is_none() {
                    // No more chunks to request from this seeder
                    break; // Switch to another seeder
                }
                let chunk_hash = chunk_hash.unwrap();
                chunks_to_query.remove(&chunk_hash);

                let send_res =
                    channel.send(&FudFindRequest { info: Some(*file_hash), key: chunk_hash }).await;
                if let Err(e) = send_res {
                    warn!(target: "fud::fetch_chunks()", "Error while sending FudFindRequest: {}", e);
                    break; // Switch to another seeder
                }

                let chunk_recv =
                    msg_subscriber_chunk.receive_with_timeout(self.chunk_timeout).fuse();
                let notfound_recv =
                    msg_subscriber_notfound.receive_with_timeout(self.chunk_timeout).fuse();

                pin_mut!(chunk_recv, notfound_recv);

                // Wait for a FudChunkReply or FudNotFound
                select! {
                    chunk_reply = chunk_recv => {
                        if let Err(e) = chunk_reply {
                            warn!(target: "fud::fetch_chunks()", "Error waiting for chunk reply: {}", e);
                            break; // Switch to another seeder
                        }
                        let reply = chunk_reply.unwrap();

                        match self.geode.write_chunk(file_hash, file_path, &reply.chunk).await {
                            Ok(inserted_hash) => {
                                if inserted_hash != chunk_hash {
                                    warn!("Received chunk does not match requested chunk");
                                    msg_subscriber_chunk.unsubscribe().await;
                                    msg_subscriber_notfound.unsubscribe().await;
                                    continue; // Skip to next chunk, will retry this chunk later
                                }

                                // Update resource `chunks_downloaded`
                                let mut resources_write = self.resources.write().await;
                                let resource = match resources_write.get_mut(file_hash) {
                                    Some(resource) => {
                                        resource.status = ResourceStatus::Downloading;
                                        resource.chunks_downloaded += 1;
                                        resource.clone()
                                    }
                                    None => return Ok(()) // Resource was removed, abort
                                };
                                drop(resources_write);

                                info!(target: "fud::fetch_chunks()", "Received chunk {} from seeder {}", hash_to_string(&chunk_hash), hash_to_string(&seeder.node.id));
                                self.event_publisher
                                    .notify(FudEvent::ChunkDownloadCompleted(ChunkDownloadCompleted {
                                        hash: *file_hash,
                                        chunk_hash,
                                        resource,
                                    }))
                                    .await;
                                remaining_chunks.remove(&chunk_hash);
                            }
                            Err(e) => {
                                error!("Failed inserting chunk {} to Geode: {}", hash_to_string(&chunk_hash), e);
                            }
                        };
                    }
                    notfound_reply = notfound_recv => {
                        if let Err(e) = notfound_reply {
                            warn!(target: "fud::fetch_chunks()", "Error waiting for NOTFOUND reply: {}", e);
                            msg_subscriber_chunk.unsubscribe().await;
                            msg_subscriber_notfound.unsubscribe().await;
                            break; // Switch to another seeder
                        }
                        info!(target: "fud::fetch_chunks()", "Received NOTFOUND {} from seeder {}", hash_to_string(&chunk_hash), hash_to_string(&seeder.node.id));
                        self.event_publisher
                            .notify(FudEvent::ChunkNotFound(ChunkNotFound {
                                hash: *file_hash,
                                chunk_hash,
                            }))
                        .await;
                    }
                };

                msg_subscriber_chunk.unsubscribe().await;
                msg_subscriber_notfound.unsubscribe().await;
            }

            // Stop when there are no missing chunks
            if remaining_chunks.is_empty() {
                break;
            }
        }

        Ok(())
    }

    /// Fetch a single file metadata from `nodes`.
    /// If the file is smaller than a single chunk then the chunk is returned.
    /// 1. Request seeders for the file from those nodes
    /// 2. Request the file from the seeders
    pub async fn fetch_file_metadata(
        &self,
        nodes: Vec<DhtNode>,
        file_hash: blake3::Hash,
    ) -> Option<FetchReply> {
        let mut queried_seeders: HashSet<blake3::Hash> = HashSet::new();
        let mut result: Option<FetchReply> = None;

        for node in nodes {
            // 1. Request list of seeders
            let channel = match self.get_channel(&node).await {
                Ok(channel) => channel,
                Err(e) => {
                    warn!(target: "fud::fetch_file_metadata()", "Could not get a channel for node {}: {}", hash_to_string(&node.id), e);
                    continue;
                }
            };
            let msg_subsystem = channel.message_subsystem();
            msg_subsystem.add_dispatch::<FudFindSeedersReply>().await;

            let msg_subscriber = match channel.subscribe_msg::<FudFindSeedersReply>().await {
                Ok(msg_subscriber) => msg_subscriber,
                Err(e) => {
                    warn!(target: "fud::fetch_file_metadata()", "Error subscribing to msg: {}", e);
                    continue;
                }
            };

            let send_res = channel.send(&FudFindSeedersRequest { key: file_hash }).await;
            if let Err(e) = send_res {
                warn!(target: "fud::fetch_file_metadata()", "Error while sending FudFindSeedersRequest: {}", e);
                msg_subscriber.unsubscribe().await;
                continue;
            }

            let reply = match msg_subscriber.receive_with_timeout(self.dht().settings.timeout).await
            {
                Ok(reply) => reply,
                Err(e) => {
                    warn!(target: "fud::fetch_file_metadata()", "Error waiting for reply: {}", e);
                    msg_subscriber.unsubscribe().await;
                    continue;
                }
            };

            let mut seeders = reply.seeders.clone();
            info!(target: "fud::fetch_file_metadata()", "Found {} seeders for {} (from {})", seeders.len(), hash_to_string(&file_hash), hash_to_string(&node.id));

            msg_subscriber.unsubscribe().await;

            // 2. Request the file/chunk from the seeders
            while let Some(seeder) = seeders.pop() {
                // Only query a seeder once
                if queried_seeders.iter().any(|s| *s == seeder.node.id) {
                    continue;
                }
                queried_seeders.insert(seeder.node.id);

                if let Ok(channel) = self.get_channel(&seeder.node).await {
                    let msg_subsystem = channel.message_subsystem();
                    msg_subsystem.add_dispatch::<FudChunkReply>().await;
                    msg_subsystem.add_dispatch::<FudFileReply>().await;
                    msg_subsystem.add_dispatch::<FudNotFound>().await;
                    let msg_subscriber_chunk =
                        channel.subscribe_msg::<FudChunkReply>().await.unwrap();
                    let msg_subscriber_file =
                        channel.subscribe_msg::<FudFileReply>().await.unwrap();
                    let msg_subscriber_notfound =
                        channel.subscribe_msg::<FudNotFound>().await.unwrap();

                    let send_res =
                        channel.send(&FudFindRequest { info: None, key: file_hash }).await;
                    if let Err(e) = send_res {
                        warn!(target: "fud::fetch_file_metadata()", "Error while sending FudFindRequest: {}", e);
                        msg_subscriber_chunk.unsubscribe().await;
                        msg_subscriber_file.unsubscribe().await;
                        msg_subscriber_notfound.unsubscribe().await;
                        continue;
                    }

                    let chunk_recv =
                        msg_subscriber_chunk.receive_with_timeout(self.chunk_timeout).fuse();
                    let file_recv =
                        msg_subscriber_file.receive_with_timeout(self.chunk_timeout).fuse();
                    let notfound_recv =
                        msg_subscriber_notfound.receive_with_timeout(self.chunk_timeout).fuse();

                    pin_mut!(chunk_recv, file_recv, notfound_recv);

                    let cleanup = async || {
                        msg_subscriber_chunk.unsubscribe().await;
                        msg_subscriber_file.unsubscribe().await;
                        msg_subscriber_notfound.unsubscribe().await;
                    };

                    // Wait for a FudChunkReply, FudFileReply, or FudNotFound
                    select! {
                        // Received a chunk while requesting a file, this is allowed to
                        // optimize fetching files smaller than a single chunk
                        chunk_reply = chunk_recv => {
                            cleanup().await;
                            if let Err(e) = chunk_reply {
                                warn!(target: "fud::fetch_file_metadata()", "Error waiting for chunk reply: {}", e);
                                continue;
                            }
                            let reply = chunk_reply.unwrap();
                            let chunk_hash = blake3::hash(&reply.chunk);
                            // Check that this is the only chunk in the file
                            if !self.geode.verify_file(&file_hash, &[chunk_hash]) {
                                warn!(target: "fud::fetch_file_metadata()", "Received a chunk while fetching a file, the chunk did not match the file hash");
                                continue;
                            }
                            info!(target: "fud::fetch_file_metadata()", "Received chunk {} (for file {}) from seeder {}", hash_to_string(&chunk_hash), hash_to_string(&file_hash), hash_to_string(&seeder.node.id));
                            result = Some(FetchReply::Chunk((*reply).clone()));
                            break;
                        }
                        file_reply = file_recv => {
                            cleanup().await;
                            if let Err(e) = file_reply {
                                warn!(target: "fud::fetch_file_metadata()", "Error waiting for file reply: {}", e);
                                continue;
                            }
                            let reply = file_reply.unwrap();
                            if !self.geode.verify_file(&file_hash, &reply.chunk_hashes) {
                                warn!(target: "fud::fetch_file_metadata()", "Received invalid file metadata");
                                continue;
                            }
                            info!(target: "fud::fetch_file_metadata()", "Received file {} from seeder {}", hash_to_string(&file_hash), hash_to_string(&seeder.node.id));
                            result = Some(FetchReply::File((*reply).clone()));
                            break;
                        }
                        notfound_reply = notfound_recv => {
                            cleanup().await;
                            if let Err(e) = notfound_reply {
                                warn!(target: "fud::fetch_file_metadata()", "Error waiting for NOTFOUND reply: {}", e);
                                continue;
                            }
                            info!(target: "fud::fetch_file_metadata()", "Received NOTFOUND {} from seeder {}", hash_to_string(&file_hash), hash_to_string(&seeder.node.id));
                        }
                    };
                }
            }

            if result.is_some() {
                break;
            }
        }

        result
    }

    /// Download a file from the network to `file_path`.
    pub async fn get(&self, file_hash: &blake3::Hash, file_path: &PathBuf) -> Result<()> {
        let self_node = self.dht().node().await;
        let mut closest_nodes = vec![];

        // Add path to the sled db
        self.path_tree
            .insert(file_hash.as_bytes(), file_path.to_string_lossy().to_string().as_bytes())?;

        // Add resource to `self.resources`
        let resource = Resource {
            hash: *file_hash,
            path: file_path.clone(),
            status: ResourceStatus::Discovering,
            chunks_total: 0,
            chunks_downloaded: 0,
        };
        let mut resources_write = self.resources.write().await;
        resources_write.insert(*file_hash, resource.clone());
        drop(resources_write);

        // Send a DownloadStarted event
        self.event_publisher
            .notify(FudEvent::DownloadStarted(event::DownloadStarted {
                hash: *file_hash,
                resource,
            }))
            .await;

        // Try to get the chunked file from geode
        let chunked_file = match self.geode.get(file_hash, file_path).await {
            // We already know the list of chunk hashes for this file
            Ok(v) => v,
            // The metadata in geode is invalid or corrupted
            Err(Error::GeodeNeedsGc) => todo!(),
            // If we could not find the file in geode, get the file metadata from the network
            Err(Error::GeodeFileNotFound) => {
                // Find nodes close to the file hash
                info!(target: "self::get()", "Requested file {} not found in Geode, triggering fetch", hash_to_string(file_hash));
                closest_nodes = self.lookup_nodes(file_hash).await.unwrap_or_default();

                // Fetch file metadata (list of chunk hashes)
                self.file_fetch_tx
                    .send((closest_nodes.clone(), *file_hash, file_path.clone(), Ok(())))
                    .await
                    .unwrap();
                info!(target: "self::get()", "Waiting for background file fetch task...");
                let (i_file_hash, status) = self.file_fetch_end_rx.recv().await.unwrap();
                match status {
                    // The file metadata was found and inserted into geode
                    Ok(()) => self.geode.get(&i_file_hash, file_path).await.unwrap(),
                    // We could not find the file metadata
                    Err(Error::GeodeFileRouteNotFound) => {
                        // Set resource status to `Incomplete` and send FudEvent::FileNotFound
                        let mut resources_write = self.resources.write().await;
                        if let Some(resource) = resources_write.get_mut(file_hash) {
                            resource.status = ResourceStatus::Incomplete;

                            self.event_publisher
                                .notify(FudEvent::FileNotFound(event::FileNotFound {
                                    hash: *file_hash,
                                    resource: resource.clone(),
                                }))
                                .await;
                        }
                        drop(resources_write);
                        return Err(Error::GeodeFileRouteNotFound);
                    }

                    Err(e) => {
                        error!(target: "fud::handle_get()", "{}", e);
                        return Err(e);
                    }
                }
            }

            Err(e) => {
                error!(target: "fud::handle_get()", "{}", e);
                return Err(e);
            }
        };

        // Set resource status to `Downloading`
        let mut resources_write = self.resources.write().await;
        let resource = match resources_write.get_mut(file_hash) {
            Some(resource) => {
                resource.status = ResourceStatus::Downloading;
                resource.chunks_downloaded = chunked_file.local_chunks() as u64;
                resource.chunks_total = chunked_file.len() as u64;
                resource.clone()
            }
            None => return Ok(()), // Resource was removed, abort
        };
        drop(resources_write);

        // Send a FileDownloadCompleted event
        self.event_publisher
            .notify(FudEvent::FileDownloadCompleted(event::FileDownloadCompleted {
                hash: *file_hash,
                resource: resource.clone(),
            }))
            .await;

        // If the file is already complete, we don't need to download any chunk
        if chunked_file.is_complete() {
            // Announce the file
            let self_announce =
                FudAnnounce { key: *file_hash, seeders: vec![self_node.clone().into()] };
            let _ = self.announce(file_hash, &self_announce, self.seeders_router.clone()).await;

            // Set resource status to `Seeding`
            let mut resources_write = self.resources.write().await;
            let resource = match resources_write.get_mut(file_hash) {
                Some(resource) => {
                    resource.status = ResourceStatus::Seeding;
                    resource.chunks_downloaded = chunked_file.len() as u64;
                    resource.clone()
                }
                None => return Ok(()), // Resource was removed, abort
            };
            drop(resources_write);

            // Send a DownloadCompleted event
            self.event_publisher
                .notify(FudEvent::DownloadCompleted(event::DownloadCompleted {
                    hash: *file_hash,
                    resource,
                }))
                .await;

            return Ok(());
        }

        // Find nodes close to the file hash if we didn't previously fetched them
        if closest_nodes.is_empty() {
            closest_nodes = self.lookup_nodes(file_hash).await.unwrap_or_default();
        }

        // Find seeders and remove ourselves from the result
        let seeders = self
            .fetch_seeders(&closest_nodes, file_hash)
            .await
            .iter()
            .filter(|seeder| seeder.node.id != self_node.id)
            .cloned()
            .collect();

        // List missing chunks
        let mut missing_chunks = HashSet::new();
        for (chunk, path) in chunked_file.iter() {
            if path.is_none() {
                missing_chunks.insert(*chunk);
            }
        }

        // Fetch missing chunks from seeders
        self.fetch_chunks(file_path, file_hash, &missing_chunks, &seeders).await?;

        // Get chunked file from geode
        let chunked_file = match self.geode.get(file_hash, file_path).await {
            Ok(v) => v,
            Err(e) => {
                error!(target: "fud::handle_get()", "{}", e);
                return Err(e);
            }
        };

        // We fetched all chunks, but the file is not complete
        // (some chunks were missing from all seeders)
        if !chunked_file.is_complete() {
            // Set resource status to `Incomplete`
            let mut resources_write = self.resources.write().await;
            let resource = match resources_write.get_mut(file_hash) {
                Some(resource) => {
                    resource.status = ResourceStatus::Incomplete;
                    resource.clone()
                }
                None => return Ok(()), // Resource was removed, abort
            };
            drop(resources_write);

            // Send a MissingChunks event
            self.event_publisher
                .notify(FudEvent::MissingChunks(event::MissingChunks {
                    hash: *file_hash,
                    resource,
                }))
                .await;
            return Ok(());
        }

        // Announce the file
        let self_announce =
            FudAnnounce { key: *file_hash, seeders: vec![self_node.clone().into()] };
        let _ = self.announce(file_hash, &self_announce, self.seeders_router.clone()).await;

        // Set resource status to `Seeding`
        let mut resources_write = self.resources.write().await;
        let resource = match resources_write.get_mut(file_hash) {
            Some(resource) => {
                resource.status = ResourceStatus::Seeding;
                resource.chunks_downloaded = chunked_file.len() as u64;
                resource.clone()
            }
            None => return Ok(()), // Resource was removed, abort
        };
        drop(resources_write);

        // Send a DownloadCompleted event
        self.event_publisher
            .notify(FudEvent::DownloadCompleted(event::DownloadCompleted {
                hash: *file_hash,
                resource,
            }))
            .await;

        Ok(())
    }
}
