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

use std::{
    collections::{HashMap, HashSet},
    io::ErrorKind,
    path::{Path, PathBuf},
    sync::Arc,
};

use sled_overlay::sled;
use smol::{
    channel,
    fs::{self, OpenOptions},
    lock::RwLock,
};
use tracing::{error, info, warn};

use darkfi::{
    dht::{tasks as dht_tasks, Dht, DhtHandler, DhtSettings},
    geode::{hash_to_string, Chunk, ChunkedStorage, FileSequence, Geode, MAX_CHUNK_SIZE},
    net::P2pPtr,
    system::{ExecutorPtr, PublisherPtr, StoppableTask},
    util::{path::expand_path, time::Timestamp},
    Error, Result,
};
use darkfi_sdk::crypto::{schnorr::SchnorrSecret, SecretKey};
use darkfi_serial::{deserialize_async, serialize_async};

/// P2P protocols
pub mod proto;
use proto::FudAnnounce;

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
use tasks::start_task;

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
use util::{create_all_files, get_all_files, FileSelection};

/// Download methods
mod download;
use download::{fetch_chunks, fetch_metadata};

/// [`DhtHandler`] implementation and fud-specific DHT structs
pub mod dht;
use dht::FudSeeder;

use crate::{dht::FudNode, pow::PowSettings};

const SLED_PATH_TREE: &[u8] = b"_fud_paths";
const SLED_FILE_SELECTION_TREE: &[u8] = b"_fud_file_selections";
const SLED_SCRAP_TREE: &[u8] = b"_fud_scraps";

#[derive(Clone, Debug)]
pub struct FudState {
    /// Our own [`VerifiableNodeData`]
    node_data: VerifiableNodeData,
    /// Our secret key (the public key is in `node_data`)
    secret_key: SecretKey,
}

pub struct Fud {
    state: Arc<RwLock<Option<FudState>>>,
    /// The Geode instance
    geode: Geode,
    /// Default download directory
    downloads_path: PathBuf,
    /// Chunk transfer timeout in seconds
    chunk_timeout: u64,
    /// The [`FudPow`] instance
    pub pow: Arc<RwLock<FudPow>>,
    /// The DHT instance
    dht: Arc<Dht<Fud>>,
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
    /// We save scraps to be able to verify integrity even if part of the chunk
    /// is not saved to the filesystem in the downloaded files.
    /// "chunk/scrap hash -> chunk content"
    scrap_tree: sled::Tree,
    /// Get requests sender
    get_tx: channel::Sender<(blake3::Hash, PathBuf, FileSelection)>,
    /// Get requests receiver
    get_rx: channel::Receiver<(blake3::Hash, PathBuf, FileSelection)>,
    /// Put requests sender
    put_tx: channel::Sender<PathBuf>,
    /// Put requests receiver
    put_rx: channel::Receiver<PathBuf>,
    /// Lookup requests sender
    lookup_tx: channel::Sender<blake3::Hash>,
    /// Lookup requests receiver
    lookup_rx: channel::Receiver<blake3::Hash>,
    /// Verify node requests sender
    verify_node_tx: channel::Sender<FudNode>,
    /// Verify node requests receiver
    verify_node_rx: channel::Receiver<FudNode>,
    /// Currently active downloading tasks (running the `fud.fetch_resource()` method)
    fetch_tasks: Arc<RwLock<HashMap<blake3::Hash, Arc<StoppableTask>>>>,
    /// Currently active put tasks (running the `fud.insert_resource()` method)
    put_tasks: Arc<RwLock<HashMap<PathBuf, Arc<StoppableTask>>>>,
    /// Currently active lookup tasks (running the `fud.lookup_value()` method)
    lookup_tasks: Arc<RwLock<HashMap<blake3::Hash, Arc<StoppableTask>>>>,
    /// Currently active tasks (defined in `tasks`, started with the `start_task` macro)
    tasks: Arc<RwLock<HashMap<String, Arc<StoppableTask>>>>,
    /// Used to send events to fud clients
    event_publisher: PublisherPtr<FudEvent>,
    /// Pointer to the P2P network instance
    p2p: P2pPtr,
    /// Global multithreaded executor reference
    pub executor: ExecutorPtr,
}

impl Fud {
    pub async fn new(
        settings: Args,
        p2p: P2pPtr,
        sled_db: &sled::Db,
        event_publisher: PublisherPtr<FudEvent>,
        executor: ExecutorPtr,
    ) -> Result<Arc<Self>> {
        let dht_settings: DhtSettings = settings.dht.into();
        let net_settings_lock = p2p.settings();
        let mut net_settings = net_settings_lock.write().await;
        // We do not need any outbound slot
        net_settings.outbound_connections = 0;
        // Default GetAddrsMessage's `max` is dht's `k`
        net_settings.getaddrs_max =
            Some(net_settings.getaddrs_max.unwrap_or(dht_settings.k.min(u32::MAX as usize) as u32));
        drop(net_settings);

        let basedir = expand_path(&settings.base_dir)?;
        let downloads_path = match settings.downloads_path {
            Some(downloads_path) => expand_path(&downloads_path)?,
            None => basedir.join("downloads"),
        };

        let pow_settings: PowSettings = settings.pow.into();
        let pow = FudPow::new(pow_settings.clone(), executor.clone());

        // Geode
        info!(target: "fud::new()", "Instantiating Geode instance");
        let geode = Geode::new(&basedir).await?;

        // DHT
        let dht: Arc<Dht<Fud>> =
            Arc::new(Dht::<Fud>::new(&dht_settings, p2p.clone(), executor.clone()).await);

        let (get_tx, get_rx) = smol::channel::unbounded();
        let (put_tx, put_rx) = smol::channel::unbounded();
        let (lookup_tx, lookup_rx) = smol::channel::unbounded();
        let (verify_node_tx, verify_node_rx) = smol::channel::unbounded();
        let fud = Arc::new(Self {
            state: Arc::new(RwLock::new(None)),
            geode,
            downloads_path,
            chunk_timeout: settings.chunk_timeout,
            pow: Arc::new(RwLock::new(pow)),
            dht: dht.clone(),
            path_tree: sled_db.open_tree(SLED_PATH_TREE)?,
            file_selection_tree: sled_db.open_tree(SLED_FILE_SELECTION_TREE)?,
            scrap_tree: sled_db.open_tree(SLED_SCRAP_TREE)?,
            resources: Arc::new(RwLock::new(HashMap::new())),
            get_tx,
            get_rx,
            put_tx,
            put_rx,
            lookup_tx,
            lookup_rx,
            verify_node_tx,
            verify_node_rx,
            fetch_tasks: Arc::new(RwLock::new(HashMap::new())),
            put_tasks: Arc::new(RwLock::new(HashMap::new())),
            lookup_tasks: Arc::new(RwLock::new(HashMap::new())),
            tasks: Arc::new(RwLock::new(HashMap::new())),
            event_publisher,
            p2p,
            executor,
        });
        *dht.handler.write().await = Arc::downgrade(&fud);

        Ok(fud)
    }

    /// Run the PoW and generate a `VerifiableNodeData`, then start tasks
    pub async fn start(self: &Arc<Self>) -> Result<()> {
        let mut pow = self.pow.write().await;
        if pow.settings.read().await.btc_enabled {
            pow.bitcoin_hash_cache.update().await?; // Fetch BTC block hashes
        }
        let (node_data, secret_key) = pow.generate_node().await?;
        info!(target: "fud::init()", "Your node ID: {}", hash_to_string(&node_data.id()));
        let mut state = self.state.write().await;
        *state = Some(FudState { node_data, secret_key });
        drop(state);
        drop(pow);

        self.start_tasks().await;

        Ok(())
    }

    async fn start_tasks(self: &Arc<Self>) {
        let mut tasks = self.tasks.write().await;
        start_task!(self, "get", tasks::get_task, tasks);
        start_task!(self, "put", tasks::put_task, tasks);
        start_task!(self, "events", tasks::handle_dht_events, tasks);
        start_task!(self, "DHT events", dht_tasks::events_task::<Fud>, tasks);
        start_task!(self, "DHT channel", dht_tasks::channel_task::<Fud>, tasks);
        start_task!(self, "DHT cleanup channels", dht_tasks::cleanup_channels_task::<Fud>, tasks);
        start_task!(self, "DHT add node", dht_tasks::add_node_task::<Fud>, tasks);
        start_task!(self, "DHT refinery", dht_tasks::dht_refinery_task::<Fud>, tasks);
        start_task!(
            self,
            "DHT disconnect inbounds",
            dht_tasks::disconnect_inbounds_task::<Fud>,
            tasks
        );
        start_task!(self, "lookup", tasks::lookup_task, tasks);
        start_task!(self, "verify node", tasks::verify_node_task, tasks);
        start_task!(self, "announce", tasks::announce_seed_task, tasks);
        start_task!(self, "node ID", tasks::node_id_task, tasks);
    }

    /// Verify our resources, add ourselves to the seeders (`dht.hash_table`)
    /// for the resources we already have, announce our resources.
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

        let self_node = self.node().await?;

        // Stop here if we have no external address
        if self_node.addresses.is_empty() {
            return Ok(());
        }

        // Add our own node as a seeder for the resources we are seeding
        for resource in &resources {
            if let Ok(seeder) = self.new_seeder(&resource.hash).await {
                let self_router_items = vec![seeder];
                self.add_value(&resource.hash, &self_router_items).await;
            }
        }

        info!(target: "fud::init()", "Announcing resources...");
        for resource in resources {
            if let Ok(seeder) = self.new_seeder(&resource.hash).await {
                let seeders = vec![seeder];
                let _ = self
                    .dht
                    .announce(
                        &resource.hash,
                        &seeders.clone(),
                        &FudAnnounce { key: resource.hash, seeders },
                    )
                    .await;
            }
        }

        Ok(())
    }

    /// Get a copy of the current resources
    pub async fn resources(&self) -> HashMap<blake3::Hash, Resource> {
        let resources = self.resources.read().await;
        resources.clone()
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

    /// Create a new [`dht::FudSeeder`] for own node
    pub async fn new_seeder(&self, key: &blake3::Hash) -> Result<FudSeeder> {
        let state = self.state.read().await;
        if state.is_none() {
            return Err(Error::Custom("Fud is not ready yet".to_string()))
        }
        let state_ = state.clone().unwrap();
        drop(state);
        let node = self.node().await?;

        Ok(FudSeeder {
            key: *key,
            node: node.clone(),
            sig: state_
                .secret_key
                .sign(&[key.as_bytes().to_vec(), serialize_async(&node).await].concat()),
            timestamp: Timestamp::current_time().inner(),
        })
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
                    .filter(|chunk| chunk_hashes.contains(&chunk.hash) && chunk.available)
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
    /// If we need to fetch from the network, the seeders we find are sent to
    /// `seeders_pub`.
    /// The seeder in the returned result is only defined if we fetched from
    /// the network.
    pub async fn get_metadata(
        &self,
        hash: &blake3::Hash,
        path: &Path,
    ) -> Result<(ChunkedStorage, Option<FudSeeder>)> {
        match self.geode.get(hash, path).await {
            // We already know the metadata
            Ok(v) => Ok((v, None)),
            // The metadata in geode is invalid or corrupted
            Err(Error::GeodeNeedsGc) => todo!(),
            // If we could not find the metadata in geode, get it from the network
            Err(Error::GeodeFileNotFound) => {
                // Find nodes close to the file hash
                info!(target: "fud::get_metadata()", "Requested metadata {} not found in Geode, triggering fetch", hash_to_string(hash));
                let dht_sub = self.dht.subscribe().await;
                if let Err(e) = self.lookup_tx.send(*hash).await {
                    dht_sub.unsubscribe().await;
                    return Err(e.into())
                }

                // Fetch resource metadata
                let fetch_res = fetch_metadata(self, hash, path, &dht_sub).await;
                dht_sub.unsubscribe().await;
                let seeder = fetch_res?;
                Ok((self.geode.get(hash, path).await?, Some(seeder)))
            }
            Err(e) => Err(e),
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

        // Subscribe to DHT events early for `fetch_chunks()`
        let dht_sub = self.dht.subscribe().await;

        // Send a DownloadStarted event
        notify_event!(self, DownloadStarted, resource);

        // Try to get the chunked file or directory from geode
        let metadata_result = self.get_metadata(hash, path).await;

        if let Err(e) = metadata_result {
            // Set resource status to `Incomplete` and send a `MetadataNotFound` event
            let resource = update_resource!(hash, { status = ResourceStatus::Incomplete });
            notify_event!(self, MetadataNotFound, resource);
            dht_sub.unsubscribe().await;
            return Err(e)
        }
        let (mut chunked, metadata_seeder) = metadata_result.unwrap();

        // Get a list of all file paths the user wants to fetch
        let resources_read = self.resources.read().await;
        let resource = match resources_read.get(hash) {
            Some(resource) => resource,
            None => {
                // Resource was removed, abort
                dht_sub.unsubscribe().await;
                return Ok(())
            }
        };
        let files_vec: Vec<PathBuf> = resource.get_selected_files(&chunked);
        drop(resources_read);

        // Create all files (and all necessary directories)
        if let Err(e) = create_all_files(&files_vec).await {
            dht_sub.unsubscribe().await;
            return Err(e)
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
        if let Err(e) = self.write_scraps(&mut chunked, &chunk_hashes).await {
            dht_sub.unsubscribe().await;
            return Err(e)
        }

        // Mark locally available chunks as such
        let verify_res = self.verify_chunks(&resource, &mut chunked).await;
        if let Err(e) = verify_res {
            dht_sub.unsubscribe().await;
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
            let fs_metadata = fs::metadata(&path).await;
            if let Err(e) = fs_metadata {
                dht_sub.unsubscribe().await;
                return Err(e.into());
            }
            if fs_metadata.unwrap().len() > (chunked.len() * MAX_CHUNK_SIZE) as u64 {
                if let Ok(file) = OpenOptions::new().write(true).create(true).open(path).await {
                    let _ = file.set_len((chunked.len() * MAX_CHUNK_SIZE) as u64).await;
                }
            }
        }

        // Set of all chunks we need locally and their current availability
        let chunks: HashSet<Chunk> =
            chunked.iter().filter(|c| chunk_hashes.contains(&c.hash)).cloned().collect();

        // Set of the chunks we need to download
        let mut missing_chunks: HashSet<blake3::Hash> =
            chunks.iter().filter(|&c| !c.available).map(|c| c.hash).collect();

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

        let download_completed = async |chunked: &ChunkedStorage| -> Result<()> {
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
                if let Ok(seeder) = self.new_seeder(hash).await {
                    let seeders = vec![seeder];
                    let self_announce = FudAnnounce { key: *hash, seeders: seeders.clone() };
                    let _ = self.dht.announce(hash, &seeders, &self_announce).await;
                }
            }

            // Send a DownloadCompleted event
            notify_event!(self, DownloadCompleted, resource);

            Ok(())
        };

        // If we don't need to download any chunk
        if missing_chunks.is_empty() {
            dht_sub.unsubscribe().await;
            return download_completed(&chunked).await;
        }

        // Set resource status to `Downloading` and send a MetadataDownloadCompleted event
        let resource = update_resource!(hash, {
            status = ResourceStatus::Downloading,
        });
        notify_event!(self, MetadataDownloadCompleted, resource);

        // Start looking up seeders if we did not need to do it for the metadata
        if metadata_seeder.is_none() {
            if let Err(e) = self.lookup_tx.send(*hash).await {
                dht_sub.unsubscribe().await;
                return Err(e.into())
            }
        }

        // Fetch missing chunks from seeders
        let _ =
            fetch_chunks(self, hash, &mut chunked, &dht_sub, metadata_seeder, &mut missing_chunks)
                .await;

        // We don't need the DHT events sub anymore
        dht_sub.unsubscribe().await;

        // Get chunked file from geode
        let mut chunked = self.geode.get(hash, path).await?;

        // Set resource status to `Verifying` and send FudEvent::ResourceUpdated
        let resource = update_resource!(hash, { status = ResourceStatus::Verifying });
        notify_event!(self, ResourceUpdated, resource);

        // Verify all chunks
        self.verify_chunks(&resource, &mut chunked).await?;

        let is_complete =
            chunked.iter().filter(|c| chunk_hashes.contains(&c.hash)).all(|c| c.available);

        // We fetched all chunks, but the resource is not complete
        // (some chunks were missing from all seeders)
        if !is_complete {
            // Set resource status to `Incomplete`
            let resource = update_resource!(hash, { status = ResourceStatus::Incomplete });

            // Send a MissingChunks event
            notify_event!(self, MissingChunks, resource);

            return Ok(());
        }

        download_completed(&chunked).await
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
        for (chunk_index, chunk) in chunks.iter().enumerate() {
            // Read the chunk using the `FileSequence`
            let chunk_data =
                match self.geode.read_chunk(&mut chunked.get_fileseq_mut(), &chunk_index).await {
                    Ok(c) => c,
                    Err(Error::Io(ErrorKind::NotFound)) => continue,
                    Err(e) => {
                        warn!(target: "fud::verify_chunks()", "Error while verifying chunks: {e}");
                        break
                    }
                };

            // Perform chunk consistency check
            if self.geode.verify_chunk(&chunk.hash, &chunk_data) {
                chunked.get_chunk_mut(chunk_index).available = true;
                chunked.get_chunk_mut(chunk_index).size = chunk_data.len();
                bytes.insert(
                    chunk.hash,
                    (
                        chunk_data.len(),
                        resource.get_bytes_of_selection(
                            chunked,
                            file_selection,
                            &chunk.hash,
                            chunk_data.len(),
                        ),
                    ),
                );
            } else {
                chunked.get_chunk_mut(chunk_index).available = false;
            }
        }

        // Look for the chunks that are not on the filesystem
        let chunks = chunked.get_chunks().clone();
        let missing_on_fs: Vec<_> =
            chunks.iter().enumerate().filter(|(_, c)| !c.available).collect();

        // Look for scraps
        for (chunk_index, chunk) in missing_on_fs {
            let scrap = self.scrap_tree.get(chunk.hash.as_bytes())?;
            if scrap.is_none() {
                continue;
            }

            // Verify the scrap we found
            let scrap = deserialize_async(scrap.unwrap().as_ref()).await;
            if scrap.is_err() {
                continue;
            }
            let scrap: Scrap = scrap.unwrap();
            if blake3::hash(&scrap.chunk) != chunk.hash {
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
            chunked.get_chunk_mut(chunk_index).available = true;
            chunked.get_chunk_mut(chunk_index).size = scrap.chunk.len();

            // Update the sums of locally available data
            bytes.insert(
                *chunk_hash,
                (scrap.chunk.len(), resource.get_selected_bytes(chunked, &scrap.chunk)),
            );
        }

        // If the resource is a file: make the `FileSequence`'s file the
        // exact file size if we know the last chunk's size. This is not
        // needed for directories.
        let is_dir = chunked.is_dir();
        if let Some(last_chunk) = chunked.iter_mut().last() {
            if !is_dir && last_chunk.available {
                if let Some((last_chunk_size, _)) = bytes.get(&last_chunk.hash) {
                    last_chunk.size = *last_chunk_size;
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
        let self_node = self.node().await?;

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
        if let Ok(seeder) = self.new_seeder(&hash).await {
            let seeders = vec![seeder];
            let fud_announce = FudAnnounce { key: hash, seeders: seeders.clone() };
            let _ = self.dht.announce(&hash, &seeders, &fud_announce).await;
        }

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
                for chunk in chunked.iter() {
                    let _ = self.scrap_tree.remove(chunk.hash.as_bytes());
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

    /// Remove seeders that are older than `expiry_secs`
    pub async fn prune_seeders(&self, expiry_secs: u32) {
        let expiry_timestamp = Timestamp::current_time().inner() - (expiry_secs as u64);
        let mut seeders_write = self.dht.hash_table.write().await;

        let keys: Vec<_> = seeders_write.keys().cloned().collect();

        for key in keys {
            let items = seeders_write.get_mut(&key).unwrap();
            items.retain(|item| item.timestamp > expiry_timestamp);
            if items.is_empty() {
                seeders_write.remove(&key);
            }
        }
    }

    /// Stop all tasks.
    pub async fn stop(&self) {
        info!("Stopping fetch tasks...");
        // Create a clone of fetch_tasks because `task.stop()` needs a write lock
        let fetch_tasks = self.fetch_tasks.read().await;
        let cloned_fetch_tasks: HashMap<blake3::Hash, Arc<StoppableTask>> =
            fetch_tasks.iter().map(|(key, value)| (*key, value.clone())).collect();
        drop(fetch_tasks);

        for task in cloned_fetch_tasks.values() {
            task.stop().await;
        }

        info!("Stopping put tasks...");
        let put_tasks = self.put_tasks.read().await;
        let cloned_put_tasks: HashMap<PathBuf, Arc<StoppableTask>> =
            put_tasks.iter().map(|(key, value)| (key.clone(), value.clone())).collect();
        drop(put_tasks);

        for task in cloned_put_tasks.values() {
            task.stop().await;
        }

        info!("Stopping lookup tasks...");
        let lookup_tasks = self.lookup_tasks.read().await;
        let cloned_lookup_tasks: HashMap<blake3::Hash, Arc<StoppableTask>> =
            lookup_tasks.iter().map(|(key, value)| (*key, value.clone())).collect();
        drop(lookup_tasks);

        for task in cloned_lookup_tasks.values() {
            task.stop().await;
        }

        // Stop all other tasks
        let mut tasks = self.tasks.write().await;
        for (name, task) in tasks.clone() {
            info!("Stopping {name} task...");
            task.stop().await;
        }
        *tasks = HashMap::new();
    }
}
