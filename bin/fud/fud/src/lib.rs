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
    collections::{HashMap, HashSet},
    io::ErrorKind,
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

use async_trait::async_trait;
use futures::{future::FutureExt, pin_mut, select};
use log::{debug, error, info, warn};
use num_bigint::BigUint;
use rand::{prelude::IteratorRandom, rngs::OsRng, seq::SliceRandom, Rng};
use sled_overlay::sled;
use smol::{
    channel,
    fs::{self, File, OpenOptions},
    lock::RwLock,
};
use url::Url;

use darkfi::{
    dht::{
        impl_dht_node_defaults, Dht, DhtHandler, DhtNode, DhtRouterItem, DhtRouterPtr, DhtSettings,
    },
    geode::{hash_to_string, ChunkedStorage, FileSequence, Geode, MAX_CHUNK_SIZE},
    net::{ChannelPtr, P2pPtr},
    system::{ExecutorPtr, PublisherPtr, StoppableTask},
    util::path::expand_path,
    Error, Result,
};
use darkfi_sdk::crypto::{schnorr::SchnorrPublic, SecretKey};
use darkfi_serial::{deserialize_async, serialize_async, SerialDecodable, SerialEncodable};

/// P2P protocols
pub mod proto;
use proto::{
    FudAnnounce, FudChunkReply, FudDirectoryReply, FudFileReply, FudFindNodesReply,
    FudFindNodesRequest, FudFindRequest, FudFindSeedersReply, FudFindSeedersRequest, FudNotFound,
    FudPingReply, FudPingRequest,
};

/// FudEvent
pub mod event;
use event::{notify_event, FudEvent};

/// Resource definition
pub mod resource;
use resource::{Resource, ResourceStatus, ResourceType};

/// Scrap definition
pub mod scrap;
use scrap::Scrap;

/// JSON-RPC related methods
pub mod rpc;

/// Background tasks
pub mod tasks;
use tasks::FetchReply;

/// Bitcoin
pub mod bitcoin;

/// PoW
pub mod pow;
use pow::{FudPow, VerifiableNodeData};

/// Equi-X
pub mod equix;

/// Settings and args
pub mod settings;
use settings::Args;

/// Utils
pub mod util;
use util::{get_all_files, FileSelection};

const SLED_PATH_TREE: &[u8] = b"_fud_paths";
const SLED_FILE_SELECTION_TREE: &[u8] = b"_fud_file_selections";
const SLED_SCRAP_TREE: &[u8] = b"_fud_scraps";

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct FudNode {
    data: VerifiableNodeData,
    addresses: Vec<Url>,
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

pub struct Fud {
    /// Our own [`VerifiableNodeData`]
    pub node_data: Arc<RwLock<VerifiableNodeData>>,

    /// Our secret key (the public key is in `node_data`)
    pub secret_key: Arc<RwLock<SecretKey>>,

    /// Key -> Seeders
    pub seeders_router: DhtRouterPtr<FudNode>,

    /// Pointer to the P2P network instance
    p2p: P2pPtr,

    /// The Geode instance
    geode: Geode,

    /// Default download directory
    downloads_path: PathBuf,

    /// Chunk transfer timeout in seconds
    chunk_timeout: u64,

    /// The [`FudPow`] instance
    pub pow: Arc<RwLock<FudPow>>,

    /// The DHT instance
    dht: Arc<Dht<FudNode>>,

    /// Resources (current status of all downloads/seeds)
    resources: Arc<RwLock<HashMap<blake3::Hash, Resource>>>,

    /// Sled tree containing "resource hash -> path on the filesystem"
    path_tree: sled::Tree,

    /// Sled tree containing "resource hash -> file selection". If the file
    /// selection is all files of the resource (or if the resource is not a
    /// directory), the resource does not store its file selection in the tree.
    file_selection_tree: sled::Tree,

    /// Sled tree containing scraps which are chunks containing data the user
    /// did not want to save to files. They also contain data the user wanted
    /// otherwise we would not have downloaded the chunk at all.
    /// "chunk/scrap hash -> chunk content"
    scrap_tree: sled::Tree,

    get_tx: channel::Sender<(blake3::Hash, PathBuf, FileSelection)>,
    get_rx: channel::Receiver<(blake3::Hash, PathBuf, FileSelection)>,

    put_tx: channel::Sender<PathBuf>,
    put_rx: channel::Receiver<PathBuf>,

    /// Currently active downloading tasks (running the `fud.fetch_resource()` method)
    fetch_tasks: Arc<RwLock<HashMap<blake3::Hash, Arc<StoppableTask>>>>,

    /// Currently active put tasks (running the `fud.insert_resource()` method)
    put_tasks: Arc<RwLock<HashMap<PathBuf, Arc<StoppableTask>>>>,

    /// Used to send events to fud clients
    event_publisher: PublisherPtr<FudEvent>,
}

#[async_trait]
impl DhtHandler<FudNode> for Fud {
    fn dht(&self) -> Arc<Dht<FudNode>> {
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
        let reply = msg_subscriber.receive_with_timeout(self.dht().settings.timeout).await?;
        msg_subscriber.unsubscribe().await;

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
        if !self.dht().is_bootstrapped().await {
            let _ = self.init().await;
        }

        // Send keys that are closer to this node than we are
        let self_id = self.node_data.read().await.id();
        let channel = self.get_channel(node, None).await?;
        for (key, seeders) in self.seeders_router.read().await.iter() {
            let node_distance = BigUint::from_bytes_be(&self.dht().distance(key, &node.id()));
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

    async fn fetch_nodes(&self, node: &FudNode, key: &blake3::Hash) -> Result<Vec<FudNode>> {
        debug!(target: "fud::DhtHandler::fetch_nodes()", "Fetching nodes close to {} from node {}", hash_to_string(key), hash_to_string(&node.id()));

        let channel = self.get_channel(node, None).await?;
        let msg_subsystem = channel.message_subsystem();
        msg_subsystem.add_dispatch::<FudFindNodesReply>().await;
        let msg_subscriber_nodes = channel.subscribe_msg::<FudFindNodesReply>().await.unwrap();

        let request = FudFindNodesRequest { key: *key };
        channel.send(&request).await?;

        let reply = msg_subscriber_nodes.receive_with_timeout(self.dht().settings.timeout).await;

        msg_subscriber_nodes.unsubscribe().await;
        self.cleanup_channel(channel).await;

        Ok(reply?.nodes.clone())
    }
}

impl Fud {
    pub async fn new(
        settings: Args,
        p2p: P2pPtr,
        sled_db: &sled::Db,
        event_publisher: PublisherPtr<FudEvent>,
        executor: ExecutorPtr,
    ) -> Result<Self> {
        let basedir = expand_path(&settings.base_dir)?;
        let downloads_path = match settings.downloads_path {
            Some(downloads_path) => expand_path(&downloads_path)?,
            None => basedir.join("downloads"),
        };

        // Run the PoW and generate a `VerifiableNodeData`
        let mut pow = FudPow::new(settings.pow.into(), executor.clone());
        pow.bitcoin_hash_cache.update().await?; // Fetch BTC block hashes
        let (node_data, secret_key) = pow.generate_node().await?;
        info!(target: "fud", "Your node ID: {}", hash_to_string(&node_data.id()));

        // Geode
        info!("Instantiating Geode instance");
        let geode = Geode::new(&basedir).await?;

        // DHT
        let dht_settings: DhtSettings = settings.dht.into();
        let dht: Arc<Dht<FudNode>> =
            Arc::new(Dht::<FudNode>::new(&dht_settings, p2p.clone(), executor.clone()).await);

        let (get_tx, get_rx) = smol::channel::unbounded();
        let (put_tx, put_rx) = smol::channel::unbounded();
        let fud = Self {
            node_data: Arc::new(RwLock::new(node_data)),
            secret_key: Arc::new(RwLock::new(secret_key)),
            seeders_router: Arc::new(RwLock::new(HashMap::new())),
            p2p,
            geode,
            downloads_path,
            chunk_timeout: settings.chunk_timeout,
            pow: Arc::new(RwLock::new(pow)),
            dht,
            path_tree: sled_db.open_tree(SLED_PATH_TREE)?,
            file_selection_tree: sled_db.open_tree(SLED_FILE_SELECTION_TREE)?,
            scrap_tree: sled_db.open_tree(SLED_SCRAP_TREE)?,
            resources: Arc::new(RwLock::new(HashMap::new())),
            get_tx,
            get_rx,
            put_tx,
            put_rx,
            fetch_tasks: Arc::new(RwLock::new(HashMap::new())),
            put_tasks: Arc::new(RwLock::new(HashMap::new())),
            event_publisher,
        };

        Ok(fud)
    }

    /// Bootstrap the DHT, verify our resources, add ourselves to
    /// `seeders_router` for the resources we already have, announce our files.
    async fn init(&self) -> Result<()> {
        info!(target: "fud::init()", "Bootstrapping the DHT...");
        self.bootstrap().await;

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

            // Get the file selection from sled, fallback on FileSelection::All
            let mut file_selection = FileSelection::All;
            if let Ok(Some(fs)) = self.file_selection_tree.get(hash.as_bytes()) {
                if let Ok(path_list) = deserialize_async::<Vec<Vec<u8>>>(&fs).await {
                    file_selection = FileSelection::Set(
                        path_list
                            .into_iter()
                            .filter_map(|bytes| {
                                std::str::from_utf8(&bytes)
                                    .ok()
                                    .and_then(|path_str| expand_path(path_str).ok())
                            })
                            .collect(),
                    );
                }
            }

            // Add resource
            resources_write.insert(
                hash,
                Resource::new(
                    hash,
                    ResourceType::Unknown,
                    &path,
                    ResourceStatus::Incomplete,
                    file_selection,
                ),
            );
        }
        drop(resources_write);

        info!(target: "fud::init()", "Verifying resources...");
        let resources = self.verify_resources(None).await?;

        let self_node = self.node().await;

        // Stop here if we have no external address
        if self_node.addresses.is_empty() {
            return Ok(());
        }

        // Add our own node as a seeder for the resources we are seeding
        let self_router_items: Vec<DhtRouterItem<FudNode>> = vec![self_node.into()];
        for resource in &resources {
            self.add_to_router(
                self.seeders_router.clone(),
                &resource.hash,
                self_router_items.clone(),
            )
            .await;
        }

        info!(target: "fud::init()", "Announcing resources...");
        let seeders = vec![self.node().await.into()];
        for resource in resources {
            let _ = self
                .announce(
                    &resource.hash,
                    &FudAnnounce { key: resource.hash, seeders: seeders.clone() },
                    self.seeders_router.clone(),
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

    /// Get resource hash from path using the sled db
    pub fn path_to_hash(&self, path: &Path) -> Result<Option<blake3::Hash>> {
        let path_string = path.to_string_lossy().to_string();
        let path_bytes = path_string.as_bytes();
        for path_item in self.path_tree.iter() {
            let (key, value) = path_item?;
            if value == path_bytes {
                let bytes: &[u8] = &key;
                if bytes.len() != 32 {
                    return Err(Error::Custom(format!(
                        "Expected a 32-byte BLAKE3, got {} bytes",
                        bytes.len()
                    )));
                }

                let array: [u8; 32] = bytes.try_into().unwrap();
                return Ok(Some(array.into()))
            }
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

        let update_resource = async |resource: &mut Resource,
                                     status: ResourceStatus,
                                     chunked: Option<&ChunkedStorage>,
                                     total_bytes_downloaded: u64,
                                     target_bytes_downloaded: u64| {
            let files = match chunked {
                Some(chunked) => resource.get_selected_files(chunked),
                None => vec![],
            };
            let chunk_hashes = match chunked {
                Some(chunked) => resource.get_selected_chunks(chunked),
                None => HashSet::new(),
            };

            if let Some(chunked) = chunked {
                resource.rtype = match chunked.is_dir() {
                    false => ResourceType::File,
                    true => ResourceType::Directory,
                };
            }

            resource.status = status;
            resource.total_chunks_count = match chunked {
                Some(chunked) => chunked.len() as u64,
                None => 0,
            };
            resource.target_chunks_count = chunk_hashes.len() as u64;
            resource.total_chunks_downloaded = match chunked {
                Some(chunked) => chunked.local_chunks() as u64,
                None => 0,
            };
            resource.target_chunks_downloaded = match chunked {
                Some(chunked) => chunked
                    .iter()
                    .filter(|(hash, available)| chunk_hashes.contains(hash) && *available)
                    .count() as u64,
                None => 0,
            };

            resource.total_bytes_size = match chunked {
                Some(chunked) => chunked.get_fileseq().len(),
                None => 0,
            };
            resource.target_bytes_size = match chunked {
                Some(chunked) => chunked
                    .get_files()
                    .iter()
                    .filter(|(path, _)| files.contains(path))
                    .map(|(_, size)| size)
                    .sum(),
                None => 0,
            };

            resource.total_bytes_downloaded = total_bytes_downloaded;
            resource.target_bytes_downloaded = target_bytes_downloaded;

            notify_event!(self, ResourceUpdated, resource);
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
                    update_resource(&mut resource, ResourceStatus::Incomplete, None, 0, 0).await;
                    continue;
                }
            };
            let mut chunked = match self.geode.get(&resource.hash, &resource_path).await {
                Ok(v) => v,
                Err(_) => {
                    update_resource(&mut resource, ResourceStatus::Incomplete, None, 0, 0).await;
                    continue;
                }
            };
            let verify_res = self.verify_chunks(resource, &mut chunked).await;
            if let Err(e) = verify_res {
                error!(target: "fud::verify_resources()", "Error while verifying chunks of {}: {e}", hash_to_string(&resource.hash));
                update_resource(&mut resource, ResourceStatus::Incomplete, None, 0, 0).await;
                continue;
            }
            let (total_bytes_downloaded, target_bytes_downloaded) = verify_res.unwrap();

            if !chunked.is_complete() {
                update_resource(
                    &mut resource,
                    ResourceStatus::Incomplete,
                    Some(&chunked),
                    total_bytes_downloaded,
                    target_bytes_downloaded,
                )
                .await;
                continue;
            }

            update_resource(
                &mut resource,
                ResourceStatus::Seeding,
                Some(&chunked),
                total_bytes_downloaded,
                target_bytes_downloaded,
            )
            .await;
            seeding_resources.push(resource.clone());
        }

        Ok(seeding_resources)
    }

    /// Query `nodes` to find the seeders for `key`
    async fn fetch_seeders(
        &self,
        nodes: &Vec<FudNode>,
        key: &blake3::Hash,
    ) -> HashSet<DhtRouterItem<FudNode>> {
        let self_node = self.node().await;
        let mut seeders: HashSet<DhtRouterItem<FudNode>> = HashSet::new();

        for node in nodes {
            let channel = match self.get_channel(node, None).await {
                Ok(channel) => channel,
                Err(e) => {
                    warn!(target: "fud::fetch_seeders()", "Could not get a channel for node {}: {e}", hash_to_string(&node.id()));
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

        seeders =
            seeders.iter().filter(|seeder| seeder.node.id() != self_node.id()).cloned().collect();

        info!(target: "fud::fetch_seeders()", "Found {} seeders for {}", seeders.len(), hash_to_string(key));
        seeders
    }

    /// Fetch `chunks` for `chunked` (file or directory) from `seeders`.
    async fn fetch_chunks(
        &self,
        hash: &blake3::Hash,
        chunked: &mut ChunkedStorage,
        seeders: &HashSet<DhtRouterItem<FudNode>>,
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
                    warn!(target: "fud::fetch_chunks()", "Could not get a channel for node {}: {e}", hash_to_string(&seeder.node.id()));
                    continue;
                }
            };
            let mut chunks_to_query = remaining_chunks.clone();
            info!("Requesting chunks from seeder {}", hash_to_string(&seeder.node.id()));
            loop {
                let start_time = Instant::now();
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

                                info!(target: "fud::fetch_chunks()", "Received chunk {} from seeder {}", hash_to_string(&chunk_hash), hash_to_string(&seeder.node.id()));

                                // If we did not write the whole chunk to the filesystem,
                                // save the chunk in the scraps.
                                if bytes_written < reply.chunk.len() {
                                    info!(target: "fud::fetch_chunks()", "Saving chunk {} as a scrap", hash_to_string(&chunk_hash));
                                    let chunk_written = self.geode.get_chunk(chunked, &chunk_hash).await?;
                                    if let Err(e) = self.scrap_tree.insert(chunk_hash.as_bytes(), serialize_async(&Scrap {
                                        chunk: reply.chunk.clone(),
                                        hash_written: blake3::hash(&chunk_written),
                                    }).await) {
                                        error!(target: "fud::fetch_chunks()", "Failed to save chunk {} as a scrap: {e}", hash_to_string(&chunk_hash))
                                    }
                                }

                                // Update resource `chunks_downloaded` and `bytes_downloaded`
                                let mut resources_write = self.resources.write().await;
                                let resource = match resources_write.get_mut(hash) {
                                    Some(resource) => {
                                        resource.status = ResourceStatus::Downloading;
                                        resource.total_chunks_downloaded += 1;
                                        resource.target_chunks_downloaded += 1;

                                        resource.total_bytes_downloaded += reply.chunk.len() as u64;
                                        resource.target_bytes_downloaded += resource.get_selected_bytes(chunked, &reply.chunk) as u64;
                                        resource.speeds.push(reply.chunk.len() as f64 / start_time.elapsed().as_secs_f64());
                                        if resource.speeds.len() > 12 {
                                            resource.speeds = resource.speeds.split_off(resource.speeds.len() - 12); // Only keep the last 6 speeds
                                        }

                                        // If we just fetched the last chunk of a file, compute
                                        // `total_bytes_size` (and `target_bytes_size`) again,
                                        // as `geode.write_chunk()` updated the FileSequence
                                        // to the exact file size.
                                        if let Some((last_chunk_hash, _)) = chunked.iter().last() {
                                            if matches!(resource.rtype, ResourceType::File) && *last_chunk_hash == chunk_hash {
                                                resource.total_bytes_size = chunked.get_fileseq().len();
                                                resource.target_bytes_size = resource.total_bytes_size;
                                            }
                                        }
                                        resource.clone()
                                    }
                                    None => return Ok(()) // Resource was removed, abort
                                };
                                drop(resources_write);

                                notify_event!(self, ChunkDownloadCompleted, { hash: *hash, chunk_hash, resource });
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
                        info!(target: "fud::fetch_chunks()", "Received NOTFOUND {} from seeder {}", hash_to_string(&chunk_hash), hash_to_string(&seeder.node.id()));
                        notify_event!(self, ChunkNotFound, { hash: *hash, chunk_hash });
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
        nodes: &Vec<FudNode>,
        path: &Path,
    ) -> Result<()> {
        let mut queried_seeders: HashSet<blake3::Hash> = HashSet::new();
        let mut result: Option<FetchReply> = None;

        for node in nodes {
            // 1. Request list of seeders
            let channel = match self.get_channel(node, Some(*hash)).await {
                Ok(channel) => channel,
                Err(e) => {
                    warn!(target: "fud::fetch_metadata()", "Could not get a channel for node {}: {e}", hash_to_string(&node.id()));
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
            info!(target: "fud::fetch_metadata()", "Found {} seeders for {} (from {})", seeders.len(), hash_to_string(hash), hash_to_string(&node.id()));

            msg_subscriber.unsubscribe().await;
            self.cleanup_channel(channel).await;

            // 2. Request the file/chunk from the seeders
            while let Some(seeder) = seeders.pop() {
                // Only query a seeder once
                if queried_seeders.iter().any(|s| *s == seeder.node.id()) {
                    continue;
                }
                queried_seeders.insert(seeder.node.id());

                let channel = self.get_channel(&seeder.node, Some(*hash)).await;
                if let Ok(channel) = channel {
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
                            info!(target: "fud::fetch_metadata()", "Received chunk {} (for file {}) from seeder {}", hash_to_string(&chunk_hash), hash_to_string(hash), hash_to_string(&seeder.node.id()));
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
                            info!(target: "fud::fetch_metadata()", "Received file {} from seeder {}", hash_to_string(hash), hash_to_string(&seeder.node.id()));
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
                            info!(target: "fud::fetch_metadata()", "Received directory {} from seeder {}", hash_to_string(hash), hash_to_string(&seeder.node.id()));
                            result = Some(FetchReply::Directory((*reply).clone()));
                            break;
                        }
                        notfound_reply = notfound_recv => {
                            cleanup().await;
                            if let Err(e) = notfound_reply {
                                warn!(target: "fud::fetch_metadata()", "Error waiting for NOTFOUND reply: {e}");
                                continue;
                            }
                            info!(target: "fud::fetch_metadata()", "Received NOTFOUND {} from seeder {}", hash_to_string(hash), hash_to_string(&seeder.node.id()));
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

    /// Try to get the chunked file or directory from geode, if we don't have it
    /// then it is fetched from the network using `fetch_metadata()`.
    pub async fn get_metadata(
        &self,
        hash: &blake3::Hash,
        path: &Path,
    ) -> Result<(ChunkedStorage, Vec<FudNode>)> {
        match self.geode.get(hash, path).await {
            // We already know the metadata
            Ok(v) => Ok((v, vec![])),
            // The metadata in geode is invalid or corrupted
            Err(Error::GeodeNeedsGc) => todo!(),
            // If we could not find the metadata in geode, get it from the network
            Err(Error::GeodeFileNotFound) => {
                // Find nodes close to the file hash
                info!(target: "fud::get_metadata()", "Requested metadata {} not found in Geode, triggering fetch", hash_to_string(hash));
                let closest_nodes = self.lookup_nodes(hash).await.unwrap_or_default();

                // Fetch file or directory metadata
                match self.fetch_metadata(hash, &closest_nodes, path).await {
                    // The file metadata was found and inserted into geode
                    Ok(()) => Ok((self.geode.get(hash, path).await?, closest_nodes)),
                    // We could not find the metadata, or any other error occured
                    Err(e) => Err(e),
                }
            }

            Err(e) => {
                error!(target: "fud::get_metadata()", "{e}");
                Err(e)
            }
        }
    }

    /// Download a file or directory from the network to `path`.
    /// Called when `get()` creates a new fetch task.
    pub async fn fetch_resource(
        &self,
        hash: &blake3::Hash,
        path: &Path,
        files: &FileSelection,
    ) -> Result<()> {
        let self_node = self.node().await;

        let hash_bytes = hash.as_bytes();
        let path_string = path.to_string_lossy().to_string();
        let path_bytes = path_string.as_bytes();

        // Macro that acquires a write lock on `self.resources`, updates a
        // resource, and returns the resource (dropping the write lock)
        macro_rules! update_resource {
            ($hash:ident, { $($field:ident = $value:expr $(,)?)* }) => {{
                let mut resources_write = self.resources.write().await;
                let resource = match resources_write.get_mut($hash) {
                    Some(resource) => {
                        $(resource.$field = $value;)* // Apply the field assignments
                        resource.clone()
                    }
                    None => return Ok(()), // Resource was removed, abort
                };
                resource
            }};
        }

        // Make sure we don't already have another resource on that path
        if let Ok(Some(hash_found)) = self.path_to_hash(path) {
            if *hash != hash_found {
                return Err(Error::Custom(format!(
                    "There is already another resource on path {path_string}"
                )))
            }
        }

        // Add path to the sled db
        self.path_tree.insert(hash_bytes, path_bytes)?;

        // Add file selection to the sled db
        if let FileSelection::Set(selected_files) = files {
            let paths: Vec<Vec<u8>> = selected_files
                .iter()
                .map(|f| f.to_string_lossy().to_string().as_bytes().to_vec())
                .collect();
            let serialized_paths = serialize_async(&paths).await;
            // Abort if the file selection cannot be inserted into sled
            if let Err(e) = self.file_selection_tree.insert(hash_bytes, serialized_paths) {
                return Err(Error::SledError(e))
            }
        }

        // Add resource to `self.resources`
        let resource = Resource::new(
            *hash,
            ResourceType::Unknown,
            path,
            ResourceStatus::Discovering,
            files.clone(),
        );
        let mut resources_write = self.resources.write().await;
        resources_write.insert(*hash, resource.clone());
        drop(resources_write);

        // Send a DownloadStarted event
        notify_event!(self, DownloadStarted, resource);

        // Try to get the chunked file or directory from geode or the network
        let (mut chunked, mut closest_nodes) = match self.get_metadata(hash, path).await {
            Ok(chunked) => chunked,
            Err(e) => {
                // Set resource status to `Incomplete` and send a `MetadataNotFound` event
                let resource = update_resource!(hash, { status = ResourceStatus::Incomplete });
                notify_event!(self, MetadataNotFound, resource);
                return Err(e);
            }
        };

        // Get a list of all file paths the user wants to fetch
        let resources_read = self.resources.read().await;
        let resource = match resources_read.get(hash) {
            Some(resource) => resource,
            None => return Ok(()), // Resource was removed, abort
        };
        let files_vec: Vec<PathBuf> = resource.get_selected_files(&chunked);
        drop(resources_read);

        // Create all files (and all necessary directories)
        for file_path in files_vec.iter() {
            if !file_path.exists() {
                if let Some(dir) = file_path.parent() {
                    fs::create_dir_all(dir).await?;
                }
                File::create(&file_path).await?;
            }
        }

        // Set resource status to `Verifying` and send a `MetadataDownloadCompleted` event
        let resource = update_resource!(hash, {
            status = ResourceStatus::Verifying,
            total_chunks_count = chunked.len() as u64,
            total_bytes_size = chunked.get_fileseq().len(),
            rtype = match chunked.is_dir() {
                false => ResourceType::File,
                true => ResourceType::Directory,
            },
        });
        notify_event!(self, MetadataDownloadCompleted, resource);

        // Set of all chunks we need locally (including the ones we already have)
        let chunk_hashes = resource.get_selected_chunks(&chunked);

        // Write all scraps to make sure the data on the filesystem is correct
        self.write_scraps(&mut chunked, &chunk_hashes).await?;

        // Mark locally available chunks as such
        let verify_res = self.verify_chunks(&resource, &mut chunked).await;
        if let Err(e) = verify_res {
            error!(target: "fud::fetch_resource()", "Error while verifying chunks: {e}");
            return Err(e);
        }
        let (total_bytes_downloaded, target_bytes_downloaded) = verify_res.unwrap();

        // Update `total_bytes_size` if the resource is a file
        if let ResourceType::File = resource.rtype {
            update_resource!(hash, { total_bytes_size = chunked.get_fileseq().len() });
            notify_event!(self, ResourceUpdated, resource);
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

        // Set of all chunks we need locally and their current availability
        let chunks: HashSet<(blake3::Hash, bool)> =
            chunked.iter().filter(|(hash, _)| chunk_hashes.contains(hash)).cloned().collect();

        // Set of the chunks we need to download
        let missing_chunks: HashSet<blake3::Hash> =
            chunks.iter().filter(|&(_, available)| !available).map(|(chunk, _)| *chunk).collect();

        // Update the resource with the chunks/bytes counts
        update_resource!(hash, {
            target_chunks_count = chunks.len() as u64,
            total_chunks_downloaded = chunked.local_chunks() as u64,
            target_chunks_downloaded = (chunks.len() - missing_chunks.len()) as u64,

            target_bytes_size =
                chunked.get_fileseq().subset_len(files_vec.into_iter().collect()),
            total_bytes_downloaded = total_bytes_downloaded,
            target_bytes_downloaded = target_bytes_downloaded,
        });

        // If we don't need to download any chunk
        if missing_chunks.is_empty() {
            // Set resource status to `Seeding` or `Incomplete`
            let resource = update_resource!(hash, {
                status = match chunked.is_complete() {
                    true => ResourceStatus::Seeding,
                    false => ResourceStatus::Incomplete,
                }
            });

            // Announce the resource if we have all chunks
            if chunked.is_complete() {
                let self_announce =
                    FudAnnounce { key: *hash, seeders: vec![self_node.clone().into()] };
                let _ = self.announce(hash, &self_announce, self.seeders_router.clone()).await;
            }

            // Send a DownloadCompleted event
            notify_event!(self, DownloadCompleted, resource);

            return Ok(());
        }

        // Set resource status to `Downloading` and send a MetadataDownloadCompleted event
        let resource = update_resource!(hash, {
            status = ResourceStatus::Downloading,
        });
        notify_event!(self, MetadataDownloadCompleted, resource);

        // Find nodes close to the file hash if we didn't previously fetched them
        if closest_nodes.is_empty() {
            closest_nodes = self.lookup_nodes(hash).await.unwrap_or_default();
        }

        // Find seeders and remove ourselves from the result
        let seeders = self.fetch_seeders(&closest_nodes, hash).await;

        // Fetch missing chunks from seeders
        self.fetch_chunks(hash, &mut chunked, &seeders, &missing_chunks).await?;

        // Get chunked file from geode
        let mut chunked = match self.geode.get(hash, path).await {
            Ok(v) => v,
            Err(e) => {
                error!(target: "fud::fetch_resource()", "{e}");
                return Err(e);
            }
        };

        // Set resource status to `Verifying` and send FudEvent::ResourceUpdated
        let resource = update_resource!(hash, { status = ResourceStatus::Verifying });
        notify_event!(self, ResourceUpdated, resource);

        // Verify all chunks
        self.verify_chunks(&resource, &mut chunked).await?;

        let is_complete = chunked
            .iter()
            .filter(|(hash, _)| chunk_hashes.contains(hash))
            .all(|(_, available)| *available);

        // We fetched all chunks, but the resource is not complete
        // (some chunks were missing from all seeders)
        if !is_complete {
            // Set resource status to `Incomplete`
            let resource = update_resource!(hash, { status = ResourceStatus::Incomplete });

            // Send a MissingChunks event
            notify_event!(self, MissingChunks, resource);

            return Ok(());
        }

        // Set resource status to `Seeding` or `Incomplete`
        let resource = update_resource!(hash, {
            status = match chunked.is_complete() {
                true => ResourceStatus::Seeding,
                false => ResourceStatus::Incomplete,
            },
            target_chunks_downloaded = chunks.len() as u64,
            total_chunks_downloaded = chunked.local_chunks() as u64,
        });

        // Announce the resource if we have all chunks
        if chunked.is_complete() {
            let self_announce = FudAnnounce { key: *hash, seeders: vec![self_node.clone().into()] };
            let _ = self.announce(hash, &self_announce, self.seeders_router.clone()).await;
        }

        // Send a DownloadCompleted event
        notify_event!(self, DownloadCompleted, resource);

        Ok(())
    }

    async fn write_scraps(
        &self,
        chunked: &mut ChunkedStorage,
        chunk_hashes: &HashSet<blake3::Hash>,
    ) -> Result<()> {
        // Get all scraps
        let mut scraps = HashMap::new();
        // TODO: This can be improved to not loop over all chunks
        for chunk_hash in chunk_hashes {
            let scrap = self.scrap_tree.get(chunk_hash.as_bytes())?;
            if scrap.is_none() {
                continue;
            }

            // Verify the scrap we found
            let scrap = deserialize_async(scrap.unwrap().as_ref()).await;
            if scrap.is_err() {
                continue;
            }
            let scrap: Scrap = scrap.unwrap();

            // Add the scrap to the HashMap
            scraps.insert(chunk_hash, scrap);
        }

        // Write all scraps
        if !scraps.is_empty() {
            info!(target: "fud::write_scraps()", "Writing {} scraps...", scraps.len());
        }
        for (scrap_hash, mut scrap) in scraps {
            let len = scrap.chunk.len();
            let write_res = self.geode.write_chunk(chunked, scrap.chunk.clone()).await;
            if let Err(e) = write_res {
                error!(target: "fud::write_scraps()", "Error rewriting scrap {}: {e}", hash_to_string(scrap_hash));
                continue;
            }
            let (_, chunk_bytes_written) = write_res.unwrap();

            // If the whole scrap was written, we can remove it from sled
            if chunk_bytes_written == len {
                self.scrap_tree.remove(scrap_hash.as_bytes())?;
                continue;
            }
            // Otherwise update the scrap in sled
            let chunk_res = self.geode.get_chunk(chunked, scrap_hash).await;
            if let Err(e) = chunk_res {
                error!(target: "fud::write_scraps()", "Failed to get scrap {}: {e}", hash_to_string(scrap_hash));
                continue;
            }
            scrap.hash_written = blake3::hash(&chunk_res.unwrap());
            if let Err(e) =
                self.scrap_tree.insert(scrap_hash.as_bytes(), serialize_async(&scrap).await)
            {
                error!(target: "fud::write_scraps()", "Failed to save chunk {} as a scrap after rewrite: {e}", hash_to_string(scrap_hash));
            }
        }

        Ok(())
    }

    /// Iterate over chunks and find which chunks are available locally,
    /// either in the filesystem (using geode::verify_chunks()) or in scraps.
    /// `chunk_hashes` is the list of chunk hashes we want to take into account, `None` means to
    /// take all chunks into account.
    /// Return the scraps in a HashMap, and the size in bytes of locally available data
    /// (downloaded and downloaded+targeted).
    pub async fn verify_chunks(
        &self,
        resource: &Resource,
        chunked: &mut ChunkedStorage,
    ) -> Result<(u64, u64)> {
        let chunks = chunked.get_chunks().clone();
        let mut bytes: HashMap<blake3::Hash, (usize, usize)> = HashMap::new();

        // Gather all available chunks
        for (chunk_index, (chunk_hash, _)) in chunks.iter().enumerate() {
            // Read the chunk using the `FileSequence`
            let chunk =
                match self.geode.read_chunk(&mut chunked.get_fileseq_mut(), &chunk_index).await {
                    Ok(c) => c,
                    Err(Error::Io(ErrorKind::NotFound)) => continue,
                    Err(e) => {
                        warn!(target: "fud::verify_chunks()", "Error while verifying chunks: {e}");
                        break
                    }
                };

            // Perform chunk consistency check
            if self.geode.verify_chunk(chunk_hash, &chunk) {
                chunked.get_chunk_mut(chunk_index).1 = true;
                bytes.insert(
                    *chunk_hash,
                    (chunk.len(), resource.get_selected_bytes(chunked, &chunk)),
                );
            }
        }

        // Look for the chunks that are not on the filesystem
        let chunks = chunked.get_chunks().clone();
        let missing_on_fs: Vec<_> =
            chunks.iter().enumerate().filter(|(_, (_, available))| !available).collect();

        // Look for scraps
        for (chunk_index, (chunk_hash, _)) in missing_on_fs {
            let scrap = self.scrap_tree.get(chunk_hash.as_bytes())?;
            if scrap.is_none() {
                continue;
            }

            // Verify the scrap we found
            let scrap = deserialize_async(scrap.unwrap().as_ref()).await;
            if scrap.is_err() {
                continue;
            }
            let scrap: Scrap = scrap.unwrap();
            if blake3::hash(&scrap.chunk) != *chunk_hash {
                continue;
            }

            // Check if the scrap is still written on the filesystem
            let scrap_chunk =
                self.geode.read_chunk(&mut chunked.get_fileseq_mut(), &chunk_index).await;
            if scrap_chunk.is_err() {
                continue;
            }
            let scrap_chunk = scrap_chunk.unwrap();

            // The scrap is not available if the chunk on the disk changed
            if !self.geode.verify_chunk(&scrap.hash_written, &scrap_chunk) {
                continue;
            }

            // Mark the chunk as available
            chunked.get_chunk_mut(chunk_index).1 = true;

            // Update the sums of locally available data
            bytes.insert(
                *chunk_hash,
                (scrap.chunk.len(), resource.get_selected_bytes(chunked, &scrap.chunk)),
            );
        }

        // If the resource is a file: make the `FileSequence`'s file the
        // exact file size if we know the last chunk's size. This is not
        // needed for directories.
        if let Some((last_chunk_hash, last_chunk_available)) = chunked.iter().last() {
            if !chunked.is_dir() && *last_chunk_available {
                if let Some((last_chunk_size, _)) = bytes.get(last_chunk_hash) {
                    let exact_file_size =
                        chunked.len() * MAX_CHUNK_SIZE - (MAX_CHUNK_SIZE - last_chunk_size);
                    chunked.get_fileseq_mut().set_file_size(0, exact_file_size as u64);
                }
            }
        }

        let total_bytes_downloaded = bytes.iter().map(|(_, (b, _))| b).sum::<usize>() as u64;
        let target_bytes_downloaded = bytes.iter().map(|(_, (_, b))| b).sum::<usize>() as u64;

        Ok((total_bytes_downloaded, target_bytes_downloaded))
    }

    /// Add a resource from the file system.
    pub async fn put(&self, path: &Path) -> Result<()> {
        let put_tasks = self.put_tasks.read().await;
        drop(put_tasks);

        self.put_tx.send(path.to_path_buf()).await?;

        Ok(())
    }

    /// Insert a file or directory from the file system.
    /// Called when `put()` creates a new put task.
    pub async fn insert_resource(&self, path: &PathBuf) -> Result<()> {
        let self_node = self.node().await;

        if self_node.addresses.is_empty() {
            return Err(Error::Custom(
                "Cannot put resource, you don't have any external address".to_string(),
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
        let total_size = stream.len();
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
                file_selection: FileSelection::All,
                total_chunks_count: chunk_hashes.len() as u64,
                target_chunks_count: chunk_hashes.len() as u64,
                total_chunks_downloaded: chunk_hashes.len() as u64,
                target_chunks_downloaded: chunk_hashes.len() as u64,
                total_bytes_size: total_size,
                target_bytes_size: total_size,
                total_bytes_downloaded: total_size,
                target_bytes_downloaded: total_size,
                speeds: vec![],
            },
        );
        drop(resources_write);

        // Announce the new resource
        let fud_announce = FudAnnounce { key: hash, seeders: vec![self_node.into()] };
        let _ = self.announce(&hash, &fud_announce, self.seeders_router.clone()).await;

        // Send InsertCompleted event
        notify_event!(self, InsertCompleted, {
            hash,
            path: path.to_path_buf()
        });

        Ok(())
    }

    /// Removes:
    /// - a resource
    /// - its metadata in geode
    /// - its path in the sled path tree
    /// - its file selection in the sled file selection tree
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

        // Remove the file selection in sled
        let _ = self.file_selection_tree.remove(hash.as_bytes());

        // Send a `ResourceRemoved` event
        notify_event!(self, ResourceRemoved, { hash: *hash });
    }

    /// Stop all tasks in `fetch_tasks` and `put_tasks.
    pub async fn stop(&self) {
        // Create a clone of fetch_tasks because `task.stop()` needs a write lock
        let fetch_tasks = self.fetch_tasks.read().await;
        let cloned_fetch_tasks: HashMap<blake3::Hash, Arc<StoppableTask>> =
            fetch_tasks.iter().map(|(key, value)| (*key, value.clone())).collect();
        drop(fetch_tasks);

        // Stop all fetch tasks
        for task in cloned_fetch_tasks.values() {
            task.stop().await;
        }

        // Create a clone of put_tasks because `task.stop()` needs a write lock
        let put_tasks = self.put_tasks.read().await;
        let cloned_put_tasks: HashMap<PathBuf, Arc<StoppableTask>> =
            put_tasks.iter().map(|(key, value)| (key.clone(), value.clone())).collect();
        drop(put_tasks);

        // Stop all put tasks
        for task in cloned_put_tasks.values() {
            task.stop().await;
        }
    }
}
