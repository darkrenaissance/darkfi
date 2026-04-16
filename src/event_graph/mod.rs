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

use darkfi_serial::{deserialize_async, serialize_async};
use event::Header;
use futures::{stream::FuturesUnordered, StreamExt};
use sled_overlay::{sled, SledTreeOverlay};
use smol::{
    lock::{OnceCell, RwLock},
    Executor,
};
use tracing::{error, info};
use url::Url;

use crate::{
    event_graph::{
        proto::StaticPut,
        util::{next_hour_timestamp, next_rotation_timestamp, replayer_log},
    },
    net::{channel::Channel, P2pPtr},
    system::{msleep, Publisher, PublisherPtr, StoppableTask, StoppableTaskPtr, Subscription},
    Error, Result,
};

pub mod event;
pub use event::{display_order, Event};

pub mod proto;
use proto::{EventRep, EventReq, HeaderRep, HeaderReq, SyncDirection, TipRep, TipReq};

pub mod rln;
use rln::{IdentityState, RlnState, ZkKeys};

pub mod util;
use util::{generate_genesis, millis_until_next_rotation};

pub mod deg;
use deg::DegEvent;

#[cfg(test)]
mod tests;

/// Number of parent references each event carries.
pub const N_EVENT_PARENTS: usize = 5;

/// Allowed timestamp drift in milliseconds.
const EVENT_TIME_DRIFT: u64 = 60_000;

/// The null event ID (32 zero bytes).
pub const NULL_ID: blake3::Hash = blake3::Hash::from_bytes([0x00; blake3::OUT_LEN]);

/// Array of null parents (used by genesis events).
pub const NULL_PARENTS: [blake3::Hash; N_EVENT_PARENTS] = [NULL_ID; N_EVENT_PARENTS];

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

pub type EventGraphPtr = Arc<EventGraph>;
/// Unreferenced tips grouped by layer.
pub type LayerUTips = BTreeMap<u64, HashSet<blake3::Hash>>;

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
        self.index.entry(ts).or_default().push(id);
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

    (tips.last_key_value().unwrap().0 + 1, parents)
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
    pub async fn new(sled_db: sled::Db, config: &EventGraphConfig) -> Self {
        let mut dags = BTreeMap::new();

        if config.hours_rotation == 0 {
            let genesis = generate_genesis(config);
            dags.insert(genesis.header.timestamp, Self::create_slot(&sled_db, &genesis).await);
            return Self { db: sled_db, dags }
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

        Self { db: sled_db, dags }
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
    pub async fn add_dag(&mut self, genesis: &Event, max_dags: Option<usize>) {
        if let Some(limit) = max_dags {
            if self.dags.len() >= limit {
                let (_, old) = self.dags.pop_first().unwrap();
                self.db.drop_tree(old.header_tree.name()).unwrap();
                self.db.drop_tree(old.main_tree.name()).unwrap();
            }
        }
        let slot = Self::create_slot(&self.db, genesis).await;
        self.dags.insert(genesis.header.timestamp, slot);
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
    pub(crate) static_dag: sled::Tree,
    datastore: PathBuf,
    replay_mode: bool,
    pub(crate) broadcasted_ids: RwLock<HashSet<blake3::Hash>>,
    pub prune_task: OnceCell<StoppableTaskPtr>,
    pub event_pub: PublisherPtr<Event>,
    pub static_pub: PublisherPtr<Event>,
    pub current_genesis: RwLock<Event>,
    pub config: EventGraphConfig,
    pub synced: AtomicBool,
    pub deg_enabled: AtomicBool,
    deg_publisher: PublisherPtr<DegEvent>,
    pub(crate) sled_db: sled::Db,
    pub(crate) zk_keys: ZkKeys,
    pub(crate) identity_state: RwLock<IdentityState>,
    pub(crate) rln_state: RwLock<RlnState>,
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
        let zk_keys = ZkKeys::build_and_load(&sled_db)?;
        let identity_state = IdentityState::new(&sled_db)?;
        let current_genesis = generate_genesis(&config);
        let dag_store = DagStore::new(sled_db.clone(), &config).await;
        let static_dag = Self::static_new(&sled_db, &config).await?;

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
            datastore,
            replay_mode,
            broadcasted_ids: RwLock::new(HashSet::new()),
            prune_task: OnceCell::new(),
            event_pub: Publisher::new(),
            static_pub: Publisher::new(),
            current_genesis: RwLock::new(current_genesis.clone()),
            config: config.clone(),
            synced: AtomicBool::new(false),
            deg_enabled: AtomicBool::new(false),
            deg_publisher: Publisher::new(),
            zk_keys,
            identity_state: RwLock::new(identity_state),
            rln_state: RwLock::new(RlnState::new()),
        });

        if need_prune {
            info!(
                target: "event_graph::new",
                "[EVENTGRAPH] Pruning: current genesis not found",
            );
            self_.dag_prune(current_genesis).await?;
        }

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

    /// Sync just the DAG structure (headers) for a single DAG, no
    /// event content.
    ///
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
        if channels.len() < 2 {
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
        let mut received: BTreeMap<usize, Vec<Event>> = BTreeMap::new();
        let mut peer_st: HashMap<Url, PeerStatus> = HashMap::new();
        let mut count = 0;
        let mut fs = FuturesUnordered::new();

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
            let n = std::cmp::min(free.len(), remaining.len());
            let ids: Vec<usize> = remaining.iter().take(n).copied().collect();
            for (i, cid) in ids.iter().enumerate() {
                fs.push(request_event(free[i].clone(), chunks[cid].clone(), *cid, timeout));
                remaining.remove(cid);
                peer_st.insert(free[i].address().clone(), PeerStatus::Busy);
            }
            if let Some((evts, cid, ch)) = fs.next().await {
                if let Ok(e) = evts {
                    count += e.len();
                    received.insert(cid, e);
                    peer_st.insert(ch.address().clone(), PeerStatus::Free);
                } else {
                    remaining.insert(cid);
                    peer_st.insert(ch.address().clone(), PeerStatus::Failed);
                }
            }
        }
        for (_, chunk) in received {
            self.dag_insert(&chunk, dag_name).await?;
        }
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

    /// Fetch a page of events, crossing DAG boundaries transparently.
    pub async fn fetch_page(
        &self,
        cursor_ts: u64,
        dir: SyncDirection,
        limit: usize,
    ) -> Result<Vec<Event>> {
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
        self.dag_store.write().await.add_dag(&genesis, self.config.max_dags).await;
        *cur = genesis;
        *bcast = HashSet::new();
        Ok(())
    }

    async fn dag_prune_task(self: Arc<Self>) -> Result<()> {
        loop {
            let next =
                next_rotation_timestamp(self.config.initial_genesis, self.config.hours_rotation);
            let hdr = Header {
                timestamp: next,
                parents: NULL_PARENTS,
                layer: 0,
                content_hash: blake3::hash(&self.config.genesis_contents),
            };
            let genesis = Event { header: hdr, content: self.config.genesis_contents.clone() };
            msleep(millis_until_next_rotation(next)).await;
            self.dag_prune(genesis).await?;
        }
    }

    pub async fn dag_insert(&self, events: &[Event], dag_name: &str) -> Result<Vec<blake3::Hash>> {
        if events.is_empty() {
            return Ok(vec![])
        }

        let dag_ts = u64::from_str(dag_name)?;
        let mut bcast = self.broadcasted_ids.write().await;
        let mut store = self.dag_store.write().await;
        let slot = store.get_slot_mut(&dag_ts).ok_or(Error::DagSyncFailed)?;

        let mut ids = Vec::with_capacity(events.len());
        let mut overlay = SledTreeOverlay::new(&slot.main_tree);

        for ev in events {
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
            if !ev.dag_validate(&slot.header_tree, &self.config).await? {
                return Err(Error::EventIsInvalid)
            }
            let se = serialize_async(ev).await;
            overlay.insert(eid.as_bytes(), &se)?;
            if self.replay_mode {
                replayer_log(&self.datastore, "insert".into(), se)?;
            }
            ids.push(eid);
        }

        if let Some(b) = overlay.aggregate() {
            slot.main_tree.apply_batch(b).unwrap();
        } else {
            return Ok(vec![])
        }

        for ev in events {
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
        let mut store = self.dag_store.write().await;
        let slot = store.get_slot_mut(&dag_ts).ok_or(Error::DagSyncFailed)?;
        let mut overlay = SledTreeOverlay::new(&slot.header_tree);
        let mut hdrs = headers;
        hdrs.sort_by_key(|h| h.layer);

        for hdr in &hdrs {
            if hdr.parents == NULL_PARENTS {
                continue
            }

            let hid = hdr.id();

            if !hdr.validate(&slot.header_tree, &self.config, Some(&overlay)).await? {
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
        let genesis = generate_genesis(&EventGraphConfig { hours_rotation: 0, ..config.clone() });
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
    Ok(r.0.clone())
}

async fn request_header(
    peer: &Channel,
    name: String,
    tips: LayerUTips,
    timeout: u64,
) -> Result<Vec<Header>> {
    let sub = peer.subscribe_msg::<HeaderRep>().await?;
    peer.send(&HeaderReq(name, tips)).await?;
    let r = sub
        .receive_with_timeout(timeout)
        .await
        .map_err(|_| Error::EventNotFound("hdr timeout".into()))?;
    sub.unsubscribe().await;
    Ok(r.0.to_vec())
}

async fn request_event(
    peer: Arc<Channel>,
    ids: Vec<blake3::Hash>,
    cid: usize,
    timeout: u64,
) -> (Result<Vec<Event>>, usize, Arc<Channel>) {
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
            (Ok(r.0.clone()), cid, peer)
        }
        Err(_) => (Err(Error::EventNotFound("ev timeout".into())), cid, peer),
    }
}
