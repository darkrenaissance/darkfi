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
    fs::{self, File, OpenOptions},
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
    geode::{hash_to_string, ChunkedStorage, FileSequence, Geode, MAX_CHUNK_SIZE},
    net::{ChannelPtr, P2pPtr},
    system::{PublisherPtr, StoppableTask},
    util::path::expand_path,
    Error, Result,
};

/// P2P protocols
pub mod proto;
use proto::{
    FudAnnounce, FudChunkReply, FudDirectoryReply, FudFileReply, FudFindNodesReply,
    FudFindNodesRequest, FudFindRequest, FudFindSeedersReply, FudFindSeedersRequest, FudNotFound,
    FudPingReply, FudPingRequest,
};

/// FudEvent
pub mod event;
use event::{
    ChunkDownloadCompleted, ChunkNotFound, FudEvent, MetadataDownloadCompleted, ResourceUpdated,
};

/// Resource definition
pub mod resource;
use resource::{Resource, ResourceStatus, ResourceType};

/// JSON-RPC related methods
pub mod rpc;

/// Background tasks
pub mod tasks;
use tasks::FetchReply;

/// Utils
pub mod util;
use util::{get_all_files, FileSelection};

const SLED_PATH_TREE: &[u8] = b"_fud_paths";
const SLED_SCRAP_TREE: &[u8] = b"_fud_scraps";

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

    /// Sled tree containing scraps which are chunks containing data the user
    /// did not want to save to files. They also contain data the user wanted
    /// otherwise we would not have downloaded the chunk at all.
    /// "chunk/scrap hash -> chunk content"
    scrap_tree: sled::Tree,

    get_tx: channel::Sender<(blake3::Hash, PathBuf, FileSelection)>,
    get_rx: channel::Receiver<(blake3::Hash, PathBuf, FileSelection)>,

    /// Currently active downloading tasks (running the `fud.fetch_resource()` method)
    fetch_tasks: Arc<RwLock<HashMap<blake3::Hash, Arc<StoppableTask>>>>,

    /// Used to send events to fud clients
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
        let channel = self.get_channel(node, None).await?;
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
        self.cleanup_channel(channel).await;

        Ok(())
    }

    async fn fetch_nodes(&self, node: &DhtNode, key: &blake3::Hash) -> Result<Vec<DhtNode>> {
        debug!(target: "fud::DhtHandler::fetch_nodes()", "Fetching nodes close to {} from node {}", hash_to_string(key), hash_to_string(&node.id));

        let channel = self.get_channel(node, None).await?;
        let msg_subsystem = channel.message_subsystem();
        msg_subsystem.add_dispatch::<FudFindNodesReply>().await;
        let msg_subscriber_nodes = channel.subscribe_msg::<FudFindNodesReply>().await.unwrap();

        let request = FudFindNodesRequest { key: *key };
        channel.send(&request).await?;

        let reply = msg_subscriber_nodes.receive_with_timeout(self.dht().settings.timeout).await?;

        msg_subscriber_nodes.unsubscribe().await;
        self.cleanup_channel(channel).await;

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
        sled_db: &sled::Db,
        event_publisher: PublisherPtr<FudEvent>,
    ) -> Result<Self> {
        let (get_tx, get_rx) = smol::channel::unbounded();

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
            path_tree: sled_db.open_tree(SLED_PATH_TREE)?,
            scrap_tree: sled_db.open_tree(SLED_SCRAP_TREE)?,
            resources: Arc::new(RwLock::new(HashMap::new())),
            get_tx,
            get_rx,
            fetch_tasks: Arc::new(RwLock::new(HashMap::new())),
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
                    rtype: ResourceType::Unknown,
                    path,
                    status: ResourceStatus::Incomplete,
                    chunks_total: 0,
                    chunks_downloaded: 0,
                    chunks_target: 0,
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
    /// Takes an optional list of resource hashes.
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
                   chunked: Option<&ChunkedStorage>| {
                resource.status = status;
                resource.chunks_total = match chunked {
                    Some(chunked_file) => chunked_file.len() as u64,
                    None => 0,
                };
                resource.chunks_downloaded = match chunked {
                    Some(chunked_file) => chunked_file.local_chunks() as u64,
                    None => 0,
                };

                if let Some(chunked) = chunked {
                    resource.rtype = match chunked.is_dir() {
                        false => ResourceType::File,
                        true => ResourceType::Directory,
                    };
                }

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
            let mut chunked = match self.geode.get(&resource.hash, &resource_path).await {
                Ok(v) => v,
                Err(_) => {
                    update_resource(&mut resource, ResourceStatus::Incomplete, None).await;
                    continue;
                }
            };
            if let Err(e) = self.verify_chunks(&mut chunked).await {
                error!(target: "fud::verify_resources()", "Error while verifying chunks of {}: {e}", hash_to_string(&resource.hash));
                update_resource(&mut resource, ResourceStatus::Incomplete, None).await;
                continue;
            }
            if !chunked.is_complete() {
                update_resource(&mut resource, ResourceStatus::Incomplete, Some(&chunked)).await;
                continue;
            }

            update_resource(&mut resource, ResourceStatus::Seeding, Some(&chunked)).await;
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
            let channel = match self.get_channel(node, None).await {
                Ok(channel) => channel,
                Err(e) => {
                    warn!(target: "fud::fetch_seeders()", "Could not get a channel for node {}: {e}", hash_to_string(&node.id));
                    continue;
                }
            };
            let msg_subsystem = channel.message_subsystem();
            msg_subsystem.add_dispatch::<FudFindSeedersReply>().await;

            let msg_subscriber = match channel.subscribe_msg::<FudFindSeedersReply>().await {
                Ok(msg_subscriber) => msg_subscriber,
                Err(e) => {
                    warn!(target: "fud::fetch_seeders()", "Error subscribing to msg: {e}");
                    self.cleanup_channel(channel).await;
                    continue;
                }
            };

            let send_res = channel.send(&FudFindSeedersRequest { key: *key }).await;
            if let Err(e) = send_res {
                warn!(target: "fud::fetch_seeders()", "Error while sending FudFindSeedersRequest: {e}");
                msg_subscriber.unsubscribe().await;
                self.cleanup_channel(channel).await;
                continue;
            }

            let reply = match msg_subscriber.receive_with_timeout(self.dht().settings.timeout).await
            {
                Ok(reply) => reply,
                Err(e) => {
                    warn!(target: "fud::fetch_seeders()", "Error waiting for reply: {e}");
                    msg_subscriber.unsubscribe().await;
                    self.cleanup_channel(channel).await;
                    continue;
                }
            };

            msg_subscriber.unsubscribe().await;
            self.cleanup_channel(channel).await;

            seeders.extend(reply.seeders.clone());
        }

        info!(target: "fud::fetch_seeders()", "Found {} seeders for {}", seeders.len(), hash_to_string(key));
        seeders
    }

    /// Fetch `chunks` for `chunked` (file or directory) from `seeders`.
    async fn fetch_chunks(
        &self,
        hash: &blake3::Hash,
        chunked: &mut ChunkedStorage,
        seeders: &HashSet<DhtRouterItem>,
        chunks: &HashSet<blake3::Hash>,
    ) -> Result<()> {
        let mut remaining_chunks = chunks.clone();
        let mut shuffled_seeders = {
            let mut vec: Vec<_> = seeders.iter().cloned().collect();
            vec.shuffle(&mut OsRng);
            vec
        };

        while let Some(seeder) = shuffled_seeders.pop() {
            let channel = match self.get_channel(&seeder.node, Some(*hash)).await {
                Ok(channel) => channel,
                Err(e) => {
                    warn!(target: "fud::fetch_chunks()", "Could not get a channel for node {}: {e}", hash_to_string(&seeder.node.id));
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
                let mut chunk = None;
                if let Some(random_chunk) = chunks_to_query.iter().choose(&mut OsRng) {
                    chunk = Some(*random_chunk);
                }

                if chunk.is_none() {
                    // No more chunks to request from this seeder
                    break; // Switch to another seeder
                }
                let chunk_hash = chunk.unwrap();
                chunks_to_query.remove(&chunk_hash);

                let send_res =
                    channel.send(&FudFindRequest { info: Some(*hash), key: chunk_hash }).await;
                if let Err(e) = send_res {
                    warn!(target: "fud::fetch_chunks()", "Error while sending FudFindRequest: {e}");
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
                            warn!(target: "fud::fetch_chunks()", "Error waiting for chunk reply: {e}");
                            break; // Switch to another seeder
                        }
                        let reply = chunk_reply.unwrap();

                        match self.geode.write_chunk(chunked, &reply.chunk).await {
                            Ok((inserted_hash, bytes_written)) => {
                                if inserted_hash != chunk_hash {
                                    warn!(target: "fud::fetch_chunks()", "Received chunk does not match requested chunk");
                                    msg_subscriber_chunk.unsubscribe().await;
                                    msg_subscriber_notfound.unsubscribe().await;
                                    continue; // Skip to next chunk, will retry this chunk later
                                }

                                info!(target: "fud::fetch_chunks()", "Received chunk {} from seeder {}", hash_to_string(&chunk_hash), hash_to_string(&seeder.node.id));

                                // If we did not write the whole chunk to the filesystem,
                                // save the chunk in the scraps.
                                if bytes_written < reply.chunk.len() {
                                    info!(target: "fud::fetch_chunks()", "Saving chunk {} as a scrap", hash_to_string(&chunk_hash));
                                    if let Err(e) = self.scrap_tree.insert(chunk_hash.as_bytes(), reply.chunk.clone()) {
                                        error!(target: "fud::fetch_chunks()", "Failed to save chunk {} as a scrap: {e}", hash_to_string(&chunk_hash))
                                    }
                                }

                                // Update resource `chunks_downloaded`
                                let mut resources_write = self.resources.write().await;
                                let resource = match resources_write.get_mut(hash) {
                                    Some(resource) => {
                                        resource.status = ResourceStatus::Downloading;
                                        resource.chunks_downloaded += 1;
                                        resource.clone()
                                    }
                                    None => return Ok(()) // Resource was removed, abort
                                };
                                drop(resources_write);

                                self.event_publisher
                                    .notify(FudEvent::ChunkDownloadCompleted(ChunkDownloadCompleted {
                                        hash: *hash,
                                        chunk_hash,
                                        resource,
                                    }))
                                    .await;
                                remaining_chunks.remove(&chunk_hash);
                            }
                            Err(e) => {
                                error!(target: "fud::fetch_chunks()", "Failed inserting chunk {} to Geode: {e}", hash_to_string(&chunk_hash));
                            }
                        };
                    }
                    notfound_reply = notfound_recv => {
                        if let Err(e) = notfound_reply {
                            warn!(target: "fud::fetch_chunks()", "Error waiting for NOTFOUND reply: {e}");
                            msg_subscriber_chunk.unsubscribe().await;
                            msg_subscriber_notfound.unsubscribe().await;
                            break; // Switch to another seeder
                        }
                        info!(target: "fud::fetch_chunks()", "Received NOTFOUND {} from seeder {}", hash_to_string(&chunk_hash), hash_to_string(&seeder.node.id));
                        self.event_publisher
                            .notify(FudEvent::ChunkNotFound(ChunkNotFound {
                                hash: *hash,
                                chunk_hash,
                            }))
                        .await;
                    }
                };

                msg_subscriber_chunk.unsubscribe().await;
                msg_subscriber_notfound.unsubscribe().await;
            }

            self.cleanup_channel(channel).await;

            // Stop when there are no missing chunks
            if remaining_chunks.is_empty() {
                break;
            }
        }

        Ok(())
    }

    /// Fetch a single resource metadata from `nodes`.
    /// If the resource is a file smaller than a single chunk then seeder can send the
    /// chunk directly, and we will create the file from it on path `path`.
    /// 1. Request seeders from those nodes
    /// 2. Request the metadata from the seeders
    /// 3. Insert metadata to geode using the reply
    pub async fn fetch_metadata(
        &self,
        hash: &blake3::Hash,
        nodes: &Vec<DhtNode>,
        path: &Path,
    ) -> Result<()> {
        let mut queried_seeders: HashSet<blake3::Hash> = HashSet::new();
        let mut result: Option<FetchReply> = None;

        for node in nodes {
            // 1. Request list of seeders
            let channel = match self.get_channel(node, Some(*hash)).await {
                Ok(channel) => channel,
                Err(e) => {
                    warn!(target: "fud::fetch_metadata()", "Could not get a channel for node {}: {e}", hash_to_string(&node.id));
                    continue;
                }
            };
            let msg_subsystem = channel.message_subsystem();
            msg_subsystem.add_dispatch::<FudFindSeedersReply>().await;

            let msg_subscriber = match channel.subscribe_msg::<FudFindSeedersReply>().await {
                Ok(msg_subscriber) => msg_subscriber,
                Err(e) => {
                    warn!(target: "fud::fetch_metadata()", "Error subscribing to msg: {e}");
                    continue;
                }
            };

            let send_res = channel.send(&FudFindSeedersRequest { key: *hash }).await;
            if let Err(e) = send_res {
                warn!(target: "fud::fetch_metadata()", "Error while sending FudFindSeedersRequest: {e}");
                msg_subscriber.unsubscribe().await;
                self.cleanup_channel(channel).await;
                continue;
            }

            let reply = match msg_subscriber.receive_with_timeout(self.dht().settings.timeout).await
            {
                Ok(reply) => reply,
                Err(e) => {
                    warn!(target: "fud::fetch_metadata()", "Error waiting for reply: {e}");
                    msg_subscriber.unsubscribe().await;
                    self.cleanup_channel(channel).await;
                    continue;
                }
            };

            let mut seeders = reply.seeders.clone();
            info!(target: "fud::fetch_metadata()", "Found {} seeders for {} (from {})", seeders.len(), hash_to_string(hash), hash_to_string(&node.id));

            msg_subscriber.unsubscribe().await;
            self.cleanup_channel(channel).await;

            // 2. Request the file/chunk from the seeders
            while let Some(seeder) = seeders.pop() {
                // Only query a seeder once
                if queried_seeders.iter().any(|s| *s == seeder.node.id) {
                    continue;
                }
                queried_seeders.insert(seeder.node.id);

                if let Ok(channel) = self.get_channel(&seeder.node, Some(*hash)).await {
                    let msg_subsystem = channel.message_subsystem();
                    msg_subsystem.add_dispatch::<FudChunkReply>().await;
                    msg_subsystem.add_dispatch::<FudFileReply>().await;
                    msg_subsystem.add_dispatch::<FudDirectoryReply>().await;
                    msg_subsystem.add_dispatch::<FudNotFound>().await;
                    let msg_subscriber_chunk =
                        channel.subscribe_msg::<FudChunkReply>().await.unwrap();
                    let msg_subscriber_file =
                        channel.subscribe_msg::<FudFileReply>().await.unwrap();
                    let msg_subscriber_dir =
                        channel.subscribe_msg::<FudDirectoryReply>().await.unwrap();
                    let msg_subscriber_notfound =
                        channel.subscribe_msg::<FudNotFound>().await.unwrap();

                    let send_res = channel.send(&FudFindRequest { info: None, key: *hash }).await;
                    if let Err(e) = send_res {
                        warn!(target: "fud::fetch_metadata()", "Error while sending FudFindRequest: {e}");
                        msg_subscriber_chunk.unsubscribe().await;
                        msg_subscriber_file.unsubscribe().await;
                        msg_subscriber_dir.unsubscribe().await;
                        msg_subscriber_notfound.unsubscribe().await;
                        self.cleanup_channel(channel).await;
                        continue;
                    }

                    let chunk_recv =
                        msg_subscriber_chunk.receive_with_timeout(self.chunk_timeout).fuse();
                    let file_recv =
                        msg_subscriber_file.receive_with_timeout(self.chunk_timeout).fuse();
                    let dir_recv =
                        msg_subscriber_dir.receive_with_timeout(self.chunk_timeout).fuse();
                    let notfound_recv =
                        msg_subscriber_notfound.receive_with_timeout(self.chunk_timeout).fuse();

                    pin_mut!(chunk_recv, file_recv, dir_recv, notfound_recv);

                    let cleanup = async || {
                        msg_subscriber_chunk.unsubscribe().await;
                        msg_subscriber_file.unsubscribe().await;
                        msg_subscriber_dir.unsubscribe().await;
                        msg_subscriber_notfound.unsubscribe().await;
                        self.cleanup_channel(channel).await;
                    };

                    // Wait for a FudChunkReply, FudFileReply, FudDirectoryReply, or FudNotFound
                    select! {
                        // Received a chunk while requesting metadata, this is allowed to
                        // optimize fetching files smaller than a single chunk
                        chunk_reply = chunk_recv => {
                            cleanup().await;
                            if let Err(e) = chunk_reply {
                                warn!(target: "fud::fetch_metadata()", "Error waiting for chunk reply: {e}");
                                continue;
                            }
                            let reply = chunk_reply.unwrap();
                            let chunk_hash = blake3::hash(&reply.chunk);
                            // Check that this is the only chunk in the file
                            if !self.geode.verify_metadata(hash, &[chunk_hash], &[]) {
                                warn!(target: "fud::fetch_metadata()", "Received a chunk while fetching a file, the chunk did not match the file hash");
                                continue;
                            }
                            info!(target: "fud::fetch_metadata()", "Received chunk {} (for file {}) from seeder {}", hash_to_string(&chunk_hash), hash_to_string(hash), hash_to_string(&seeder.node.id));
                            result = Some(FetchReply::Chunk((*reply).clone()));
                            break;
                        }
                        file_reply = file_recv => {
                            cleanup().await;
                            if let Err(e) = file_reply {
                                warn!(target: "fud::fetch_metadata()", "Error waiting for file reply: {e}");
                                continue;
                            }
                            let reply = file_reply.unwrap();
                            if !self.geode.verify_metadata(hash, &reply.chunk_hashes, &[]) {
                                warn!(target: "fud::fetch_metadata()", "Received invalid file metadata");
                                continue;
                            }
                            info!(target: "fud::fetch_metadata()", "Received file {} from seeder {}", hash_to_string(hash), hash_to_string(&seeder.node.id));
                            result = Some(FetchReply::File((*reply).clone()));
                            break;
                        }
                        dir_reply = dir_recv => {
                            cleanup().await;
                            if let Err(e) = dir_reply {
                                warn!(target: "fud::fetch_metadata()", "Error waiting for directory reply: {e}");
                                continue;
                            }
                            let reply = dir_reply.unwrap();

                            // Convert all file paths from String to PathBuf
                            let files: Vec<_> = reply.files.clone().into_iter()
                                .map(|(path_str, size)| (PathBuf::from(path_str), size))
                                .collect();

                            if !self.geode.verify_metadata(hash, &reply.chunk_hashes, &files) {
                                warn!(target: "fud::fetch_metadata()", "Received invalid directory metadata");
                                continue;
                            }
                            info!(target: "fud::fetch_metadata()", "Received directory {} from seeder {}", hash_to_string(hash), hash_to_string(&seeder.node.id));
                            result = Some(FetchReply::Directory((*reply).clone()));
                            break;
                        }
                        notfound_reply = notfound_recv => {
                            cleanup().await;
                            if let Err(e) = notfound_reply {
                                warn!(target: "fud::fetch_metadata()", "Error waiting for NOTFOUND reply: {e}");
                                continue;
                            }
                            info!(target: "fud::fetch_metadata()", "Received NOTFOUND {} from seeder {}", hash_to_string(hash), hash_to_string(&seeder.node.id));
                        }
                    };
                }
            }

            if result.is_some() {
                break;
            }
        }

        // We did not find the resource
        if result.is_none() {
            return Err(Error::GeodeFileRouteNotFound)
        }

        // 3. Insert metadata to geode using the reply
        // At this point the reply content is already verified
        match result.unwrap() {
            FetchReply::Directory(FudDirectoryReply { files, chunk_hashes }) => {
                // Convert all file paths from String to PathBuf
                let mut files: Vec<_> = files
                    .into_iter()
                    .map(|(path_str, size)| (PathBuf::from(path_str), size))
                    .collect();

                self.geode.sort_files(&mut files);
                if let Err(e) = self.geode.insert_metadata(hash, &chunk_hashes, &files).await {
                    error!(target: "fud::fetch_metadata()", "Failed inserting directory {} to Geode: {e}", hash_to_string(hash));
                    return Err(e)
                }
            }
            FetchReply::File(FudFileReply { chunk_hashes }) => {
                if let Err(e) = self.geode.insert_metadata(hash, &chunk_hashes, &[]).await {
                    error!(target: "fud::fetch_metadata()", "Failed inserting file {} to Geode: {e}", hash_to_string(hash));
                    return Err(e)
                }
            }
            // Looked for a file but got a chunk: the entire file fits in a single chunk
            FetchReply::Chunk(FudChunkReply { chunk }) => {
                info!(target: "fud::fetch_metadata()", "File fits in a single chunk");
                let chunk_hash = blake3::hash(&chunk);
                let _ = self.geode.insert_metadata(hash, &[chunk_hash], &[]).await;
                let mut chunked_file = ChunkedStorage::new(
                    &[chunk_hash],
                    &[(path.to_path_buf(), chunk.len() as u64)],
                    false,
                );
                if let Err(e) = self.geode.write_chunk(&mut chunked_file, &chunk).await {
                    error!(target: "fud::fetch_metadata()", "Failed inserting chunk {} to Geode: {e}", hash_to_string(&chunk_hash));
                    return Err(e)
                };
            }
        };

        Ok(())
    }

    /// Start downloading a file or directory from the network to `path`.
    /// This creates a new task in `fetch_tasks` calling `fetch_resource()`.
    /// `files` is the list of files (relative paths) you want to download
    /// (if the resource is a directory), None means you want all files.
    pub async fn get(&self, hash: &blake3::Hash, path: &Path, files: FileSelection) -> Result<()> {
        let fetch_tasks = self.fetch_tasks.read().await;
        if fetch_tasks.contains_key(hash) {
            return Err(Error::Custom(format!(
                "Resource {} is already being downloaded",
                hash_to_string(hash)
            )))
        }
        drop(fetch_tasks);

        self.get_tx.send((*hash, path.to_path_buf(), files)).await?;

        Ok(())
    }

    /// Download a file or directory from the network to `path`.
    /// Called when `get()` creates a new fetch task.
    pub async fn fetch_resource(
        &self,
        hash: &blake3::Hash,
        path: &Path,
        files: &FileSelection,
    ) -> Result<()> {
        let self_node = self.dht().node().await;
        let mut closest_nodes = vec![];

        let hash_bytes = hash.as_bytes();
        let path_string = path.to_string_lossy().to_string();
        let path_bytes = path_string.as_bytes();

        // Make sure we don't already have another resource on that path
        for path_item in self.path_tree.iter() {
            let (key, value) = path_item?;
            if key != hash_bytes && value == path_bytes {
                let err_str = format!("There is already another resource on path {path_string}");
                self.event_publisher
                    .notify(FudEvent::DownloadError(event::DownloadError {
                        hash: *hash,
                        error: err_str.clone(),
                    }))
                    .await;
                return Err(Error::Custom(err_str))
            }
        }

        // Add path to the sled db
        self.path_tree.insert(hash_bytes, path_bytes)?;

        // Add resource to `self.resources`
        let resource = Resource {
            hash: *hash,
            rtype: ResourceType::Unknown,
            path: path.to_path_buf(),
            status: ResourceStatus::Discovering,
            chunks_total: 0,
            chunks_downloaded: 0,
            chunks_target: 0,
        };
        let mut resources_write = self.resources.write().await;
        resources_write.insert(*hash, resource.clone());
        drop(resources_write);

        // Send a DownloadStarted event
        self.event_publisher
            .notify(FudEvent::DownloadStarted(event::DownloadStarted { hash: *hash, resource }))
            .await;

        // Try to get the chunked file or directory from geode
        let mut chunked = match self.geode.get(hash, path).await {
            // We already know the metadata
            Ok(v) => v,
            // The metadata in geode is invalid or corrupted
            Err(Error::GeodeNeedsGc) => todo!(),
            // If we could not find the metadata in geode, get it from the network
            Err(Error::GeodeFileNotFound) => {
                // Find nodes close to the file hash
                info!(target: "fud::get()", "Requested metadata {} not found in Geode, triggering fetch", hash_to_string(hash));
                closest_nodes = self.lookup_nodes(hash).await.unwrap_or_default();

                // Fetch file or directory metadata
                match self.fetch_metadata(hash, &closest_nodes, path).await {
                    // The file metadata was found and inserted into geode
                    Ok(()) => self.geode.get(hash, path).await.unwrap(),
                    // We could not find the metadata, or any other error occured
                    Err(e) => {
                        // Set resource status to `Incomplete` and send FudEvent::FileNotFound
                        let mut resources_write = self.resources.write().await;
                        if let Some(resource) = resources_write.get_mut(hash) {
                            resource.status = ResourceStatus::Incomplete;

                            self.event_publisher
                                .notify(FudEvent::MetadataNotFound(event::MetadataNotFound {
                                    hash: *hash,
                                    resource: resource.clone(),
                                }))
                                .await;
                        }
                        drop(resources_write);
                        return Err(e);
                    }
                }
            }

            Err(e) => {
                error!(target: "fud::handle_get()", "{e}");
                return Err(e);
            }
        };

        // Get a list of all file paths
        let files_to_create: Vec<PathBuf> = match files {
            FileSelection::Set(files) => files
                .iter()
                .map(|file| path.join(file))
                .filter(|abs| chunked.get_files().iter().any(|(f, _)| f == abs))
                .collect(),
            FileSelection::All => chunked.get_files().iter().map(|(f, _)| f.clone()).collect(),
        };
        // Create all files (and all necessary directories)
        for file_path in files_to_create.iter() {
            if !file_path.exists() {
                if let Some(dir) = file_path.parent() {
                    fs::create_dir_all(dir).await?;
                }
                File::create(&file_path).await?;
            }
        }

        // Set resource status to `Verifying` and send FudEvent::MetadataDownloadCompleted
        let mut resources_write = self.resources.write().await;
        if let Some(resource) = resources_write.get_mut(hash) {
            resource.status = ResourceStatus::Verifying;
            resource.chunks_total = chunked.len() as u64;
            resource.rtype = match chunked.is_dir() {
                false => ResourceType::File,
                true => ResourceType::Directory,
            };

            self.event_publisher
                .notify(FudEvent::MetadataDownloadCompleted(MetadataDownloadCompleted {
                    hash: *hash,
                    resource: resource.clone(),
                }))
                .await;
        }
        drop(resources_write);

        // Mark locally available chunks as such
        let scraps = self.verify_chunks(&mut chunked).await;
        if let Err(e) = scraps {
            error!(target: "fud::get()", "Error while verifying chunks: {e}");
            return Err(e);
        }
        let scraps = scraps.unwrap();

        // Write all scraps to make sure the data on the filesystem is correct
        if !scraps.is_empty() {
            info!(target: "fud::get()", "Writing {} scraps...", scraps.len());
        }
        for (scrap_hash, scrap) in scraps {
            let len = scrap.len();
            let (_, bytes_written) = self.geode.write_chunk(&mut chunked, scrap).await?;

            // If the whole scrap was written, we can remove it from sled
            if bytes_written == len {
                self.scrap_tree.remove(scrap_hash.as_bytes())?;
            }
        }

        // If `chunked` is a file that is bigger than the all its chunks,
        // truncate the file to the chunks.
        // This fixes two edge-cases: a file that exactly ends at the end of
        // a chunk, and a file with no chunk.
        if !chunked.is_dir() {
            let fs_metadata = fs::metadata(&path).await?;
            if fs_metadata.len() > (chunked.len() * MAX_CHUNK_SIZE) as u64 {
                if let Ok(file) = OpenOptions::new().write(true).create(true).open(path).await {
                    let _ = file.set_len((chunked.len() * MAX_CHUNK_SIZE) as u64).await;
                }
            }
        }

        // Set of all chunks we need locally (including the ones we already have)
        let chunks: HashSet<(blake3::Hash, bool)> = match files {
            FileSelection::Set(files) => {
                let mut chunks = HashSet::new();
                for file in files {
                    chunks.extend(chunked.get_chunks_of_file(&path.join(file)));
                }
                chunks
            }
            FileSelection::All => chunked.iter().cloned().collect(),
        };
        let chunk_hashes: HashSet<_> = chunks.iter().map(|(hash, _)| hash).collect();

        // Set of the chunks we need to download
        let missing_chunks: HashSet<blake3::Hash> = {
            let mut missing_chunks = HashSet::new();
            for (chunk, available) in chunks.clone() {
                if !available {
                    missing_chunks.insert(chunk);
                }
            }
            missing_chunks
        };

        // If we don't need to download any chunk
        if missing_chunks.is_empty() {
            // Set resource status to `Seeding` or `Incomplete`
            let mut resources_write = self.resources.write().await;
            let resource = match resources_write.get_mut(hash) {
                Some(resource) => {
                    resource.status = match chunked.is_complete() {
                        true => ResourceStatus::Seeding,
                        false => ResourceStatus::Incomplete,
                    };
                    resource.chunks_downloaded = chunks.len() as u64;
                    resource.chunks_target = chunks.len() as u64;
                    resource.clone()
                }
                None => return Ok(()), // Resource was removed, abort
            };
            drop(resources_write);

            // Announce the resource if we have all chunks
            if chunked.is_complete() {
                let self_announce =
                    FudAnnounce { key: *hash, seeders: vec![self_node.clone().into()] };
                let _ = self.announce(hash, &self_announce, self.seeders_router.clone()).await;
            }

            // Send a DownloadCompleted event
            self.event_publisher
                .notify(FudEvent::DownloadCompleted(event::DownloadCompleted {
                    hash: *hash,
                    resource,
                }))
                .await;

            return Ok(());
        }

        // Set resource status to `Downloading`
        let mut resources_write = self.resources.write().await;
        let resource = match resources_write.get_mut(hash) {
            Some(resource) => {
                resource.status = ResourceStatus::Downloading;
                resource.chunks_target = chunks.len() as u64;
                resource.chunks_downloaded = (chunks.len() - missing_chunks.len()) as u64;
                resource.clone()
            }
            None => return Ok(()), // Resource was removed, abort
        };
        drop(resources_write);

        // Send a MetadataDownloadCompleted event
        self.event_publisher
            .notify(FudEvent::MetadataDownloadCompleted(event::MetadataDownloadCompleted {
                hash: *hash,
                resource: resource.clone(),
            }))
            .await;

        // Find nodes close to the file hash if we didn't previously fetched them
        if closest_nodes.is_empty() {
            closest_nodes = self.lookup_nodes(hash).await.unwrap_or_default();
        }

        // Find seeders and remove ourselves from the result
        let seeders = self
            .fetch_seeders(&closest_nodes, hash)
            .await
            .iter()
            .filter(|seeder| seeder.node.id != self_node.id)
            .cloned()
            .collect();

        // Fetch missing chunks from seeders
        self.fetch_chunks(hash, &mut chunked, &seeders, &missing_chunks).await?;

        // Get chunked file from geode
        let mut chunked = match self.geode.get(hash, path).await {
            Ok(v) => v,
            Err(e) => {
                error!(target: "fud::handle_get()", "{e}");
                return Err(e);
            }
        };

        // Set resource status to `Verifying` and send FudEvent::ResourceUpdated
        let mut resources_write = self.resources.write().await;
        if let Some(resource) = resources_write.get_mut(hash) {
            resource.status = ResourceStatus::Verifying;

            self.event_publisher
                .notify(FudEvent::ResourceUpdated(ResourceUpdated {
                    hash: *hash,
                    resource: resource.clone(),
                }))
                .await;
        }
        drop(resources_write);

        // Verify all chunks
        self.verify_chunks(&mut chunked).await?;

        let is_complete = chunked
            .iter()
            .filter(|(hash, _)| chunk_hashes.contains(hash))
            .all(|(_, available)| *available);

        // We fetched all chunks, but the resource is not complete
        // (some chunks were missing from all seeders)
        if !is_complete {
            // Set resource status to `Incomplete`
            let mut resources_write = self.resources.write().await;
            let resource = match resources_write.get_mut(hash) {
                Some(resource) => {
                    resource.status = ResourceStatus::Incomplete;
                    resource.clone()
                }
                None => return Ok(()), // Resource was removed, abort
            };
            drop(resources_write);

            // Send a MissingChunks event
            self.event_publisher
                .notify(FudEvent::MissingChunks(event::MissingChunks { hash: *hash, resource }))
                .await;
            return Ok(());
        }

        // Set resource status to `Seeding` or `Incomplete`
        let mut resources_write = self.resources.write().await;
        let resource = match resources_write.get_mut(hash) {
            Some(resource) => {
                resource.status = match chunked.is_complete() {
                    true => ResourceStatus::Seeding,
                    false => ResourceStatus::Incomplete,
                };
                resource.chunks_downloaded = chunks.len() as u64;
                resource.clone()
            }
            None => return Ok(()), // Resource was removed, abort
        };
        drop(resources_write);

        // Announce the resource if we have all chunks
        if chunked.is_complete() {
            let self_announce = FudAnnounce { key: *hash, seeders: vec![self_node.clone().into()] };
            let _ = self.announce(hash, &self_announce, self.seeders_router.clone()).await;
        }

        // Send a DownloadCompleted event
        self.event_publisher
            .notify(FudEvent::DownloadCompleted(event::DownloadCompleted { hash: *hash, resource }))
            .await;

        Ok(())
    }

    /// Iterate over chunks and find which chunks are available locally,
    /// either in the filesystem (using geode::verify_chunks()) or in scraps.
    /// Return the scraps in a HashMap.
    pub async fn verify_chunks(
        &self,
        chunked: &mut ChunkedStorage,
    ) -> Result<HashMap<blake3::Hash, Vec<u8>>> {
        self.geode.verify_chunks(chunked).await?;

        // Look for the chunks that are not on the filesystem in the scraps
        let chunks = chunked.get_chunks().clone();
        let missing_on_fs: Vec<_> =
            chunks.iter().enumerate().filter(|(_, (_, available))| !available).collect();
        let mut scraps = HashMap::new();
        for (chunk_index, (chunk_hash, _)) in missing_on_fs {
            let chunk = self.scrap_tree.get(chunk_hash.as_bytes())?;
            if chunk.is_none() {
                continue;
            }

            // Verify the scrap we found
            let chunk = chunk.unwrap();
            if self.geode.verify_chunk(chunk_hash, &chunk) {
                // Mark it as available if it's valid
                chunked.get_chunk_mut(chunk_index).1 = true;
                scraps.insert(*chunk_hash, chunk.to_vec());
            }
        }

        Ok(scraps)
    }

    /// Add a resource from the file system.
    pub async fn put(&self, path: &PathBuf) -> Result<blake3::Hash> {
        let self_node = self.dht.node().await;

        if self_node.addresses.is_empty() {
            return Err(Error::Custom(
                "Cannot put file, you don't have any external address".to_string(),
            ))
        }

        let metadata = fs::metadata(path).await?;

        // Get the list of files and the resource type (file or directory)
        let (files, resource_type) = if metadata.is_file() {
            (vec![(path.clone(), metadata.len())], ResourceType::File)
        } else if metadata.is_dir() {
            let mut files = get_all_files(path).await?;
            self.geode.sort_files(&mut files);
            (files, ResourceType::Directory)
        } else {
            return Err(Error::Custom(format!("{} is not a valid path", path.to_string_lossy())))
        };

        // Read the file or directory and create the chunks
        let stream = FileSequence::new(&files, false);
        let (mut hasher, chunk_hashes) = self.geode.chunk_stream(stream).await?;

        // Get the relative file paths included in the metadata and hash of directories
        let relative_files = if let ResourceType::Directory = resource_type {
            // [(absolute file path, file size)] -> [(relative file path, file size)]
            let relative_files = files
                .into_iter()
                .map(|(file_path, size)| match file_path.strip_prefix(path) {
                    Ok(rel_path) => Ok((rel_path.to_path_buf(), size)),
                    Err(_) => Err(Error::Custom("Invalid file path".to_string())),
                })
                .collect::<Result<Vec<_>>>()?;

            // Add the files metadata to the hasher to complete the resource hash
            self.geode.hash_files_metadata(&mut hasher, &relative_files);

            relative_files
        } else {
            vec![]
        };

        // Finalize the resource hash
        let hash = hasher.finalize();

        // Create the metadata file in geode
        if let Err(e) = self.geode.insert_metadata(&hash, &chunk_hashes, &relative_files).await {
            error!(target: "fud::put()", "Failed inserting {path:?} to geode: {e}");
            return Err(e)
        }

        // Add path to the sled db
        if let Err(e) =
            self.path_tree.insert(hash.as_bytes(), path.to_string_lossy().to_string().as_bytes())
        {
            error!(target: "fud::put()", "Failed inserting new resource into sled: {e}");
            return Err(e.into())
        }

        // Add resource
        let mut resources_write = self.resources.write().await;
        resources_write.insert(
            hash,
            Resource {
                hash,
                rtype: resource_type,
                path: path.to_path_buf(),
                status: ResourceStatus::Seeding,
                chunks_total: chunk_hashes.len() as u64,
                chunks_downloaded: chunk_hashes.len() as u64,
                chunks_target: chunk_hashes.len() as u64,
            },
        );
        drop(resources_write);

        // Announce the new resource
        let fud_announce = FudAnnounce { key: hash, seeders: vec![self_node.into()] };
        let _ = self.announce(&hash, &fud_announce, self.seeders_router.clone()).await;

        Ok(hash)
    }

    /// Removes:
    /// - a resource
    /// - its metadata in geode
    /// - its path in the sled path tree
    /// - and any related scrap in the sled scrap tree,
    ///
    /// then sends a `ResourceRemoved` fud event.
    pub async fn remove(&self, hash: &blake3::Hash) {
        // Remove the resource
        let mut resources_write = self.resources.write().await;
        resources_write.remove(hash);
        drop(resources_write);

        // Remove the scraps in sled
        if let Ok(Some(path)) = self.hash_to_path(hash) {
            let chunked = self.geode.get(hash, &path).await;

            if let Ok(chunked) = chunked {
                for (chunk_hash, _) in chunked.iter() {
                    let _ = self.scrap_tree.remove(chunk_hash.as_bytes());
                }
            }
        }

        // Remove the metadata in geode
        let hash_str = hash_to_string(hash);
        let _ = fs::remove_file(self.geode.files_path.join(&hash_str)).await;
        let _ = fs::remove_file(self.geode.dirs_path.join(&hash_str)).await;

        // Remove the path in sled
        let _ = self.path_tree.remove(hash.as_bytes());

        // Send a `ResourceRemoved` event
        self.event_publisher
            .notify(FudEvent::ResourceRemoved(event::ResourceRemoved { hash: *hash }))
            .await;
    }

    /// Stop all tasks in `fetch_tasks`.
    pub async fn stop(&self) {
        // Create a clone of fetch_tasks because `task.stop()` needs a write lock
        let fetch_tasks = self.fetch_tasks.read().await;
        let cloned_fetch_tasks: HashMap<blake3::Hash, Arc<StoppableTask>> =
            fetch_tasks.iter().map(|(key, value)| (*key, value.clone())).collect();
        drop(fetch_tasks);

        // Stop all tasks
        for task in cloned_fetch_tasks.values() {
            task.stop().await;
        }
    }
}
