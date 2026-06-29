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

//! Multi-DAG Event Graph with bidirectional sync, RLN rate limiting,
//! and periodic DAG rotation.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque},
    path::PathBuf,
    str::FromStr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use darkfi_sdk::{crypto::pasta_prelude::PrimeField, pasta::pallas};
use darkfi_serial::{deserialize_async, deserialize_async_partial, serialize_async};
use futures::{stream::FuturesUnordered, StreamExt};
use sled_overlay::{sled, SledTreeOverlay};
use smol::{
    lock::{OnceCell, RwLock},
    Executor,
};
use tracing::{error, info, warn};
use url::Url;

use crate::{
    net::{channel::Channel, P2pPtr},
    system::{msleep, Publisher, PublisherPtr, StoppableTask, StoppableTaskPtr, Subscription},
    Error, Result,
};

pub mod event;
pub use event::{display_order, Event, Header};

pub mod proto;
use proto::{
    cap_layer_tips, count_layer_tips, EventRep, EventReq, HeaderRep, HeaderReq, StaticPut,
    SyncDirection, TipRep, TipReq, MAX_EVENT_REP_EVENTS, MAX_EVENT_REQ_IDS, MAX_HEADER_REP_HEADERS,
    MAX_HEADER_REQ_TIPS, MAX_RANGE_PAGE_SIZE, MAX_TIP_REP_TIPS,
};

pub mod rln;
use rln::{IdentityState, RlnState, ZkKeys};

pub mod util;
use util::{
    generate_genesis, millis_until_next_rotation, next_hour_timestamp, next_rotation_timestamp,
    replayer_log,
};

pub mod deg;
use deg::DegEvent;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod tests_rln;

#[cfg(test)]
mod test_helpers;

/// Number of parent references each event carries.
pub const N_EVENT_PARENTS: usize = 5;

/// Allowed timestamp drift in milliseconds.
const EVENT_TIME_DRIFT: u64 = 60_000;

/// The null event ID (32 zero bytes).
pub const NULL_ID: blake3::Hash = blake3::Hash::from_bytes([0x00; blake3::OUT_LEN]);

/// Array of null parents (used by genesis events).
pub const NULL_PARENTS: [blake3::Hash; N_EVENT_PARENTS] = [NULL_ID; N_EVENT_PARENTS];

/// Maximum number of static-DAG events `static_sync` will pull in
/// one invocation. Defends against malicious deep-ancestry chains.
const SYNC_MAX_STATIC_EVENTS: usize = 100_000;

/// Runtime configuration for an Event Graph instance.
#[derive(Clone, Debug)]
pub struct EventGraphConfig {
    /// Epoch origin timestamp in millis.
    /// All rotation boundaries are computed as offsets from this point.
    /// Should be UTC midnight for clean hourly alignment.
    pub initial_genesis: u64,
    /// How often the DAG rotates, in hours. 0 = no rotation.
    pub hours_rotation: u64,
    /// Unique payload embedded in genesis events.
    /// Different protocols must use different values.
    pub genesis_contents: Vec<u8>,
    /// App-provided pregenerated RLN identity commitments.
    ///
    /// EventGraph treats these as the only proof-less registration
    /// commitments accepted with [`rln::GENESIS_BLOB_GUARD`]. Apps
    /// that do not use pregenerated RLN identities should leave this
    /// empty.
    pub pregenerated_identity_commitments: Vec<[u8; 32]>,
    /// Maximum number of DAGs to keep in the rolling window.
    ///
    /// * `Some(n)` - keep n rotation periods.
    ///   When the n+1 period is created, the oldest is permanently
    ///   deleted from sled. This is the normal mode for end-user nodes.
    /// * `None` - never prune. Every DAG ever created is kept in sled
    ///   and loaded at startup. This is archive mode for nodes that want
    ///   complete history.
    ///
    /// With `hours_rotation = 1` and `max_dags = Some(24)`, events
    /// older than 24 hours are lost. With `hours_rotation = 6` and
    /// `max_dags = Some(24)`, the window is 6 days.
    pub max_dags: Option<usize>,
}

impl EventGraphConfig {
    /// Validate consensus-critical event graph configuration.
    pub fn validate(&self) -> Result<()> {
        if self.max_dags == Some(0) {
            return Err(Error::Custom("event graph max_dags must be greater than 0".into()))
        }

        self.rotation_period_millis()?;
        Ok(())
    }

    /// Rotation period in milliseconds, or `None` for non-rotating graphs.
    pub(crate) fn rotation_period_millis(&self) -> Result<Option<u64>> {
        if self.hours_rotation == 0 {
            return Ok(None)
        }

        let rotation_ms = self.hours_rotation.checked_mul(util::HOUR_MS).ok_or_else(|| {
            Error::Custom("event graph rotation period overflows milliseconds".into())
        })?;

        if self.initial_genesis.checked_add(rotation_ms).is_none() {
            return Err(Error::Custom(
                "event graph initial genesis plus one rotation overflows".into(),
            ))
        }

        Ok(Some(rotation_ms))
    }
}

pub type EventGraphPtr = Arc<EventGraph>;
/// Unreferenced tips grouped by layer.
pub type LayerUTips = BTreeMap<u64, HashSet<blake3::Hash>>;

/// Generate the deterministic genesis event for the static DAG.
fn generate_static_genesis(config: &EventGraphConfig) -> Event {
    let header = Header {
        timestamp: config.initial_genesis,
        parents: NULL_PARENTS,
        layer: 0,
        content_hash: blake3::hash(&config.genesis_contents),
    };

    Event { header, content: config.genesis_contents.clone() }
}

fn validate_pregenerated_identity_commitments(
    config: &EventGraphConfig,
) -> Result<(Vec<pallas::Base>, HashSet<[u8; 32]>)> {
    let mut commitments = Vec::with_capacity(config.pregenerated_identity_commitments.len());
    let mut reprs = HashSet::with_capacity(config.pregenerated_identity_commitments.len());

    for (index, repr) in config.pregenerated_identity_commitments.iter().enumerate() {
        if !reprs.insert(*repr) {
            return Err(Error::Custom(format!(
                "duplicate pregenerated identity commitment at index {index}"
            )))
        }

        let commitment: Option<pallas::Base> = pallas::Base::from_repr(*repr).into();
        let Some(commitment) = commitment else {
            return Err(Error::Custom(format!(
                "invalid pregenerated identity commitment at index {index}"
            )))
        };

        commitments.push(commitment);
    }

    Ok((commitments, reprs))
}

/// Bidirectional timestamp -> event-ID index.
#[derive(Clone, Debug, Default)]
pub struct TimeIndex {
    index: BTreeMap<u64, Vec<blake3::Hash>>,
    count: usize,
}

impl TimeIndex {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn from_header_dag(tree: &sled::Tree) -> Self {
        let mut idx = Self::new();
        for item in tree.iter() {
            let (id, hdr) = item.unwrap();
            let id = blake3::Hash::from_bytes((&id as &[u8]).try_into().unwrap());
            let hdr: Header = deserialize_async(&hdr).await.unwrap();
            idx.insert(hdr.timestamp, id);
        }
        idx
    }

    pub fn insert(&mut self, ts: u64, id: blake3::Hash) {
        let ids = self.index.entry(ts).or_default();
        if ids.contains(&id) {
            return
        }

        ids.push(id);
        self.count += 1;
    }

    pub fn newest(&self, n: usize) -> Vec<blake3::Hash> {
        self.rev(u64::MAX, n)
    }

    pub fn oldest(&self, n: usize) -> Vec<blake3::Hash> {
        self.fwd(0, n)
    }

    pub fn before(&self, cursor: u64, n: usize) -> Vec<blake3::Hash> {
        self.rev(cursor.saturating_sub(1), n)
    }

    pub fn after(&self, cursor: u64, n: usize) -> Vec<blake3::Hash> {
        self.fwd(cursor.saturating_add(1), n)
    }

    fn rev(&self, start: u64, n: usize) -> Vec<blake3::Hash> {
        let mut out = Vec::with_capacity(n);
        for (_, ids) in self.index.range(..=start).rev() {
            for id in ids {
                out.push(*id);
                if out.len() >= n {
                    return out
                }
            }
        }
        out
    }

    fn fwd(&self, start: u64, n: usize) -> Vec<blake3::Hash> {
        let mut out = Vec::with_capacity(n);
        for (_, ids) in self.index.range(start..) {
            for id in ids {
                out.push(*id);
                if out.len() >= n {
                    return out
                }
            }
        }
        out
    }

    pub fn len(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
}

/// All per-DAG state: trees, tips, and the timestamp index.
pub struct DagSlot {
    pub header_tree: sled::Tree,
    pub main_tree: sled::Tree,
    pub tips: LayerUTips,
    pub time_index: TimeIndex,
}

/// Full-scan tip computation.
/// Compute unreferenced tips - events that exist in the DAG but are
/// not referenced as a parent by any other event - grouped by layer.
pub(crate) async fn compute_unreferenced_tips(dag: &sled::Tree) -> LayerUTips {
    let mut candidates: HashMap<blake3::Hash, u64> = HashMap::new();
    let mut referenced: HashSet<blake3::Hash> = HashSet::new();

    for item in dag.iter() {
        let (id_bytes, val_bytes) = item.unwrap();
        let id = blake3::Hash::from_bytes((&id_bytes as &[u8]).try_into().unwrap());
        let ev: Event = deserialize_async(&val_bytes).await.unwrap();

        candidates.insert(id, ev.header.layer);
        for p in ev.header.parents.iter() {
            if *p != NULL_ID {
                referenced.insert(*p);
            }
        }
    }

    // Bucket the unreferenced candidates by their layer
    let mut map: LayerUTips = BTreeMap::new();
    for (id, layer) in candidates {
        if !referenced.contains(&id) {
            map.entry(layer).or_default().insert(id);
        }
    }
    map
}

/// Pick up to N_EVENT_PARENTS tips from the highest layers.
///
/// If the highest local tip is already at `u64::MAX`, no valid child
/// layer exists. Return `u64::MAX` rather than wrapping; header
/// validation rejects any attempted child of that saturated layer.
fn select_parents_from_tips(tips: &LayerUTips) -> (u64, [blake3::Hash; N_EVENT_PARENTS]) {
    let mut parents = [NULL_ID; N_EVENT_PARENTS];
    let mut i = 0;
    'outer: for (_, layer_tips) in tips.iter().rev() {
        for t in layer_tips {
            parents[i] = *t;
            i += 1;
            if i >= N_EVENT_PARENTS {
                break 'outer
            }
        }
    }

    let layer =
        tips.last_key_value().and_then(|(layer, _)| layer.checked_add(1)).unwrap_or(u64::MAX);
    (layer, parents)
}

/// Storage layer for all rotating DAGs.
pub struct DagStore {
    db: sled::Db,
    dags: BTreeMap<u64, DagSlot>,
}

impl DagStore {
    /// Create or open DAG slots.
    ///
    /// * **Bounded mode** (`max_dags = Some(n)`): create a rolling
    ///   window of the most recent `n` DAGs. Old trees already in
    ///   sled outside this window are left untouched (they're just
    ///   not loaded into memory).
    /// * **Archive mode** (`max_dags = None`): discover *all*
    ///   existing DAG trees in sled and load them, plus ensure the
    ///   recent window exists. Nothing is ever dropped.
    pub async fn new(sled_db: sled::Db, config: &EventGraphConfig) -> Result<Self> {
        config.validate()?;
        let mut dags = BTreeMap::new();

        if config.hours_rotation == 0 {
            let genesis = generate_genesis(config);
            dags.insert(genesis.header.timestamp, Self::create_slot(&sled_db, &genesis).await);
            return Ok(Self { db: sled_db, dags })
        }

        // Determine how many recent DAGs to create/ensure exist.
        let window = config.max_dags.unwrap_or(24);

        // In archive mode, first discover and load any existing DAG
        // trees that are already in sled from previous runs.
        //
        // A DAG is stored across two trees: `<timestamp>` for events
        // and `headers_<timestamp>` for headers. We walk every tree
        // name in sled and pick out the ones whose name is a valid u64
        // timestamp.
        if config.max_dags.is_none() {
            for name in sled_db.tree_names() {
                let name_str = String::from_utf8_lossy(&name);
                if let Ok(ts) = name_str.parse::<u64>() {
                    // Reconstruct the genesis for this timestamp
                    let hdr = Header {
                        timestamp: ts,
                        parents: NULL_PARENTS,
                        layer: 0,
                        content_hash: blake3::hash(&config.genesis_contents),
                    };
                    let genesis = Event { header: hdr, content: config.genesis_contents.clone() };
                    let slot = Self::create_slot(&sled_db, &genesis).await;
                    dags.insert(ts, slot);
                }
            }
        }

        // Ensure the recent window of DAGs exists.
        // Creates them if they're not already loaded from the discovery step.
        for i in 1..=window {
            let ts = next_hour_timestamp((i as i64) - (window as i64));
            if dags.contains_key(&ts) {
                // Already loaded from sled discovery
                continue
            }
            let hdr = Header {
                timestamp: ts,
                parents: NULL_PARENTS,
                layer: 0,
                content_hash: blake3::hash(&config.genesis_contents),
            };
            let genesis = Event { header: hdr, content: config.genesis_contents.clone() };
            dags.insert(ts, Self::create_slot(&sled_db, &genesis).await);
        }

        Ok(Self { db: sled_db, dags })
    }

    async fn create_slot(db: &sled::Db, genesis: &Event) -> DagSlot {
        let name = genesis.header.timestamp.to_string();
        let ht = db.open_tree(format!("headers_{name}")).unwrap();
        let mt = db.open_tree(&name).unwrap();
        for (tree, data) in
            [(&ht, serialize_async(&genesis.header).await), (&mt, serialize_async(genesis).await)]
        {
            if tree.is_empty() {
                let mut ov = SledTreeOverlay::new(tree);
                ov.insert(genesis.id().as_bytes(), &data).unwrap();
                if let Some(b) = ov.aggregate() {
                    tree.apply_batch(b).unwrap();
                }
            }
        }
        DagSlot {
            tips: compute_unreferenced_tips(&mt).await,
            time_index: TimeIndex::from_header_dag(&ht).await,
            header_tree: ht,
            main_tree: mt,
        }
    }

    /// Add a new DAG on rotation. In bounded mode, drops the oldest DAG
    /// when the limit is reached. In archive mode, never drops.
    pub async fn add_dag(&mut self, genesis: &Event, max_dags: Option<usize>) -> Result<()> {
        if let Some(limit) = max_dags {
            if limit == 0 {
                return Err(Error::Custom("event graph max_dags must be greater than 0".into()))
            }

            if self.dags.len() >= limit {
                let Some((_, old)) = self.dags.pop_first() else {
                    return Err(Error::Custom("event graph DAG store is empty".into()))
                };
                self.db.drop_tree(old.header_tree.name()).unwrap();
                self.db.drop_tree(old.main_tree.name()).unwrap();
            }
        }
        let slot = Self::create_slot(&self.db, genesis).await;
        self.dags.insert(genesis.header.timestamp, slot);
        Ok(())
    }

    pub fn get_slot(&self, ts: &u64) -> Option<&DagSlot> {
        self.dags.get(ts)
    }

    pub fn get_slot_mut(&mut self, ts: &u64) -> Option<&mut DagSlot> {
        self.dags.get_mut(ts)
    }

    pub fn get_header_tree(&self, dag_name: &str) -> sled::Tree {
        self.db.open_tree(format!("headers_{dag_name}")).unwrap()
    }

    pub fn dag_timestamps(&self) -> Vec<u64> {
        self.dags.keys().cloned().collect()
    }
}

enum PeerStatus {
    Free,
    Busy,
    Failed,
}

/// Match an `EventRep` against the exact IDs requested for one sync chunk.
///
/// The response may be partial, but every returned event must be unique and
/// must belong to the outstanding request. Returned events and blobs are
/// reordered to match the request order, and missing IDs are returned for
/// retry with another peer.
pub(crate) fn filter_requested_event_rep(
    requested: &[blake3::Hash],
    events: Vec<Event>,
    blobs: Vec<Vec<u8>>,
) -> Result<(Vec<Event>, Vec<Vec<u8>>, Vec<blake3::Hash>)> {
    if events.len() != blobs.len() {
        return Err(Error::DagSyncFailed)
    }

    let requested_set: HashSet<blake3::Hash> = requested.iter().copied().collect();
    let mut by_id = HashMap::with_capacity(events.len());

    for (event, blob) in events.into_iter().zip(blobs.into_iter()) {
        let event_id = event.id();
        if !requested_set.contains(&event_id) || by_id.insert(event_id, (event, blob)).is_some() {
            return Err(Error::DagSyncFailed)
        }
    }

    let mut matched_events = Vec::with_capacity(by_id.len());
    let mut matched_blobs = Vec::with_capacity(by_id.len());
    let mut missing = Vec::new();

    for id in requested {
        if let Some((event, blob)) = by_id.remove(id) {
            matched_events.push(event);
            matched_blobs.push(blob);
        } else {
            missing.push(*id);
        }
    }

    Ok((matched_events, matched_blobs, missing))
}

/// Merge one static-sync `EventRep` into the current batch state.
///
/// Returns the number of still-pending requested IDs satisfied by this
/// response. Invalid responses are rejected before any state is mutated.
pub(crate) fn merge_static_sync_event_rep(
    requested: &[blake3::Hash],
    pending: &mut HashSet<blake3::Hash>,
    known: &mut HashSet<blake3::Hash>,
    want: &mut HashSet<blake3::Hash>,
    fetched: &mut Vec<(Event, Vec<u8>)>,
    events: Vec<Event>,
    blobs: Vec<Vec<u8>>,
) -> Result<usize> {
    let (matched_events, matched_blobs, _) = filter_requested_event_rep(requested, events, blobs)?;
    let mut matched = 0;

    for (ev, blob) in matched_events.into_iter().zip(matched_blobs) {
        let eid = ev.id();
        if !pending.remove(&eid) {
            continue
        }

        matched += 1;
        if known.insert(eid) {
            for p in ev.header.parents.iter() {
                if *p != NULL_ID && !known.contains(p) {
                    want.insert(*p);
                }
            }
            fetched.push((ev, blob));
        }
    }

    Ok(matched)
}

/// The main Event Graph instance.
///
/// Manages a rolling window of DAGs (one per rotation period), a
/// static DAG for long-lived state (RLN identities), and the P2P
/// protocol for syncing with peers.
///
/// # Sync model
///
/// Headers are synced eagerly (complete DAG skeleton in seconds).
/// Event content is fetched lazily in the direction the application
/// needs.
///
/// The [`TimeIndex`] in each [`DagSlot`] enables O(log n)
/// bidirectional pagination that crosses DAG boundaries
/// transparently - the caller sees a flat chronological stream.
pub struct EventGraph {
    pub(crate) p2p: P2pPtr,
    pub(crate) dag_store: RwLock<DagStore>,
    /// Side-table mapping `event_id -> original RLN signal blob` for
    /// rotating-DAG events. Mirror of [`Self::static_dag_blobs`] but
    /// for the rotating DAGs.
    ///
    /// Populated by `handle_event_put` after successful RLN
    /// verification, and by `dag_insert_with_blobs` during sync when
    /// the serving peer included the blob in its `EventRep`. Read
    /// by `handle_event_req` to forward blobs to syncing peers.
    /// Pruned by `dag_prune` when the corresponding rotating DAG
    /// rolls out of the retention window.
    pub(crate) dag_blobs: sled::Tree,
    /// Historical SMT roots, in canonical apply order.
    ///
    /// Key: `(layer:u64_be, event_id:32) = 40 bytes`. Value:
    /// `(root:32, timestamp:u64_be:8) = 40 bytes`.
    ///
    /// Big-endian layer encoding makes lexicographic byte order
    /// match canonical apply order, so `Tree::range` iterates
    /// chronologically and `Tree::get_lt` / `get_gt` give cheap
    /// neighbor lookups (used to find the timestamp interval during
    /// which a given root was the live root).
    ///
    /// See [`Self::apply_rln_static_event`] for the canonical-order
    /// rationale and [`Self::is_root_valid_at`] for how this is
    /// consulted during signal verification.
    pub(crate) rln_historical_roots_ordered: sled::Tree,
    /// Reverse index: `(root:32, ordered_key:40) -> []`.
    ///
    /// A root can appear more than once when static events are no-ops
    /// (duplicate registrations, idempotent slashes), so the value
    /// index stores every canonical occurrence rather than a single
    /// root-to-key mapping. [`Self::is_root_valid_at`] scans this
    /// prefix and accepts if any interval for the root matches.
    pub(crate) rln_historical_roots_by_value: sled::Tree,
    pub(crate) static_dag: sled::Tree,
    /// Side-table mapping `event_id -> original RLN blob` for static
    /// events. Used by [`Self::static_sync`] to re-verify the ZK
    /// proof of historical events at sync time. Every static-DAG
    /// event MUST have a corresponding entry - `static_sync` rejects
    /// events whose blob isn't available rather than falling through.
    pub(crate) static_dag_blobs: sled::Tree,
    datastore: PathBuf,
    replay_mode: bool,
    pub(crate) broadcasted_ids: RwLock<HashSet<blake3::Hash>>,
    pub prune_task: OnceCell<StoppableTaskPtr>,
    pub event_pub: PublisherPtr<Event>,
    pub static_pub: PublisherPtr<Event>,
    pub current_genesis: RwLock<Event>,
    pub config: EventGraphConfig,
    /// Decoded app-provided pregenerated RLN commitments.
    pregenerated_identity_commitments: Vec<pallas::Base>,
    /// Canonical byte representations for fast admission checks.
    pregenerated_identity_commitment_reprs: HashSet<[u8; 32]>,
    pub synced: AtomicBool,
    pub deg_enabled: AtomicBool,
    deg_publisher: PublisherPtr<DegEvent>,
    pub sled_db: sled::Db,
    pub zk_keys: Arc<ZkKeys>,
    pub identity_state: RwLock<IdentityState>,
    pub rln_state: RwLock<RlnState>,
    /// App identifier mixed into the RLN external nullifier. Derived
    /// from `config.genesis_contents` so two deployments using the
    /// same circuit cannot collide on internal_nullifiers.
    rln_app_id: rln::RlnAppId,
}

impl EventGraph {
    /// Create a new Event Graph.
    pub async fn new(
        p2p: P2pPtr,
        sled_db: sled::Db,
        datastore: PathBuf,
        replay_mode: bool,
        config: EventGraphConfig,
        ex: Arc<Executor<'_>>,
    ) -> Result<EventGraphPtr> {
        config.validate()?;
        let zk_keys = Arc::new(ZkKeys::build_and_load(&sled_db)?);
        Self::with_zk_keys(p2p, sled_db, datastore, replay_mode, config, zk_keys, ex).await
    }

    /// Same as [`Self::new`] but accepts a pre-built [`ZkKeys`].
    ///
    /// Production always wants `Self::new`, which builds keys once
    /// against its own sled DB. Tests use this variant to share a
    /// single [`Arc<ZkKeys>`] across many `EventGraph` instances -
    /// proving keys are large (hundreds of MB each) and copying
    /// them per-test would blow out RAM and `/dev/shm`.
    pub async fn with_zk_keys(
        p2p: P2pPtr,
        sled_db: sled::Db,
        datastore: PathBuf,
        replay_mode: bool,
        config: EventGraphConfig,
        zk_keys: Arc<ZkKeys>,
        ex: Arc<Executor<'_>>,
    ) -> Result<EventGraphPtr> {
        config.validate()?;
        let identity_state = IdentityState::new(&sled_db)?;
        let rln_app_id = rln::RlnAppId::from_genesis(&config.genesis_contents);
        let current_genesis = generate_genesis(&config);
        let (pregenerated_identity_commitments, pregenerated_identity_commitment_reprs) =
            validate_pregenerated_identity_commitments(&config)?;
        let dag_store = DagStore::new(sled_db.clone(), &config).await?;
        let static_dag = Self::static_new(&sled_db, &config).await?;
        let static_dag_blobs = sled_db.open_tree("static-dag-blobs")?;
        let dag_blobs = sled_db.open_tree("dag-blobs")?;

        // Historical-roots side-tables. See the design comment on
        // `EventGraph::apply_rln_static_event` for the full rationale.
        // In short: every static-DAG mutation produces a new SMT root,
        // and we need to recognize *any* historical root for sync-time
        // signal verification, not just the most recent N. The
        // `ordered` tree gives us canonical replay (and successor
        // lookup for the time-window check), the `by_value` tree
        // gives us O(log n) "is this root historical?" queries.
        let rln_historical_roots_ordered = sled_db.open_tree("rln-historical-roots-ordered")?;
        let rln_historical_roots_by_value = sled_db.open_tree("rln-historical-roots-by-value")?;

        // Check whether the current genesis event is already in the
        // store. If not, we need to prune (create a fresh slot).
        let dag_ts = current_genesis.header.timestamp;
        let need_prune = dag_store
            .get_slot(&dag_ts)
            .map(|s| !s.main_tree.contains_key(current_genesis.id().as_bytes()).unwrap_or(false))
            .unwrap_or(true);

        let self_ = Arc::new(Self {
            p2p,
            sled_db: sled_db.clone(),
            dag_store: RwLock::new(dag_store),
            static_dag,
            static_dag_blobs,
            dag_blobs,
            rln_historical_roots_ordered,
            rln_historical_roots_by_value,
            datastore,
            replay_mode,
            broadcasted_ids: RwLock::new(HashSet::new()),
            prune_task: OnceCell::new(),
            event_pub: Publisher::new(),
            static_pub: Publisher::new(),
            current_genesis: RwLock::new(current_genesis.clone()),
            config: config.clone(),
            pregenerated_identity_commitments,
            pregenerated_identity_commitment_reprs,
            synced: AtomicBool::new(false),
            deg_enabled: AtomicBool::new(false),
            deg_publisher: Publisher::new(),
            zk_keys,
            identity_state: RwLock::new(identity_state),
            rln_state: RwLock::new(RlnState::new()),
            rln_app_id,
        });

        // Init genesis registration events
        if config.hours_rotation > 0 {
            self_.bootstrap_genesis_identities().await?;
        }

        if need_prune {
            info!(
                target: "event_graph::new",
                "[EVENTGRAPH] Pruning: current genesis not found",
            );
            self_.dag_prune(current_genesis).await?;
        }

        // Consistency check: if the static DAG has events but the
        // historical-roots tables are empty, rebuild them by
        // replaying the static DAG in canonical order. This handles
        // the case where the operator manually deleted the
        // historical-roots trees, or where this is the first startup
        // after upgrading from a version that didn't track them.
        //
        // Without this, signal verification would fail for any root
        // beyond the in-memory `recent_roots` window.
        self_.rebuild_historical_roots_if_needed().await?;

        if config.hours_rotation > 0 {
            let task = StoppableTask::new();
            let _ = self_.prune_task.set(task.clone()).await;
            task.clone().start(
                self_.clone().dag_prune_task(),
                |res| async move {
                    if let Err(e) = res {
                        if !matches!(e, Error::DetachedTaskStopped) {
                            error!("Prune: {e}");
                        }
                    }
                },
                Error::DetachedTaskStopped,
                ex,
            );
        }
        Ok(self_)
    }

    /// Rebuild the historical-roots side-tables from the static DAG.
    ///
    /// Called once at startup. No-op if the historical-roots tables
    /// already match the static-DAG event count. Otherwise replays
    /// every static-DAG event in canonical `(layer, event_id)` order
    /// and re-records the post-mutation root for each one.
    ///
    /// **Side effect.** Resets the in-memory SMT to empty, then
    /// rebuilds it leaf-by-leaf in canonical order, so the SMT and
    /// the historical-roots tables come out consistent. The
    /// `rln-identity-leaves` tree (which `IdentityState::new`
    /// originally read) is implicitly re-derived; we don't read it
    /// during rebuild because we want to honor any slashes in the
    /// static DAG even if the leaves tree is stale.
    async fn rebuild_historical_roots_if_needed(self: &Arc<Self>) -> Result<()> {
        // Walk the static DAG once, computing both:
        //   * static_count: total non-genesis events
        //   * expected_leaves: registrations - slashes (the number
        //     of identities that should currently be in the SMT)
        // We need the second one to detect a state where leaves and
        // historical-roots happen to share counts but the leaves
        // don't actually correspond to the static-DAG events. That
        // can happen across schema changes or when older code paths
        // wrote to leaves without going through `apply_rln_static_event`.
        let mut static_count: u64 = 0;
        let mut registrations: i64 = 0;
        let mut slashes: i64 = 0;
        for item in self.static_dag.iter() {
            let (_, val) = item?;
            let ev: Event = deserialize_async(&val).await?;
            if ev.header.parents == NULL_PARENTS {
                continue
            }
            static_count += 1;
            // Try to classify this event. We tolerate failed parses
            // here because the rebuild path is best-effort: if an
            // event's content is unparseable, we just don't count it
            // toward expected_leaves. The replay loop below skips
            // it for the same reason.
            if let Ok((node, _)) = deserialize_async_partial::<rln::RLNNode>(ev.content()).await {
                match node {
                    rln::RLNNode::Registration(_) => registrations += 1,
                    rln::RLNNode::Slashing(_) => slashes += 1,
                }
            }
        }
        let expected_leaves = (registrations - slashes).max(0) as usize;

        let recorded_count = self.rln_historical_roots_ordered.len() as u64;
        let actual_leaves = self.identity_state.read().await.leaves_count();

        let counts_consistent = recorded_count == static_count;
        let leaves_consistent = actual_leaves == expected_leaves;

        info!(
            target: "event_graph::new",
            "[EVENTGRAPH] RLN state audit: static_count={} recorded_count={} \
             actual_leaves={} expected_leaves={} consistent={}",
            static_count, recorded_count, actual_leaves, expected_leaves,
            counts_consistent && leaves_consistent,
        );

        if counts_consistent && leaves_consistent {
            // Already consistent across all three sources (static
            // DAG, historical-roots table, leaves tree).
            return Ok(())
        }

        info!(
            target: "event_graph::new",
            "[EVENTGRAPH] Rebuilding historical-roots: {} static events, \
             {} recorded roots, {} leaves (expected {})",
            static_count, recorded_count, actual_leaves, expected_leaves,
        );

        // Reset the historical-roots tables to a known-empty state.
        self.rln_historical_roots_ordered.clear()?;
        self.rln_historical_roots_by_value.clear()?;

        // Reset the in-memory SMT and the leaves tree so the replay
        // below builds it correctly from the canonical static-DAG
        // sequence (including any slashes).
        {
            let mut state = self.identity_state.write().await;
            state.clear_for_rebuild()?;
        }

        // Collect static-DAG events and sort canonically.
        let mut events: Vec<Event> = vec![];
        for item in self.static_dag.iter() {
            let (_, val) = item?;
            let ev: Event = deserialize_async(&val).await?;
            if ev.header.parents != NULL_PARENTS {
                events.push(ev);
            }
        }
        events.sort_by(|a, b| {
            a.header
                .layer
                .cmp(&b.header.layer)
                .then_with(|| a.id().as_bytes().cmp(b.id().as_bytes()))
        });

        // Replay each event through the canonical apply path.
        for ev in events {
            let rln_node: rln::RLNNode = match deserialize_async_partial(ev.content()).await {
                Ok((v, _)) => v,
                Err(_) => continue,
            };
            let _ = self.apply_rln_static_event(&ev, &rln_node).await?;
        }

        info!(
            target: "event_graph::new",
            "[EVENTGRAPH] Historical-roots rebuild complete",
        );

        Ok(())
    }

    /// After header sync, event content can be fetched lazily via
    /// [`fetch_page`] or peer [`RangeReq`] - the application pulls
    /// the events it actually wants to display or process, without
    /// downloading the entire content on every sync.
    pub async fn dag_sync_headers(&self, dag_ts: u64) -> Result<()> {
        self.sync_impl(dag_ts, false).await
    }

    /// Full sync: headers plus all event content currently in the DAG.
    ///
    /// Use this when the application wants the complete historical
    /// content (e.g. an archive node, or a node rebuilding local state
    /// from the full event stream).
    pub async fn dag_sync(&self, dag_ts: u64) -> Result<()> {
        self.sync_impl(dag_ts, true).await
    }

    async fn sync_impl(&self, dag_ts: u64, fetch_content: bool) -> Result<()> {
        let dag_name = dag_ts.to_string();
        let channels = self.p2p.hosts().peers();
        // We need at least one peer to ask
        if channels.is_empty() {
            return Err(Error::DagSyncFailed)
        }
        let timeout = self.p2p.settings().read().await.outbound_connect_timeout_max();

        // Parallel tip collection
        let mut futs = FuturesUnordered::new();
        for ch in channels.iter() {
            futs.push(request_tips(ch, dag_name.clone(), timeout));
        }
        let mut tips: HashMap<blake3::Hash, (u64, usize)> = HashMap::new();
        let mut responded = 0usize;
        while let Some(res) = futs.next().await {
            if let Ok(peer_tips) = res {
                responded += 1;
                for (layer, hashes) in &peer_tips {
                    for h in hashes {
                        tips.entry(*h).and_modify(|e| e.1 += 1).or_insert((*layer, 1));
                    }
                }
            }
        }
        if tips.is_empty() {
            return Err(Error::DagSyncFailed)
        }

        // 2/3 quorum
        let threshold = (responded * 2).div_ceil(3);
        let accepted: HashSet<blake3::Hash> = tips
            .iter()
            .filter(|(h, (_, n))| **h != NULL_ID && *n >= threshold)
            .map(|(h, _)| *h)
            .collect();

        let store = self.dag_store.read().await;
        let slot = store.get_slot(&dag_ts).ok_or(Error::DagSyncFailed)?;
        let missing: HashSet<blake3::Hash> = accepted
            .iter()
            .filter(|h| !slot.main_tree.contains_key(h.as_bytes()).unwrap_or(true))
            .cloned()
            .collect();
        if missing.is_empty() {
            return Ok(())
        }
        let our_tips = slot.tips.clone();
        drop(store);

        // Parallel header sync
        let mut hfuts = FuturesUnordered::new();
        for ch in channels.iter() {
            hfuts.push(request_header(ch, dag_name.clone(), our_tips.clone(), timeout));
        }
        while let Some(res) = hfuts.next().await {
            if let Ok(hdrs) = res {
                self.header_dag_insert(hdrs, &dag_name).await?;
            }
        }

        if fetch_content {
            self.fetch_missing_events(dag_ts, &dag_name, timeout).await?;
        }
        Ok(())
    }

    async fn fetch_missing_events(&self, dag_ts: u64, dag_name: &str, timeout: u64) -> Result<()> {
        let store = self.dag_store.read().await;
        let slot = store.get_slot(&dag_ts).ok_or(Error::DagSyncFailed)?;
        let mut sorted = vec![];
        for item in slot.header_tree.iter() {
            let (hb, val) = item.unwrap();
            let hdr: Header = deserialize_async(&val).await.unwrap();
            if hdr.parents != NULL_PARENTS && !slot.main_tree.contains_key(hb)? {
                sorted.push(hdr);
            }
        }
        sorted.sort_by_key(|h| h.layer);
        drop(store);
        if sorted.is_empty() {
            return Ok(())
        }

        let batch = 20;
        let mut chunks: BTreeMap<usize, Vec<blake3::Hash>> = BTreeMap::new();
        for (i, c) in sorted.chunks(batch).enumerate() {
            chunks.insert(i, c.iter().map(|h| h.id()).collect());
        }
        let mut remaining: BTreeSet<usize> = chunks.keys().cloned().collect();
        let mut peer_st: HashMap<Url, PeerStatus> = HashMap::new();
        let mut count = 0;
        let mut fs = FuturesUnordered::new();
        // Collected by event ID so partial chunk retries cannot disturb
        // the final layer-sorted insertion order. Empty `Vec<u8>` entries
        // mean "this event has no blob from the serving peer".
        let mut received: HashMap<blake3::Hash, (Event, Vec<u8>)> = HashMap::new();

        while count < sorted.len() {
            let mut free = vec![];
            let mut busy = 0;
            self.p2p.hosts().peers().iter().for_each(|ch| match peer_st.get(ch.address()) {
                Some(PeerStatus::Free) | None => {
                    free.push(ch.clone());
                }
                Some(PeerStatus::Busy) => {
                    busy += 1;
                }
                _ => {}
            });
            if free.is_empty() && busy == 0 {
                return Err(Error::DagSyncFailed)
            }
            if remaining.is_empty() && fs.is_empty() {
                return Err(Error::DagSyncFailed)
            }
            let n = std::cmp::min(free.len(), remaining.len());
            let ids: Vec<usize> = remaining.iter().take(n).copied().collect();
            for (i, cid) in ids.iter().enumerate() {
                fs.push(request_event(free[i].clone(), chunks[cid].clone(), *cid, timeout));
                remaining.remove(cid);
                peer_st.insert(free[i].address().clone(), PeerStatus::Busy);
            }
            if let Some((evts, cid, ch)) = fs.next().await {
                if let Ok((events, blobs)) = evts {
                    let Some(requested) = chunks.get(&cid) else {
                        peer_st.insert(ch.address().clone(), PeerStatus::Failed);
                        continue
                    };

                    match filter_requested_event_rep(requested, events, blobs) {
                        Ok((matched_events, matched_blobs, missing)) => {
                            let matched = matched_events.len();
                            for (event, blob) in matched_events.into_iter().zip(matched_blobs) {
                                let event_id = event.id();
                                if received.insert(event_id, (event, blob)).is_none() {
                                    count += 1;
                                }
                            }

                            if missing.is_empty() {
                                peer_st.insert(ch.address().clone(), PeerStatus::Free);
                            } else {
                                chunks.insert(cid, missing);
                                remaining.insert(cid);
                                let status = if matched == 0 {
                                    PeerStatus::Failed
                                } else {
                                    PeerStatus::Free
                                };
                                peer_st.insert(ch.address().clone(), status);
                            }
                        }
                        Err(_) => {
                            remaining.insert(cid);
                            peer_st.insert(ch.address().clone(), PeerStatus::Failed);
                        }
                    }
                } else {
                    remaining.insert(cid);
                    peer_st.insert(ch.address().clone(), PeerStatus::Failed);
                }
            }
        }

        let mut events = Vec::with_capacity(sorted.len());
        let mut blobs = Vec::with_capacity(sorted.len());
        for hdr in sorted {
            let event_id = hdr.id();
            let Some((event, blob)) = received.remove(&event_id) else {
                return Err(Error::DagSyncFailed)
            };
            events.push(event);
            blobs.push(blob);
        }

        // dag_insert_with_blobs handles RLN re-verification per event when
        // blobs are present, and falls through to the trust-the-quorum path
        // when they're not. See dag_insert_with_blobs's docstring for the
        // policy.
        self.dag_insert_with_blobs(&events, &blobs, dag_name).await?;
        Ok(())
    }

    /// Sync the `count` most recent DAGs (full content).
    ///
    /// Iterates oldest-first so that later syncs build on earlier
    /// ones (parent events exist before children reference them).
    pub async fn sync_selected(&self, count: usize) -> Result<()> {
        let ts: Vec<u64> =
            self.dag_store.read().await.dag_timestamps().into_iter().rev().take(count).collect();
        for t in ts.into_iter().rev() {
            self.dag_sync(t).await?;
        }
        self.synced.store(true, Ordering::Release);
        Ok(())
    }

    /// Sync only headers for the `count` most recent DAGs.
    ///
    /// Fast variant - gives a full DAG skeleton without downloading
    /// event bodies. Pair with [`fetch_page`] to pull content on-demand.
    pub async fn sync_selected_headers(&self, count: usize) -> Result<()> {
        let ts: Vec<u64> =
            self.dag_store.read().await.dag_timestamps().into_iter().rev().take(count).collect();
        for t in ts.into_iter().rev() {
            self.dag_sync_headers(t).await?;
        }
        self.synced.store(true, Ordering::Release);
        Ok(())
    }

    /// Sync the static DAG from peers.
    ///
    /// The static DAG holds RLN identity events (registrations and
    /// slashes). It is *persistent* across rotation windows - unlike
    /// rotating DAGs, events are never pruned - and has no separate
    /// `header_tree`, so it uses a different sync strategy:
    ///
    /// 1. Ask every peer for their `"static-dag"` tips.
    /// 2. Take the tips that reach a 2/3 quorum.
    /// 3. BFS-fetch the events and their ancestors directly via
    ///    `EventReq` until the entire reachable subgraph is local.
    ///
    /// Peers serve static-DAG event requests even when the IDs are
    /// not in their `broadcasted_ids` set (see the relaxation in
    /// `handle_event_req`), because static-DAG state is public
    /// consensus information. Registration-event proof verification,
    /// duplicate detection, and commitment-tree updates are all done
    /// through the normal `StaticPut` ingestion path
    /// (`handle_static_put`) - but `static_sync` uses direct-insert
    /// via [`Self::static_insert`] plus on-the-fly identity-state
    /// application, because we're catching up rather than processing
    /// broadcasts.
    ///
    /// Note: for security, this method ONLY applies events whose
    /// blob/RLN verification passes. We do not trust peers blindly
    /// on historical state - proofs are re-verified locally for
    /// every single event before its effect is merged into the
    /// identity tree. This is the same discipline `handle_static_put`
    /// uses; see [`Self::rln_verify_static_event`].
    pub async fn static_sync(&self) -> Result<()> {
        static DAG_NAME: &str = "static-dag";

        let channels = self.p2p.hosts().peers();
        if channels.is_empty() {
            return Err(Error::DagSyncFailed)
        }
        let timeout = self.p2p.settings().read().await.outbound_connect_timeout_max();

        // Step 1: gather tips from every peer in parallel.
        let mut tip_futs = FuturesUnordered::new();
        for ch in channels.iter() {
            tip_futs.push(request_tips(ch, DAG_NAME.to_string(), timeout));
        }

        let mut tip_counts: HashMap<blake3::Hash, usize> = HashMap::new();
        let mut responded = 0usize;
        while let Some(res) = tip_futs.next().await {
            if let Ok(peer_tips) = res {
                responded += 1;
                for hashes in peer_tips.values() {
                    for h in hashes {
                        *tip_counts.entry(*h).or_insert(0) += 1;
                    }
                }
            }
        }

        // If no peer answered we have nothing to do. An empty
        // network-side static DAG is a valid state (brand new app
        // deployment), so we return Ok rather than error.
        if responded == 0 {
            info!(
                target: "event_graph::static_sync",
                "[STATIC_SYNC] no peer responded to TipReq; nothing to sync"
            );
            return Ok(())
        }

        // Step 2: take tips at 2/3 quorum. This matches the
        // threshold used in `sync_impl`.
        let threshold = (responded * 2).div_ceil(3);
        let total_distinct_tips = tip_counts.len();
        let tip_ids: HashSet<blake3::Hash> = tip_counts
            .into_iter()
            .filter(|(h, n)| *h != NULL_ID && *n >= threshold)
            .map(|(h, _)| h)
            .collect();

        // What's already local?
        let mut known: HashSet<blake3::Hash> = HashSet::new();
        for item in self.static_dag.iter() {
            let (k, _) = item?;
            if let Ok(bytes) = <[u8; 32]>::try_from(&k as &[u8]) {
                known.insert(blake3::Hash::from_bytes(bytes));
            }
        }

        info!(
            target: "event_graph::static_sync",
            "[STATIC_SYNC] peers_responded={} threshold={} distinct_tips_seen={} \
             tip_ids_quorum={} known_local={}",
            responded, threshold, total_distinct_tips, tip_ids.len(), known.len(),
        );

        // Step 3: BFS from the quorum tips, fetching events we
        // don't have. Any event we pull in may reference ancestors
        // we ALSO don't have; enqueue them and keep going until the
        // frontier is empty.
        //
        // Bounded at SYNC_MAX_STATIC_EVENTS (defined at module level)
        // to defend against a malicious peer who serves a fabricated
        // deep-ancestry chain. In practice static DAGs are small (one
        // event per registration / slash), so this bound is
        // comfortably above any real deployment's size.

        let mut want: HashSet<blake3::Hash> = tip_ids.difference(&known).copied().collect();
        // Events fetched during BFS, paired with their blobs (empty
        // Vec if the peer didn't have the blob - see EventRep
        // docstring). Index alignment is preserved through the
        // entire pipeline up to the apply loop.
        let mut fetched: Vec<(Event, Vec<u8>)> = vec![];

        while !want.is_empty() {
            want.retain(|id| !known.contains(id));
            if want.is_empty() {
                break
            }

            if fetched.len() >= SYNC_MAX_STATIC_EVENTS {
                error!(
                    target: "event_graph::static_sync",
                    "[STATIC_SYNC] reached {} event cap; aborting",
                    SYNC_MAX_STATIC_EVENTS,
                );
                return Err(Error::DagSyncFailed)
            }

            let batch: Vec<blake3::Hash> = want.iter().take(MAX_EVENT_REQ_IDS).copied().collect();
            for id in &batch {
                want.remove(id);
            }
            let mut pending: HashSet<blake3::Hash> = batch.iter().copied().collect();

            // Ask every peer for the same batch. We keep consuming
            // responses until the batch is complete or every peer has
            // failed to help. Irrelevant, duplicate, or blob-misaligned
            // replies do not satisfy the request.
            let mut req_futs = FuturesUnordered::new();
            for (i, ch) in channels.iter().enumerate() {
                req_futs.push(request_event(ch.clone(), batch.clone(), i, timeout));
            }

            let mut made_progress = false;
            while !pending.is_empty() {
                let Some((res, _, _)) = req_futs.next().await else { break };
                let Ok((evs, blobs)) = res else { continue };
                if evs.is_empty() {
                    continue
                }

                let Ok(matched) = merge_static_sync_event_rep(
                    &batch,
                    &mut pending,
                    &mut known,
                    &mut want,
                    &mut fetched,
                    evs,
                    blobs,
                ) else {
                    continue
                };

                if matched > 0 {
                    made_progress = true;
                }
            }

            if !pending.is_empty() {
                want.extend(pending.iter().copied().filter(|id| !known.contains(id)));
            }

            want.retain(|id| !known.contains(id));

            if !made_progress {
                // Nobody responded usefully. Give up so we don't
                // loop forever on an unreachable ancestor.
                error!(
                    target: "event_graph::static_sync",
                    "[STATIC_SYNC] no peer served requested events; aborting",
                );
                return Err(Error::DagSyncFailed)
            }
        }

        // Step 4: canonical-order the fetched events so all nodes
        // produce the same intermediate SMT roots. Primary key:
        // layer (matches DAG topology). Secondary key: event_id
        // (32-byte hash, lexicographic byte order is total). Without
        // the tie-breaker, two events at the same layer could be
        // applied in different orders on different nodes, producing
        // different intermediate roots and breaking sync-time signal
        // verification. See the design comment on
        // `apply_rln_static_event` for the full rationale.
        fetched.sort_by(|(a, _), (b, _)| {
            a.header
                .layer
                .cmp(&b.header.layer)
                .then_with(|| a.id().as_bytes().cmp(b.id().as_bytes()))
        });

        // Track the apply-loop outcome for the summary log.
        let mut applied = 0usize;
        let mut already_present = 0usize;
        let mut blob_missing = 0usize;
        let mut rejected = 0usize;
        let mut structural_invalid = 0usize;
        let mut content_unparseable = 0usize;
        let total_to_consider = fetched.len();

        for (ev, blob) in fetched {
            // Skip if someone else inserted it concurrently.
            if self.static_dag.contains_key(ev.id().as_bytes())? {
                already_present += 1;
                continue
            }

            // Structural validation always runs. Static-DAG events
            // are persistent and may be far older than the 60s drift
            // window allowed by `validate_new`; use the static
            // sibling that omits the freshness check while keeping
            // the structural ones.
            if !ev.validate_new_static() {
                structural_invalid += 1;
                continue
            }
            let rln_node: rln::RLNNode = match deserialize_async_partial(ev.content()).await {
                Ok((v, _)) => v,
                Err(_) => {
                    content_unparseable += 1;
                    continue
                }
            };

            // RLN verification is mandatory. A non-genesis static
            // event without a blob during sync is treated as
            // misbehavior: either the serving peer is buggy or
            // adversarial, or the originator never persisted the blob
            // (which itself is a protocol violation). Skip with a
            // loud log - we don't strike here because static_sync
            // doesn't have a single peer to attribute the failure
            // to (the quorum collected blobs from multiple peers).
            if blob.is_empty() {
                blob_missing += 1;
                error!(
                    target: "event_graph::static_sync",
                    "[STATIC_SYNC] no blob available for static event {}; skipping. \
                     Every static-DAG event must carry an RLN blob.",
                    ev.id(),
                );
                continue
            }

            let outcome = self.rln_verify_static_event(&rln_node, &blob, ev.header.timestamp).await;
            match outcome {
                rln::StaticEventCheck::AcceptedRegistration(_) |
                rln::StaticEventCheck::AcceptedSlash(_) => {
                    // apply_rln_static_event handles both Registration
                    // and Slashing branches and also records the
                    // post-mutation root in the historical-roots
                    // side-tables.
                    let _ = self.apply_rln_static_event(&ev, &rln_node).await;
                    self.static_blob_store(&ev.id(), &blob)?;
                    self.static_insert(&ev).await?;
                    applied += 1;
                }
                rln::StaticEventCheck::Rejected | rln::StaticEventCheck::Malicious => {
                    // A historical event whose blob fails
                    // re-verification despite being held by the 2/3
                    // quorum is a serious finding - either the blob
                    // was tampered with, the quorum was compromised,
                    // or our verifying keys diverged. Log loudly and
                    // skip.
                    rejected += 1;
                    error!(
                        target: "event_graph::static_sync",
                        "[STATIC_SYNC] historical blob FAILED re-verification for event {}: {:?}; \
                         skipping event despite quorum inclusion",
                        ev.id(),
                        outcome,
                    );
                }
            }
        }

        info!(
            target: "event_graph::static_sync",
            "[STATIC_SYNC] complete: fetched={} applied={} already_present={} \
             blob_missing={} verification_rejected={} structural_invalid={} \
             unparseable={}",
            total_to_consider, applied, already_present, blob_missing, rejected,
            structural_invalid, content_unparseable,
        );

        Ok(())
    }

    /// Fetch a page of events, crossing DAG boundaries transparently.
    pub async fn fetch_page(
        &self,
        cursor_ts: u64,
        dir: SyncDirection,
        limit: usize,
    ) -> Result<Vec<Event>> {
        let limit = limit.min(MAX_RANGE_PAGE_SIZE);
        let mut out = vec![];
        let store = self.dag_store.read().await;
        let slots: Vec<_> = match dir {
            SyncDirection::Forward => store.dags.iter().collect(),
            SyncDirection::Backward => store.dags.iter().rev().collect(),
        };
        for (_, slot) in slots {
            if out.len() >= limit {
                break
            }
            let rem = limit - out.len();
            let ids = match dir {
                SyncDirection::Forward => slot.time_index.after(cursor_ts, rem),
                SyncDirection::Backward => slot.time_index.before(cursor_ts, rem),
            };
            for id in ids {
                if let Some(bytes) = slot.main_tree.get(id.as_bytes())? {
                    out.push(deserialize_async(&bytes).await?);
                }
            }
        }
        out.truncate(limit);
        Ok(out)
    }

    async fn dag_prune(&self, genesis: Event) -> Result<()> {
        let mut bcast = self.broadcasted_ids.write().await;
        let mut cur = self.current_genesis.write().await;

        // Before the DAG store evicts the oldest DAG (which would
        // drop its main_tree), enumerate the about-to-be-dropped
        // event IDs so we can remove their blobs from `dag_blobs`.
        // Without this, blob entries would orphan and accumulate
        // forever - the side-table is not bounded by the rotation
        // window on its own.
        if let Some(limit) = self.config.max_dags {
            let store = self.dag_store.read().await;
            if store.dags.len() >= limit {
                if let Some((_, oldest)) = store.dags.iter().next() {
                    for item in oldest.main_tree.iter() {
                        let (eid, _) = match item {
                            Ok(v) => v,
                            Err(_) => continue,
                        };
                        let _ = self.dag_blobs.remove(&eid);
                    }
                }
            }
        }

        self.dag_store.write().await.add_dag(&genesis, self.config.max_dags).await?;
        *cur = genesis;
        *bcast = HashSet::new();
        Ok(())
    }

    async fn dag_prune_task(self: Arc<Self>) -> Result<()> {
        loop {
            let next =
                next_rotation_timestamp(self.config.initial_genesis, self.config.hours_rotation)?;
            let hdr = Header {
                timestamp: next,
                parents: NULL_PARENTS,
                layer: 0,
                content_hash: blake3::hash(&self.config.genesis_contents),
            };
            let genesis = Event { header: hdr, content: self.config.genesis_contents.clone() };
            msleep(millis_until_next_rotation(next)?).await;
            self.dag_prune(genesis).await?;
        }
    }

    /// Insert events into a rotating DAG **without RLN verification**.
    ///
    /// This is the post-verification entry point for callers that
    /// have already verified the proof separately. Two legitimate
    /// callers in production:
    ///
    /// * `handle_event_put` - already ran `rln_verify_signal` and
    ///   recorded the share. Calling `dag_insert_with_blobs` would
    ///   trigger the duplicate-share rejection.
    /// * The IRC client's own outbound flow - same shape.
    pub async fn dag_insert(&self, events: &[Event], dag_name: &str) -> Result<Vec<blake3::Hash>> {
        // Implementation just runs the structural-insert path;
        // dag_insert_with_blobs reaches the same shared inner code
        // when called with a `skip_verify=true` shortcut, which is
        // what an empty `blobs` slice now means after the strictness
        // tightening below - but only via this private wrapper.
        self.dag_insert_inner(events, &[], /* require_blobs */ false, dag_name).await
    }

    /// Insert events into a rotating DAG, with mandatory RLN
    /// verification.
    ///
    /// `blobs` is index-aligned with `events`. Every non-genesis
    /// event MUST have a non-empty `blobs[i]`; events that don't
    /// (whether `blobs` is empty, shorter, or has an empty entry
    /// at position `i`) are rejected with a loud log. This is the
    /// strict policy required for sync paths - a peer that serves
    /// an event without its blob is buggy or adversarial.
    ///
    /// On `Slashable`, this method does NOT broadcast a slash -
    /// that's the protocol layer's job (see
    /// `proto::handle_event_put::verify_rln_signal`). Sync-time
    /// detection of a slashable conflict simply skips the event.
    /// We don't want a node coming online to flood the network
    /// with stale slash broadcasts.
    pub async fn dag_insert_with_blobs(
        &self,
        events: &[Event],
        blobs: &[Vec<u8>],
        dag_name: &str,
    ) -> Result<Vec<blake3::Hash>> {
        self.dag_insert_inner(events, blobs, /* require_blobs */ true, dag_name).await
    }

    /// Inner implementation shared by both insert paths. The
    /// `require_blobs` flag selects strict (sync) vs. lenient
    /// (post-verified) semantics.
    async fn dag_insert_inner(
        &self,
        events: &[Event],
        blobs: &[Vec<u8>],
        require_blobs: bool,
        dag_name: &str,
    ) -> Result<Vec<blake3::Hash>> {
        if events.is_empty() {
            return Ok(vec![])
        }

        // Pre-flight RLN verification. Done BEFORE acquiring the
        // DAG-store write lock so a slow proof verification doesn't
        // hold up other inserts.
        //
        // Events we already have are skipped without verification.
        // This matters because `rln_verify_signal` records the share
        // on `Accepted`, and re-running it for an already-seen event
        // would trip its duplicate-share check (returning `Rejected`)
        // - which would be incorrect: the event is legitimate, we
        // just already know about it.
        let dag_ts = u64::from_str(dag_name)?;
        let already_have: Vec<bool> = {
            let store = self.dag_store.read().await;
            let slot = store.get_slot(&dag_ts);
            events
                .iter()
                .map(|ev| match slot {
                    Some(s) => s.main_tree.contains_key(ev.id().as_bytes()).unwrap_or(false),
                    None => false,
                })
                .collect()
        };

        let mut accepted: Vec<usize> = Vec::with_capacity(events.len());
        for (i, ev) in events.iter().enumerate() {
            // Already-known events go through structurally (the
            // downstream `contains_key` check will skip them) but
            // skip the RLN verifier to avoid double-recording the
            // share for the same (epoch, internal_nullifier, x, y)
            // tuple.
            if already_have[i] {
                accepted.push(i);
                continue
            }
            // Genesis-shaped events have no blob and no proof -
            // they're consensus inputs, not user signals.
            if ev.header.parents == NULL_PARENTS {
                accepted.push(i);
                continue
            }
            let blob = blobs.get(i).cloned().unwrap_or_default();
            if blob.is_empty() {
                if require_blobs {
                    error!(
                        target: "event_graph::dag_insert",
                        "[DAG_INSERT] sync event {} arrived without an RLN blob; rejecting. \
                         Every non-genesis rotating-DAG event must carry a blob.",
                        ev.id(),
                    );
                    continue
                }
                // Lenient path: caller pre-verified. Accept the
                // event structurally without running the RLN
                // verifier on it.
                accepted.push(i);
                continue
            }
            match self.rln_verify_signal(ev, &blob).await {
                rln::SignalCheck::Accepted => accepted.push(i),
                rln::SignalCheck::Rejected => {
                    error!(
                        target: "event_graph::dag_insert",
                        "[DAG_INSERT] sync event {} failed RLN re-verification; skipping",
                        ev.id(),
                    );
                }
                rln::SignalCheck::Slashable(_) => {
                    // The conflicting share is recorded inside
                    // `rln_verify_signal` ONLY on `Accepted`. On
                    // `Slashable` it returns the conflicting shares
                    // *without* mutating metadata, so we don't
                    // double-record. We don't broadcast a slash
                    // here - that's the live broadcast handler's
                    // job. We just skip the event.
                    error!(
                        target: "event_graph::dag_insert",
                        "[DAG_INSERT] sync event {} is slashable (slot reuse); skipping",
                        ev.id(),
                    );
                }
            }
        }

        let mut bcast = self.broadcasted_ids.write().await;
        let mut store = self.dag_store.write().await;
        let slot = store.get_slot_mut(&dag_ts).ok_or(Error::DagSyncFailed)?;

        let mut ids = Vec::with_capacity(accepted.len());
        let mut overlay = SledTreeOverlay::new(&slot.main_tree);

        for &i in &accepted {
            let ev = &events[i];
            let eid = ev.id();
            if ev.header.parents == NULL_PARENTS {
                continue
            }
            if slot.main_tree.contains_key(eid.as_bytes())? {
                continue
            }
            if !slot.header_tree.contains_key(eid.as_bytes())? {
                continue
            }
            if !ev.dag_validate(&slot.header_tree, &self.config, dag_ts).await? {
                return Err(Error::EventIsInvalid)
            }
            let se = serialize_async(ev).await;
            overlay.insert(eid.as_bytes(), &se)?;
            if self.replay_mode {
                replayer_log(&self.datastore, "insert".into(), se)?;
            }

            // Persist the blob alongside the event for future
            // sync-time re-verification by other late-joiners.
            if let Some(blob) = blobs.get(i) {
                if !blob.is_empty() {
                    let _ = self.dag_blob_store(&eid, blob);
                }
            }

            ids.push(eid);
        }

        if let Some(b) = overlay.aggregate() {
            slot.main_tree.apply_batch(b).unwrap();
        } else {
            return Ok(vec![])
        }

        for &i in &accepted {
            let ev = &events[i];
            let eid = ev.id();
            if ev.header.parents == NULL_PARENTS {
                continue
            }
            for pid in ev.header.parents.iter() {
                if *pid != NULL_ID {
                    for (layer, tips) in slot.tips.iter_mut() {
                        if *layer < ev.header.layer {
                            tips.remove(pid);
                        }
                    }
                    bcast.insert(*pid);
                }
            }
            slot.tips.retain(|_, t| !t.is_empty());
            slot.tips.entry(ev.header.layer).or_default().insert(eid);
            self.event_pub.notify(ev.clone()).await;
        }

        Ok(ids)
    }

    pub async fn header_dag_insert(&self, headers: Vec<Header>, dag_name: &str) -> Result<()> {
        let dag_ts = u64::from_str(dag_name)?;

        // The genesis ID we expect any layer-1 header in this slot
        // to reference. Computed locally from config - two networks
        // with different `genesis_contents` (or any other config
        // mismatch) produce different genesis ids, so a peer whose
        // layer-1 headers reference something else is on a different
        // network. Catching this explicitly here is strictly a
        // defense-in-depth and diagnostics improvement: the existing
        // parent-existence check in `Header::validate` already
        // rejects these (genesis headers are filtered from
        // `header_tree` on insert, so a foreign genesis id never
        // lands in the local tree). The explicit boundary check just
        // turns "HeaderIsInvalid" into a logged, named condition, so
        // an operator debugging a misconfigured deployment sees
        // "peer is on a different network" instead of a generic
        // header rejection.
        //
        // Why layer 1 is sufficient: `select_parents_from_tips` puts
        // an event at layer N+1 where N is the highest layer with
        // tips. For layer = 1, the highest tip layer must be 0, and
        // the only layer-0 entry in any slot is the genesis (the
        // single event placed by `DagStore::create_slot`). So every
        // layer-1 event's non-NULL parents are equal to that slot's
        // genesis id. Higher layers don't need the check because
        // their parent chains transitively pass through layer 1; if
        // the layer-1 events get rejected, layer-2+ events lose
        // their referenced parents and fail the existing parent-
        // existence check.
        let local_genesis_id = Header {
            timestamp: dag_ts,
            parents: NULL_PARENTS,
            layer: 0,
            content_hash: blake3::hash(&self.config.genesis_contents),
        }
        .id();

        let mut store = self.dag_store.write().await;
        let slot = store.get_slot_mut(&dag_ts).ok_or(Error::DagSyncFailed)?;
        let mut overlay = SledTreeOverlay::new(&slot.header_tree);
        let mut hdrs = headers;
        hdrs.sort_by_key(|h| h.layer);

        for hdr in &hdrs {
            if hdr.parents == NULL_PARENTS {
                continue
            }

            // Cross-network detection at the layer-1 boundary.
            if hdr.layer == 1 {
                for pid in hdr.parents.iter() {
                    if *pid != NULL_ID && *pid != local_genesis_id {
                        error!(
                            target: "event_graph::header_dag_insert",
                            "[HEADER_DAG_INSERT] layer-1 header for dag {dag_ts} \
                             references foreign genesis: claimed parent {pid:?}, \
                             local genesis is {local_genesis_id:?}. Peer is on a \
                             different network.",
                        );
                        return Err(Error::HeaderIsInvalid)
                    }
                }
            }

            let hid = hdr.id();
            if overlay.get(hid.as_bytes())?.is_some() {
                continue
            }

            if !hdr.validate(&slot.header_tree, &self.config, dag_ts, Some(&overlay)).await? {
                return Err(Error::HeaderIsInvalid)
            }

            overlay.insert(hid.as_bytes(), &serialize_async(hdr).await)?;
            slot.time_index.insert(hdr.timestamp, hid);
        }

        if let Some(b) = overlay.aggregate() {
            slot.header_tree.apply_batch(b).unwrap();
        }
        Ok(())
    }

    pub async fn fetch_event_from_dags(&self, eid: &blake3::Hash) -> Result<Option<Event>> {
        for (_, slot) in self.dag_store.read().await.dags.iter() {
            if let Some(b) = slot.main_tree.get(eid.as_bytes())? {
                return Ok(Some(deserialize_async(&b).await?))
            }
        }

        // Also check the static DAG. Static events (RLN registrations
        // and slashes) are public consensus state, so they're served
        // alongside rotating-DAG events through the same EventReq
        // path. This is what lets a fresh peer's `static_sync` walk
        // ancestry through EventReq after discovering tips.
        if let Some(b) = self.static_dag.get(eid.as_bytes())? {
            return Ok(Some(deserialize_async(&b).await?))
        }

        Ok(None)
    }

    pub(crate) async fn get_next_layer_with_parents(
        &self,
        dag_ts: &u64,
    ) -> (u64, [blake3::Hash; N_EVENT_PARENTS]) {
        select_parents_from_tips(&self.dag_store.read().await.get_slot(dag_ts).unwrap().tips)
    }

    pub(crate) async fn get_next_layer_with_parents_static(
        &self,
    ) -> (u64, [blake3::Hash; N_EVENT_PARENTS]) {
        select_parents_from_tips(&compute_unreferenced_tips(&self.static_dag).await)
    }

    pub async fn order_events(&self) -> Vec<Event> {
        let mut all = vec![];
        for (_, slot) in self.dag_store.read().await.dags.iter() {
            for item in slot.main_tree.iter() {
                let (_, b) = item.unwrap();
                let ev: Event = deserialize_async(&b).await.unwrap();
                if ev.header.parents != NULL_PARENTS {
                    all.push(ev);
                }
            }
        }

        all.sort_unstable_by(display_order);
        all
    }

    pub async fn fetch_headers_with_tips(
        &self,
        dag_name: &str,
        tips: &LayerUTips,
    ) -> Result<Vec<Header>> {
        if count_layer_tips(tips) > MAX_HEADER_REQ_TIPS {
            return Err(Error::DagSyncFailed)
        }

        let dag_ts = u64::from_str(dag_name)?;
        let store = self.dag_store.read().await;
        let slot = store.get_slot(&dag_ts).ok_or(Error::DagSyncFailed)?;
        let mut ancestors = HashSet::new();

        for hashes in tips.values() {
            for h in hashes {
                ancestors.insert(*h);
                if let Some(v) = slot.header_tree.get(h.as_bytes())? {
                    self.get_ancestors(
                        &mut ancestors,
                        deserialize_async(&v).await?,
                        &slot.header_tree,
                    )
                    .await?;
                }
            }
        }

        let mut out = vec![];

        for item in slot.header_tree.iter() {
            let (id, v) = item?;
            let h = blake3::Hash::from_bytes((&id as &[u8]).try_into()?);
            if !ancestors.contains(&h) {
                if out.len() >= MAX_HEADER_REP_HEADERS {
                    break
                }
                out.push(deserialize_async(&v).await?);
            }
        }

        out.sort_unstable_by_key(|h: &Header| h.layer);
        Ok(out)
    }

    pub(crate) async fn get_ancestors(
        &self,
        visited: &mut HashSet<blake3::Hash>,
        hdr: Header,
        tree: &sled::Tree,
    ) -> Result<()> {
        let mut stack = VecDeque::new();
        stack.push_back(hdr);

        while let Some(h) = stack.pop_back() {
            for p in h.parents {
                if p != NULL_ID && visited.insert(p) {
                    if let Some(v) = tree.get(p.as_bytes())? {
                        stack.push_back(deserialize_async(&v).await?);
                    }
                }
            }
        }

        Ok(())
    }

    async fn static_new(sled_db: &sled::Db, config: &EventGraphConfig) -> Result<sled::Tree> {
        let tree = sled_db.open_tree("static-dag")?;
        let genesis = generate_static_genesis(config);
        let mut ov = SledTreeOverlay::new(&tree);
        ov.insert(genesis.id().as_bytes(), &serialize_async(&genesis).await).unwrap();

        if let Some(b) = ov.aggregate() {
            tree.apply_batch(b).unwrap();
        }

        Ok(tree)
    }

    pub async fn static_broadcast(&self, ev: Event, blob: Vec<u8>) -> Result<()> {
        self.p2p.broadcast(&StaticPut(ev, blob)).await;
        Ok(())
    }

    pub async fn static_insert(&self, ev: &Event) -> Result<()> {
        let mut ov = SledTreeOverlay::new(&self.static_dag);
        ov.insert(ev.id().as_bytes(), &serialize_async(ev).await).unwrap();

        if let Some(b) = ov.aggregate() {
            self.static_dag.apply_batch(b).unwrap();
        }

        self.static_pub.notify(ev.clone()).await;
        Ok(())
    }

    pub async fn static_fetch(&self, eid: &blake3::Hash) -> Result<Option<Event>> {
        Ok(match self.static_dag.get(eid.as_bytes())? {
            Some(b) => Some(deserialize_async(&b).await?),
            None => None,
        })
    }
    pub async fn static_unreferenced_tips(&self) -> LayerUTips {
        compute_unreferenced_tips(&self.static_dag).await
    }

    /// Persist the original RLN blob for a static-DAG event. The
    /// blob is the wire payload from the originating `StaticPut` -
    /// proof + public inputs + attestation - needed to re-verify
    /// the proof at sync time by late-joining peers.
    ///
    /// Writing the same `(eid, blob)` repeatedly is safe.
    pub fn static_blob_store(&self, eid: &blake3::Hash, blob: &[u8]) -> Result<()> {
        self.static_dag_blobs.insert(eid.as_bytes(), blob)?;
        Ok(())
    }

    /// Look up the original RLN blob for a static-DAG event.
    ///
    /// Returns `Ok(None)` only for legitimate "not stored" cases -
    /// a peer that's never seen the event, or an event that pre-dates
    /// the side-table. The verification path in `static_sync` treats
    /// missing blobs on non-genesis events as a sync failure, not as
    /// a fall-through.
    pub fn static_blob_fetch(&self, eid: &blake3::Hash) -> Result<Option<Vec<u8>>> {
        Ok(self.static_dag_blobs.get(eid.as_bytes())?.map(|ivec| ivec.to_vec()))
    }

    /// Persist the original RLN signal blob for a rotating-DAG event.
    ///
    /// Mirror of [`Self::static_blob_store`] but for rotating-DAG
    /// events. Idempotent. Called after successful RLN verification
    /// in `handle_event_put`, and during sync by
    /// `dag_insert_with_blobs` when the peer included a blob in
    /// `EventRep`.
    pub fn dag_blob_store(&self, eid: &blake3::Hash, blob: &[u8]) -> Result<()> {
        self.dag_blobs.insert(eid.as_bytes(), blob)?;
        Ok(())
    }

    /// Look up the original RLN signal blob for a rotating-DAG
    /// event. Returns `Ok(None)` if not stored - see
    /// [`Self::static_blob_fetch`] for the exhaustive list of
    /// reasons a blob may legitimately be missing.
    ///
    /// Note: rotating-DAG blobs are pruned alongside their DAGs
    /// (see `dag_blobs_prune`). Older-than-window events therefore
    /// don't accumulate blobs in this side-table.
    pub fn dag_blob_fetch(&self, eid: &blake3::Hash) -> Result<Option<Vec<u8>>> {
        Ok(self.dag_blobs.get(eid.as_bytes())?.map(|ivec| ivec.to_vec()))
    }

    /// Apply a static-DAG event (registration or slash) to the
    /// identity-state SMT, and record the resulting root in the
    /// historical-roots side-tables.
    ///
    /// **This is the single canonical entry point** for SMT
    /// mutation. All callers - live broadcast (`handle_static_put`),
    /// originator (`nickserv.rs::handle_register`), and sync
    /// (`static_sync` apply loop) - go through here. Bypassing it
    /// will desynchronize the SMT from the historical-roots tables,
    /// which silently breaks signal verification.
    ///
    /// **Canonical order requirement.** Two nodes processing the
    /// same set of static events must produce the same sequence of
    /// intermediate roots. SMTs are commutative under set-of-leaves
    /// (final root is order-independent) but the *intermediate*
    /// roots produced during application are order-dependent. We
    /// pin the order with `(layer, event_id)`: layer is the primary
    /// key (defined by the event's parent links and consensus-agreed),
    /// event_id is the tie-breaker within a layer (32-byte hash,
    /// total-ordered lexicographically).
    ///
    /// In live broadcast and originator paths, events arrive one at
    /// a time; the canonical-order requirement is automatically
    /// satisfied because each event's layer is greater than its
    /// parents'. In sync, the caller must sort by `(layer, event_id)`
    /// before invoking this method (see `static_sync`).
    ///
    /// **Returns** the post-mutation SMT root, or an error if the
    /// SMT mutation itself fails. A duplicate-registration or
    /// slash-of-nonexistent are both treated as soft no-ops at the
    /// SMT layer, but we still record the root (which equals the
    /// pre-call root in that case) - this preserves the invariant
    /// that "every static-DAG event has a corresponding entry in
    /// rln-historical-roots-ordered" without complicating the
    /// caller's logic.
    pub async fn apply_rln_static_event(
        &self,
        ev: &Event,
        node: &rln::RLNNode,
    ) -> Result<pallas::Base> {
        let mut state = self.identity_state.write().await;

        match node {
            rln::RLNNode::Registration(commitment) => {
                // Soft-fail on duplicate (race with another peer).
                let _ = state.register(*commitment);
            }
            rln::RLNNode::Slashing(commitment) => {
                // Idempotent - slashing a non-present identity is
                // a no-op.
                let _ = state.slash(*commitment);
            }
        }

        let new_root = state.root();
        drop(state);

        // Record the root in both side-tables. We do this even if
        // the SMT mutation was a no-op (duplicate register, slash of
        // missing) so the historical-roots table has one entry per
        // static-DAG event. This makes canonical-order replay simple:
        // every event has exactly one entry, no conditional skips.
        let key = encode_historical_root_key(ev.header.layer, &ev.id());
        let value = encode_historical_root_value(&new_root, ev.header.timestamp);
        self.rln_historical_roots_ordered.insert(key, value.as_slice())?;
        let by_value_key = encode_historical_root_by_value_key(&new_root, &key);
        self.rln_historical_roots_by_value.insert(by_value_key, &[])?;

        Ok(new_root)
    }

    /// Check whether `root` is a valid SMT root for a signal whose
    /// `signal_timestamp` is given (in millis-since-epoch).
    ///
    /// A root is valid if it was the live root at any time in the
    /// drift window `[signal_timestamp - DRIFT, signal_timestamp +
    /// DRIFT]`. The drift symmetry handles two distinct concerns:
    ///
    /// * **Forward drift** (signal sees a slightly stale root): the
    ///   originator built a proof against root `R_n`, then someone
    ///   else registered, producing `R_{n+1}`, before the signal
    ///   reached the verifier. The signal's claimed `R_n` is older
    ///   than the verifier's current root by an amount up to the
    ///   propagation delay. Accept if R_n was current within DRIFT
    ///   of the signal's timestamp.
    ///
    /// * **Backward drift** (signal arrives before its root): rare
    ///   but possible if the static-DAG broadcast is racing the
    ///   rotating-DAG broadcast. The originator's machine knew about
    ///   a registration that hadn't fully propagated yet. We tolerate
    ///   up to DRIFT of backward skew.
    ///
    /// The check uses `rln_historical_roots_by_value` to find every
    /// canonical position where `root` appears, then
    /// `rln_historical_roots_ordered` to bracket each interval during
    /// which `root` was live. Each interval starts at the timestamp
    /// of an event that produced `root` and ends just before the next
    /// event timestamp (or `u64::MAX` if `root` is currently live).
    ///
    /// This subsumes the old `recent_roots` window as a special case
    /// (live verification = signal_timestamp ~= now). The in-memory
    /// `recent_roots` cache in `IdentityState` remains as a fast
    /// path for the live-broadcast hot loop; this method is consulted
    /// when the cache misses.
    pub fn is_root_valid_at(&self, root: &pallas::Base, signal_timestamp: u64) -> Result<bool> {
        let drift = EVENT_TIME_DRIFT;
        let lo = signal_timestamp.saturating_sub(drift);
        let hi = signal_timestamp.saturating_add(drift);

        for item in self.rln_historical_roots_by_value.scan_prefix(root.to_repr()) {
            let (by_value_key, _) = item?;
            if by_value_key.len() != 72 {
                continue
            }
            let ordered_key = &by_value_key[32..];

            let Some(value_bytes) = self.rln_historical_roots_ordered.get(ordered_key)? else {
                continue
            };
            let (recorded_root, root_timestamp) = decode_historical_root_value(&value_bytes)?;
            if &recorded_root != root {
                continue
            }

            let next_timestamp: u64 = {
                use std::ops::Bound::{Excluded, Unbounded};
                match self
                    .rln_historical_roots_ordered
                    .range::<&[u8], _>((Excluded(ordered_key), Unbounded))
                    .next()
                {
                    Some(Ok((_, val))) => decode_historical_root_value(&val)?.1,
                    Some(Err(e)) => return Err(e.into()),
                    None => u64::MAX,
                }
            };

            if root_timestamp <= hi && next_timestamp > lo {
                return Ok(true)
            }
        }

        Ok(false)
    }
}

fn encode_historical_root_key(layer: u64, event_id: &blake3::Hash) -> [u8; 40] {
    let mut buf = [0u8; 40];
    buf[..8].copy_from_slice(&layer.to_be_bytes());
    buf[8..].copy_from_slice(event_id.as_bytes());
    buf
}

fn encode_historical_root_value(root: &pallas::Base, timestamp: u64) -> [u8; 40] {
    let mut buf = [0u8; 40];
    buf[..32].copy_from_slice(&root.to_repr());
    buf[32..].copy_from_slice(&timestamp.to_be_bytes());
    buf
}

fn encode_historical_root_by_value_key(root: &pallas::Base, ordered_key: &[u8; 40]) -> [u8; 72] {
    let mut buf = [0u8; 72];
    buf[..32].copy_from_slice(&root.to_repr());
    buf[32..].copy_from_slice(ordered_key);
    buf
}

fn decode_historical_root_value(bytes: &[u8]) -> Result<(pallas::Base, u64)> {
    if bytes.len() != 40 {
        return Err(Error::Custom(format!(
            "historical-root value must be 40 bytes, got {}",
            bytes.len()
        )))
    }
    let mut root_repr = [0u8; 32];
    root_repr.copy_from_slice(&bytes[..32]);
    let root: pallas::Base = match pallas::Base::from_repr(root_repr).into() {
        Some(r) => r,
        None => return Err(Error::Custom("invalid root encoding".into())),
    };
    let mut ts_bytes = [0u8; 8];
    ts_bytes.copy_from_slice(&bytes[32..]);
    Ok((root, u64::from_be_bytes(ts_bytes)))
}

impl EventGraph {
    /// Return a JSON-RPC response representing the current state of
    /// the event graph.
    ///
    /// Shape (matches [`util::recreate_from_replayer_log`] so clients
    /// can reuse their parsers):
    ///
    /// ```json
    /// {
    ///   "eventgraph_info": {
    ///     "dag": {
    ///       "<event-id-hex>": <event>,
    ///       ...
    ///     }
    ///   }
    /// }
    /// ```
    ///
    /// Walks every event currently held in every rotating DAG *and*
    /// every event in the static DAG. Genesis events are included.
    #[cfg(feature = "rpc")]
    pub async fn eventgraph_info(
        &self,
        id: u16,
        _params: crate::rpc::util::JsonValue,
    ) -> crate::rpc::jsonrpc::JsonResult {
        use crate::rpc::{
            jsonrpc::{JsonResponse, JsonResult},
            util::{json_map, JsonValue},
        };

        let mut dag = HashMap::new();

        // Walk every rotating DAG.
        for (_, slot) in self.dag_store.read().await.dags.iter() {
            for item in slot.main_tree.iter() {
                let (eid, val) = match item {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                let Ok(ev) = deserialize_async::<Event>(&val).await else { continue };
                let key = blake3::Hash::from_bytes(match (&eid as &[u8]).try_into() {
                    Ok(b) => b,
                    Err(_) => continue,
                });
                dag.insert(key.to_string(), JsonValue::from(ev));
            }
        }

        // And the static DAG.
        for item in self.static_dag.iter() {
            let (eid, val) = match item {
                Ok(v) => v,
                Err(_) => continue,
            };
            let Ok(ev) = deserialize_async::<Event>(&val).await else { continue };
            let key = blake3::Hash::from_bytes(match (&eid as &[u8]).try_into() {
                Ok(b) => b,
                Err(_) => continue,
            });
            dag.insert(key.to_string(), JsonValue::from(ev));
        }

        let values = json_map([("dag", JsonValue::Object(dag))]);
        let result = JsonValue::Object(HashMap::from([("eventgraph_info".into(), values)]));
        JsonResult::Response(JsonResponse::new(result, id))
    }

    pub fn deg_enable(&self) {
        self.deg_enabled.store(true, Ordering::Release);
    }

    pub fn deg_disable(&self) {
        self.deg_enabled.store(false, Ordering::Release);
    }

    pub fn is_deg_enabled(&self) -> bool {
        self.deg_enabled.load(Ordering::Acquire)
    }

    pub fn is_synced(&self) -> bool {
        self.synced.load(Ordering::Acquire)
    }

    pub async fn deg_subscribe(&self) -> Subscription<DegEvent> {
        self.deg_publisher.clone().subscribe().await
    }

    pub async fn deg_notify(&self, ev: DegEvent) {
        self.deg_publisher.notify(ev).await;
    }

    /// Subscribe to rotating-DAG event insertions.
    ///
    /// Each subscriber receives a clone of every [`Event`] that
    /// passes validation and is committed via `dag_insert`. The
    /// publisher fires *after* state mutation, so subscribers can
    /// rely on the event being durably present in the DAG by the
    /// time they observe it.
    ///
    /// Used to build live JSON-RPC subscription endpoints (e.g.
    /// the Gource-feeding endpoint in DarkIRC). Bridge a
    /// [`Subscription`] to a `JsonSubscriber` in the application
    /// layer; this method itself contains no JSON-RPC logic.
    pub async fn event_subscribe(&self) -> Subscription<Event> {
        self.event_pub.clone().subscribe().await
    }

    /// Subscribe to static-DAG event insertions (RLN registrations
    /// and slashes). Mirrors [`Self::event_subscribe`] but for the
    /// static DAG. See that method for semantics.
    pub async fn static_subscribe(&self) -> Subscription<Event> {
        self.static_pub.clone().subscribe().await
    }

    /// The app identifier mixed into RLN external nullifiers.
    /// Exposed so the proto layer (and clients constructing signal
    /// proofs) can use the same value the verifier uses.
    pub fn rln_app_id(&self) -> rln::RlnAppId {
        self.rln_app_id
    }

    /// Build a membership proof for an identity commitment, plus the
    /// current root.
    ///
    /// This is the *only* sanctioned path for clients to produce a
    /// signal proof - they should not be holding their own copy of
    /// the SMT (the previous `event_graph.rln_identity_tree`
    /// pattern). Centralising this keeps client and verifier in
    /// agreement on the root they're proving against.
    pub async fn rln_membership_path(
        &self,
        commitment: &darkfi_sdk::pasta::pallas::Base,
    ) -> (darkfi_sdk::pasta::pallas::Base, darkfi_sdk::crypto::smt::PathFp) {
        let s = self.identity_state.read().await;
        (s.root(), s.prove_membership(commitment))
    }

    /// True if the given identity commitment is registered.
    pub async fn rln_contains(&self, commitment: &darkfi_sdk::pasta::pallas::Base) -> bool {
        self.identity_state.read().await.contains(commitment)
    }

    /// Verify an RLN signal blob against this event graph's state.
    ///
    /// The returned variant tells the caller what to do:
    /// * `Accepted` - proof valid, no conflict, share recorded.
    /// * `Rejected` - drop silently (bad proof, bad bounds, bad
    ///   root, exact duplicate).
    /// * `Slashable(shares)` - different `(x, y)` for the same
    ///   internal nullifier; the caller (protocol layer) should
    ///   build and broadcast a slash from these shares.
    ///
    /// Critical invariant: `Rejected` and `Slashable` outcomes never
    /// mutate `metadata`. `Accepted` is the only mutating outcome.
    /// This is what prevents share-poisoning by an adversary who
    /// observes an honest internal_nullifier and tries to forge a
    /// share against it.
    pub async fn rln_verify_signal(&self, event: &Event, blob: &[u8]) -> rln::SignalCheck {
        use darkfi_sdk::{crypto::poseidon_hash, pasta::pallas};
        use rln::{epoch_of, hash_event, Blob, SignalCheck, MAX_MSG_LIMIT};

        let rcvd: Blob = match deserialize_async_partial(blob).await {
            Ok((v, _)) => v,
            Err(_) => return SignalCheck::Rejected,
        };

        // Defensive bounds. The proof PI binds these too, but
        // checking up front lets us skip an expensive verify() call
        // for trivially malformed blobs.
        if rcvd.user_msg_limit == 0 || rcvd.user_msg_limit > MAX_MSG_LIMIT {
            return SignalCheck::Rejected
        }

        let epoch_n = epoch_of(event.header.timestamp);
        let epoch_field = pallas::Base::from(epoch_n);
        let app_id = self.rln_app_id();
        let ext_null = poseidon_hash([epoch_field, app_id.as_field()]);
        let x = hash_event(event);

        // 1) The merkle root must be valid for a signal at this
        //    timestamp. We accept any root that was the live SMT
        //    root at any time within EVENT_TIME_DRIFT of the signal's
        //    timestamp. This subsumes the old "recent_roots window"
        //    behaviour as a special case (live verification, signal
        //    timestamp ~= now) AND supports sync of historical signals
        //    (signal timestamp = signing time, root corresponds to
        //    that historical state).
        //
        //    Hot-path optimization: check the in-memory recent_roots
        //    cache first. For live broadcasts (the overwhelming
        //    majority of verifications) the root will be in the
        //    cache and we skip the sled lookup.
        {
            let id_state = self.identity_state.read().await;
            if !id_state.is_known_root(&rcvd.merkle_root) {
                drop(id_state);
                match self.is_root_valid_at(&rcvd.merkle_root, event.header.timestamp) {
                    Ok(true) => {}
                    Ok(false) => {
                        // Useful diagnostic: this rejection path
                        // catches both "garbage root" (attacker
                        // submitted a forged root) and "slashed-user
                        // replay" (slashed identity claiming a
                        // pre-slash root after the propagation
                        // window expired). The two are
                        // indistinguishable from the verifier's
                        // perspective by design - RLN-V2 privacy
                        // guarantees prevent identifying the
                        // signer. But observed in aggregate, a
                        // burst of these from a single peer is a
                        // strong signal of replay-after-slash
                        // misbehavior, useful for operators
                        // debugging "why are my messages being
                        // rejected" or investigating peer abuse.
                        let local_root = self.identity_state.read().await.root();
                        let historical_count = self.rln_historical_roots_ordered.len();
                        warn!(
                            target: "event_graph::rln_verify_signal",
                            "[RLN] Signal rejected: merkle_root not valid at signal \
                             timestamp {}. Possible causes: (1) forged or out-of-sync \
                             root, (2) slashed identity replaying against a pre-slash \
                             root after the propagation window expired. event_id={}, \
                             received_root={:?}, local_current_root={:?}, \
                             historical_root_count={}",
                            event.header.timestamp,
                            event.id(),
                            rcvd.merkle_root,
                            local_root,
                            historical_count,
                        );
                        return SignalCheck::Rejected
                    }
                    Err(e) => {
                        error!(
                            target: "event_graph::rln_verify_signal",
                            "[RLN] is_root_valid_at lookup failed for event {}: {e}",
                            event.id(),
                        );
                        return SignalCheck::Rejected
                    }
                }
            }
        }

        // 2) Verify the ZK proof. PI order MUST match
        //    constrain_instance() in rlnv2-diff-signal.zk:
        //      root, external_nullifier, user_message_limit, x, y, internal_nullifier
        let pi = vec![
            rcvd.merkle_root,
            ext_null,
            pallas::Base::from(rcvd.user_msg_limit),
            x,
            rcvd.y,
            rcvd.internal_nullifier,
        ];
        if rcvd.proof.verify(&self.zk_keys.signal_vk, &pi).is_err() {
            return SignalCheck::Rejected
        }

        // 3) Now consult the metadata table. Any share we look at
        //    here is guaranteed to be from a valid proof.
        let mut state = self.rln_state.write().await;

        // Prune metadata relative to THIS signal's epoch, not
        // wall-clock. The retention window is conceptually
        // "epochs near the signal we're processing", and the only
        // entries that matter for reuse detection are siblings
        // within `METADATA_RETAIN_EPOCHS` of `epoch_n`.
        //
        // In production, signals carry roughly-current wall-clock
        // timestamps, so `epoch_n ~= current_epoch()` and this is
        // equivalent to the previous `current_epoch()`-based prune.
        // The change matters in three places:
        //
        //  * Sync of historical signals: a late-arriving signal
        //    keeps its epoch's metadata visible long enough for
        //    the verifier to detect reuse. With wall-clock prune,
        //    a signal old enough to be outside the retention
        //    window would always silently lose its sibling shares
        //    before they could be matched.
        //
        //  * Tests with deterministic event timestamps: the
        //    verifier and the metadata stay consistent regardless
        //    of when the test runs.
        //
        //  * Minor DoS surface: a peer with a far-future system
        //    clock no longer causes mass-wipe of real metadata
        //    before consultation.
        state.metadata.prune_old(epoch_n);

        if state.metadata.is_duplicate(epoch_n, &rcvd.internal_nullifier, &x, &rcvd.y) {
            return SignalCheck::Rejected
        }

        if state.metadata.is_reused(epoch_n, &rcvd.internal_nullifier) {
            let mut shares = state.metadata.get_shares(epoch_n, &rcvd.internal_nullifier);
            shares.push((x, rcvd.y));
            return SignalCheck::Slashable(shares)
        }

        state.metadata.add_share(epoch_n, rcvd.internal_nullifier, x, rcvd.y);
        SignalCheck::Accepted
    }

    /// Verify a static-DAG event (RLN registration or slashing)
    /// against this event graph's state.
    ///
    /// This is the testable core of the protocol-layer
    /// `handle_static_put`. It performs all checks that the
    /// protocol layer does - bounds, attestation, proof, root
    /// recency - and returns a [`rln::StaticEventCheck`] outcome
    /// telling the caller what to do next.
    ///
    /// **This method does NOT mutate state.** The caller is
    /// responsible for invoking `IdentityState::register` /
    /// `slash` on `Accepted*` outcomes, and for striking the peer
    /// on `Malicious`. Separating decision from action makes the
    /// behaviour fully testable and lets the protocol layer keep
    /// its mutation under a single locked critical section.
    pub async fn rln_verify_static_event(
        &self,
        rln_node: &rln::RLNNode,
        blob: &[u8],
        event_timestamp: u64,
    ) -> rln::StaticEventCheck {
        use darkfi_sdk::crypto::poseidon_hash;
        use rln::{RLNNode, SlashBlob, StaticEventCheck};

        match rln_node {
            RLNNode::Registration(commitment) => {
                // Current admission policy is pregenerated identities only.
                // The guard blob is valid exclusively for commitments supplied
                // by the app config; pairing it with any other commitment is
                // an unambiguous forgery attempt.
                if blob == rln::GENESIS_BLOB_GUARD {
                    let repr = commitment.to_repr();
                    if self.pregenerated_identity_commitment_reprs.contains(&repr) {
                        if self.identity_state.read().await.contains(commitment) {
                            return StaticEventCheck::Rejected
                        }
                        return StaticEventCheck::AcceptedRegistration(*commitment)
                    } else {
                        return StaticEventCheck::Malicious
                    }
                }

                // Free non-pregenerated registration is intentionally disabled:
                // it is a sybil attack surface. Keep the proof scaffolding
                // below for the future staked tier, where acceptance must be
                // backed by a DarkFi smart-contract attestation.
                StaticEventCheck::Rejected
                /*
                #[allow(unreachable_code)]
                let reg: RegistrationBlob = match deserialize_async_partial(blob).await {
                    Ok((v, _)) => v,
                    Err(_) => return StaticEventCheck::Rejected,
                };

                // Bounds. Out-of-range limits are unambiguous misbehavior.
                if reg.user_message_limit == 0 ||
                    reg.user_message_limit > MAX_MSG_LIMIT ||
                    reg.max_message_limit != MAX_MSG_LIMIT
                {
                    return StaticEventCheck::Malicious
                }

                // Attestation must permit the claimed limit.
                // (Free-tier cap; `Staked` rejected until the stake
                // contract is online.)
                if !reg.attestation.permits(reg.user_message_limit) {
                    return StaticEventCheck::Malicious
                }

                // Duplicate registration is a soft Reject (we may
                // have raced a peer), checked here so we don't
                // pay the proof-verification cost for known leaves.
                if self.identity_state.read().await.contains(commitment) {
                    return StaticEventCheck::Rejected
                }

                // Proof.
                let pi = vec![
                    *commitment,
                    pallas::Base::from(reg.user_message_limit),
                    pallas::Base::from(reg.max_message_limit),
                ];
                if reg.proof.verify(&self.zk_keys.register_vk, &pi).is_err() {
                    return StaticEventCheck::Rejected
                }

                StaticEventCheck::AcceptedRegistration(*commitment)
                */
            }
            RLNNode::Slashing(commitment) => {
                let sl: SlashBlob = match deserialize_async_partial(blob).await {
                    Ok((v, _)) => v,
                    Err(_) => return StaticEventCheck::Rejected,
                };

                let pi = vec![sl.identity_secret_hash, sl.merkle_root];
                if sl.proof.verify(&self.zk_keys.slash_vk, &pi).is_err() {
                    return StaticEventCheck::Rejected
                }

                // Slash event names a specific commitment; verify
                // that the recovered identity_secret_hash actually
                // maps to it. Mismatch is unambiguous misbehavior.
                let rebuilt = poseidon_hash([sl.identity_secret_hash]);
                if *commitment != rebuilt {
                    return StaticEventCheck::Malicious
                }

                // The proof's root must be a valid SMT root at the
                // slash event's timestamp. Same logic as signal
                // verification: use the time-window check, which
                // accepts any root that was live within DRIFT of the
                // slash timestamp.
                {
                    let id_state = self.identity_state.read().await;
                    if !id_state.is_known_root(&sl.merkle_root) {
                        drop(id_state);
                        match self.is_root_valid_at(&sl.merkle_root, event_timestamp) {
                            Ok(true) => {}
                            Ok(false) | Err(_) => return StaticEventCheck::Rejected,
                        }
                    }
                }

                StaticEventCheck::AcceptedSlash(rebuilt)
            }
        }
    }

    /// Insert proof-less pregenerated identity commitments into the
    /// static DAG, called once at startup after the static genesis
    /// event itself is inserted. Idempotent - skips any commitment
    /// already present in the identity tree.
    pub async fn bootstrap_genesis_identities(&self) -> Result<()> {
        // Deterministic for configured pregenerated identities.
        let genesis_event = generate_static_genesis(&self.config);
        let genesis_id = genesis_event.id();
        if !self.static_dag.contains_key(genesis_id.as_bytes())? {
            return Err(Error::Custom("static DAG genesis missing during bootstrap".into()))
        }

        for commitment in self.pregenerated_identity_commitments.iter() {
            if self.identity_state.read().await.contains(commitment) {
                continue
            }

            let rln_node = rln::RLNNode::Registration(*commitment);
            let content = serialize_async(&rln_node).await;

            // Inserted at layer 1
            let mut parents = [NULL_ID; N_EVENT_PARENTS];
            parents[0] = genesis_id;

            let header = Header {
                timestamp: genesis_event.header.timestamp + 1,
                parents,
                layer: 1,
                content_hash: blake3::hash(&content),
            };
            let event = Event { header, content };

            if self.static_dag.contains_key(event.id().as_bytes())? {
                continue
            }

            let blob = rln::GENESIS_BLOB_GUARD.to_vec();
            self.static_blob_store(&event.id(), &blob)?;
            self.apply_rln_static_event(&event, &rln_node).await?;
            self.static_insert(&event).await?;
        }

        Ok(())
    }
}

async fn request_tips(
    peer: &Channel,
    dag: String,
    timeout: u64,
) -> Result<BTreeMap<u64, HashSet<blake3::Hash>>> {
    let sub = peer.subscribe_msg::<TipRep>().await?;
    peer.send(&TipReq(dag)).await?;
    let r = sub
        .receive_with_timeout(timeout)
        .await
        .map_err(|_| Error::EventNotFound("tip timeout".into()))?;
    sub.unsubscribe().await;
    if count_layer_tips(&r.0) > MAX_TIP_REP_TIPS {
        return Err(Error::DagSyncFailed)
    }
    Ok(r.0.clone())
}

async fn request_header(
    peer: &Channel,
    name: String,
    tips: LayerUTips,
    timeout: u64,
) -> Result<Vec<Header>> {
    let sub = peer.subscribe_msg::<HeaderRep>().await?;
    let tips = cap_layer_tips(&tips, MAX_HEADER_REQ_TIPS);
    peer.send(&HeaderReq(name, tips)).await?;
    let r = sub
        .receive_with_timeout(timeout)
        .await
        .map_err(|_| Error::EventNotFound("hdr timeout".into()))?;
    sub.unsubscribe().await;
    if r.0.len() > MAX_HEADER_REP_HEADERS {
        return Err(Error::DagSyncFailed)
    }
    Ok(r.0.to_vec())
}

async fn request_event(
    peer: Arc<Channel>,
    ids: Vec<blake3::Hash>,
    cid: usize,
    timeout: u64,
) -> (Result<(Vec<Event>, Vec<Vec<u8>>)>, usize, Arc<Channel>) {
    if ids.len() > MAX_EVENT_REQ_IDS {
        return (Err(Error::DagSyncFailed), cid, peer)
    }

    let sub = match peer.subscribe_msg::<EventRep>().await {
        Ok(s) => s,
        Err(e) => return (Err(e), cid, peer),
    };

    if let Err(e) = peer.send(&EventReq(ids)).await {
        return (Err(e), cid, peer)
    }

    match sub.receive_with_timeout(timeout).await {
        Ok(r) => {
            sub.unsubscribe().await;
            if r.0.len() > MAX_EVENT_REP_EVENTS || r.1.len() > MAX_EVENT_REP_EVENTS {
                return (Err(Error::DagSyncFailed), cid, peer)
            }
            (Ok((r.0.clone(), r.1.clone())), cid, peer)
        }
        Err(_) => (Err(Error::EventNotFound("ev timeout".into())), cid, peer),
    }
}
