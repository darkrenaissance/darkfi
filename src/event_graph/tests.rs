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
    slice,
    sync::{atomic::Ordering, Arc},
    time::{Duration, UNIX_EPOCH},
};

use darkfi_serial::serialize_async;
use rand::{prelude::SliceRandom, rngs::ThreadRng};
use sled_overlay::sled;
use smol::{channel, future, Executor};
use url::Url;

use crate::{
    error::Result,
    event_graph::{
        compute_unreferenced_tips,
        event::Header,
        proto::{EventPut, ProtocolEventGraph, SyncDirection},
        util::next_hour_timestamp,
        DagStore, Event, EventGraph, EventGraphConfig, EventGraphPtr, TimeIndex, NULL_ID,
        NULL_PARENTS, N_EVENT_PARENTS,
    },
    net::{session::SESSION_DEFAULT, settings::NetworkProfile, P2p, Settings},
    system::{sleep, timeout::timeout},
    util::logger::{setup_test_logger, Level},
};

const N_NODES: usize = 5;
const N_CONNS: usize = 2;

/// Test config: 15 Apr 2026 UTC, hourly rotation, 24-DAG window.
fn test_config() -> EventGraphConfig {
    EventGraphConfig {
        initial_genesis: 1_776_211_200_000,
        hours_rotation: 1,
        genesis_contents: b"test-graph-v1".to_vec(),
        max_dags: Some(24),
    }
}

/// Archive-mode variant of the test config.
fn archive_config() -> EventGraphConfig {
    EventGraphConfig { max_dags: None, ..test_config() }
}

fn init_logger() {
    let ignored = [
        "sled",
        "net::protocol_ping",
        "net::channel::subscribe_stop()",
        "net::hosts",
        "net::session",
        "net::message_subscriber",
        "net::protocol_address",
        "net::protocol_version",
        "net::protocol_registry",
        "net::channel::send()",
        "net::channel::start()",
        "net::channel::subscribe_msg()",
        "net::channel::main_receive_loop()",
        "net::tcp",
    ];
    let _ = setup_test_logger(&ignored, false, Level::Info);
}

async fn spawn_node(
    inbound: Vec<Url>,
    peers: Vec<Url>,
    ex: Arc<Executor<'static>>,
) -> Arc<EventGraph> {
    let mut profiles = HashMap::new();
    profiles.insert(
        "tcp".to_string(),
        NetworkProfile { outbound_connect_timeout: 2, ..Default::default() },
    );
    let settings = Settings {
        localnet: true,
        inbound_addrs: inbound,
        outbound_connections: 0,
        inbound_connections: usize::MAX,
        peers,
        active_profiles: vec!["tcp".to_string()],
        profiles,
        ..Default::default()
    };

    let p2p = P2p::new(settings, ex.clone()).await.unwrap();
    let sled_db = sled::Config::new().temporary(true).open().unwrap();
    let eg = EventGraph::new(p2p.clone(), sled_db, "/tmp".into(), false, test_config(), ex.clone())
        .await
        .unwrap();

    // Mark as synced so protocol handlers accept events during tests
    eg.synced.store(true, Ordering::Release);

    let eg_ = eg.clone();
    p2p.protocol_registry()
        .register(SESSION_DEFAULT, move |channel, _| {
            let eg_ = eg_.clone();
            async move { ProtocolEventGraph::init(eg_, channel).await.unwrap() }
        })
        .await;
    eg
}

#[test]
fn evgr_time_index_bidirectional_queries() {
    let mut idx = TimeIndex::new();
    for ts in [100_u64, 200, 200, 300, 400, 500] {
        let id = blake3::hash(&ts.to_be_bytes());
        idx.insert(ts, id);
    }

    assert_eq!(idx.len(), 6);
    assert_eq!(idx.newest(3).len(), 3);
    assert_eq!(idx.oldest(2).len(), 2);
    // Before 300 -> events at 200 (x2) and 100
    assert_eq!(idx.before(300, 10).len(), 3);
    // After 200 -> events at 300, 400, 500
    assert_eq!(idx.after(200, 10).len(), 3);
}

#[test]
fn evgr_time_index_saturating_cursor() {
    let mut idx = TimeIndex::new();
    idx.insert(100, blake3::hash(b"x"));

    // before(0) should not underflow
    assert_eq!(idx.before(0, 10).len(), 0);
    // after(u64::MAX) should not overflow
    assert_eq!(idx.after(u64::MAX, 10).len(), 0);
}

async fn make_dag_store() -> Result<DagStore> {
    let sled_db = sled::Config::new().temporary(true).open()?;
    Ok(DagStore::new(sled_db, &test_config()).await)
}

#[test]
fn evgr_dag_store_creates_rolling_window() -> Result<()> {
    smol::block_on(async {
        let store = make_dag_store().await?;
        assert_eq!(store.dag_timestamps().len(), 24);
        Ok(())
    })
}

#[test]
fn evgr_dag_store_all_slots_have_genesis() -> Result<()> {
    smol::block_on(async {
        let store = make_dag_store().await?;
        for ts in store.dag_timestamps() {
            let slot = store.get_slot(&ts).unwrap();
            assert!(!slot.header_tree.is_empty());
            assert!(!slot.main_tree.is_empty());
            assert!(!slot.tips.is_empty());
            assert!(!slot.time_index.is_empty());
        }
        Ok(())
    })
}

#[test]
fn evgr_dag_store_add_drops_oldest_in_bounded_mode() -> Result<()> {
    smol::block_on(async {
        let mut store = make_dag_store().await?;
        let oldest_ts = store.dag_timestamps()[0];
        let new_ts = next_hour_timestamp(1);
        let hdr = Header {
            timestamp: new_ts,
            parents: NULL_PARENTS,
            layer: 0,
            content_hash: blake3::hash(b"test-graph-v1"),
        };
        let genesis = Event { header: hdr, content: b"test-graph-v1".to_vec() };
        store.add_dag(&genesis, Some(24)).await;

        assert_eq!(store.dag_timestamps().len(), 24);
        assert!(store.get_slot(&new_ts).is_some());
        assert!(store.get_slot(&oldest_ts).is_none());
        Ok(())
    })
}

#[test]
fn evgr_dag_store_archive_mode_never_drops() -> Result<()> {
    smol::block_on(async {
        let sled_db = sled::Config::new().temporary(true).open()?;
        let mut store = DagStore::new(sled_db, &archive_config()).await;
        let initial = store.dag_timestamps().len();

        // Add DAGs well beyond the normal 24-window
        for i in 1..=30i64 {
            let ts = next_hour_timestamp(i);
            let hdr = Header {
                timestamp: ts,
                parents: NULL_PARENTS,
                layer: 0,
                content_hash: blake3::hash(b"test-graph-v1"),
            };
            let genesis = Event { header: hdr, content: b"test-graph-v1".to_vec() };
            store.add_dag(&genesis, None).await;
        }

        // Nothing should have been dropped
        assert_eq!(store.dag_timestamps().len(), initial + 30);
        Ok(())
    })
}

#[test]
fn evgr_dag_store_archive_mode_discovers_existing_trees() -> Result<()> {
    smol::block_on(async {
        let sled_db = sled::Config::new().temporary(true).open()?;

        // First run: create archive store and add some historical DAGs
        let historical_ts = next_hour_timestamp(-100);
        {
            let mut store = DagStore::new(sled_db.clone(), &archive_config()).await;
            let hdr = Header {
                timestamp: historical_ts,
                parents: NULL_PARENTS,
                layer: 0,
                content_hash: blake3::hash(b"test-graph-v1"),
            };
            let genesis = Event { header: hdr, content: b"test-graph-v1".to_vec() };
            store.add_dag(&genesis, None).await;
            drop(store);
        }

        // Second run: reopen and verify the historical DAG is discovered
        let store = DagStore::new(sled_db, &archive_config()).await;
        assert!(
            store.get_slot(&historical_ts).is_some(),
            "Archive mode should discover historical DAGs on restart"
        );
        Ok(())
    })
}

#[test]
fn evgr_compute_unreferenced_tips_single_pass() -> Result<()> {
    smol::block_on(async {
        let store = make_dag_store().await?;
        let ts = *store.dag_timestamps().last().unwrap();
        let slot = store.get_slot(&ts).unwrap();
        let genesis_hash = *slot.tips.get(&0).unwrap().iter().next().unwrap();

        // Build a small DAG manually:
        //      genesis
        //      /     \
        //     e2      e4  (both at layer 1)
        //      |
        //     e3        (layer 2)
        //
        // Header IDs include content_hash, so events with identical
        // (timestamp, parents, layer) but different content get
        // distinct IDs.
        let now = UNIX_EPOCH.elapsed().unwrap().as_millis() as u64;

        let mut p = [NULL_ID; N_EVENT_PARENTS];
        p[0] = genesis_hash;
        let e2 = Event {
            header: Header {
                timestamp: now,
                parents: p,
                layer: 1,
                content_hash: blake3::hash(b"e2"),
            },
            content: b"e2".to_vec(),
        };
        slot.main_tree.insert(e2.id().as_bytes(), serialize_async(&e2).await)?;

        let mut p = [NULL_ID; N_EVENT_PARENTS];
        p[0] = e2.id();
        let e3 = Event {
            header: Header {
                timestamp: now,
                parents: p,
                layer: 2,
                content_hash: blake3::hash(b"e3"),
            },
            content: b"e3".to_vec(),
        };
        slot.main_tree.insert(e3.id().as_bytes(), serialize_async(&e3).await)?;

        let mut p = [NULL_ID; N_EVENT_PARENTS];
        p[0] = genesis_hash;
        let e4 = Event {
            header: Header {
                timestamp: now,
                parents: p,
                layer: 1,
                content_hash: blake3::hash(b"e4"),
            },
            content: b"e4".to_vec(),
        };
        slot.main_tree.insert(e4.id().as_bytes(), serialize_async(&e4).await)?;

        assert_ne!(e2.id(), e4.id(), "e2 and e4 must have distinct IDs");

        let tips = compute_unreferenced_tips(&slot.main_tree).await;

        // e3 (layer 2) and e4 (layer 1) are unreferenced;
        // e2 is a parent of e3, so it's not a tip.
        assert!(tips.get(&2).unwrap().contains(&e3.id()));
        assert!(tips.get(&1).unwrap().contains(&e4.id()));
        assert!(!tips.values().any(|set| set.contains(&e2.id())));
        Ok(())
    })
}

async fn make_event_graph() -> Result<EventGraphPtr> {
    let ex = Arc::new(Executor::new());
    let p2p = P2p::new(Settings::default(), ex.clone()).await?;
    let sled_db = sled::Config::new().temporary(true).open()?;
    EventGraph::new(p2p, sled_db, "/tmp".into(), false, test_config(), ex).await
}

#[test]
fn evgr_dag_insert_valid_event() -> Result<()> {
    smol::block_on(async {
        let eg = make_event_graph().await?;
        let dag_ts = eg.current_genesis.read().await.header.timestamp;
        let dag_name = dag_ts.to_string();
        let sub = eg.event_pub.clone().subscribe().await;

        let event = Event::new(b"hello".to_vec(), &eg).await;
        eg.header_dag_insert(vec![event.header.clone()], &dag_name).await?;
        let ids = eg.dag_insert(slice::from_ref(&event), &dag_name).await?;
        assert_eq!(ids.len(), 1);

        // Tips updated to include the new event
        let store = eg.dag_store.read().await;
        let slot = store.get_slot(&dag_ts).unwrap();
        assert!(slot.tips.get(&1).unwrap().contains(&event.id()));
        drop(store);

        // Publisher notified
        let Ok(notified) = timeout(Duration::from_secs(1), sub.receive()).await else {
            panic!("Event notification not received");
        };
        assert_eq!(notified.id(), event.id());
        Ok(())
    })
}

#[test]
fn evgr_dag_insert_duplicate_skipped() -> Result<()> {
    smol::block_on(async {
        let eg = make_event_graph().await?;
        let dag_name = eg.current_genesis.read().await.header.timestamp.to_string();
        let event = Event::new(b"dup".to_vec(), &eg).await;
        eg.header_dag_insert(vec![event.header.clone()], &dag_name).await?;

        assert_eq!(eg.dag_insert(slice::from_ref(&event), &dag_name).await?.len(), 1);
        assert!(eg.dag_insert(slice::from_ref(&event), &dag_name).await?.is_empty());
        Ok(())
    })
}

#[test]
fn evgr_dag_insert_without_header_skipped() -> Result<()> {
    smol::block_on(async {
        let eg = make_event_graph().await?;
        let dag_name = eg.current_genesis.read().await.header.timestamp.to_string();
        let event = Event::new(b"orphan".to_vec(), &eg).await;

        // No header_dag_insert call -> event shouldn't be inserted
        let ids = eg.dag_insert(slice::from_ref(&event), &dag_name).await?;
        assert!(ids.is_empty());
        Ok(())
    })
}

#[test]
fn evgr_fetch_page_both_directions() -> Result<()> {
    smol::block_on(async {
        let eg = make_event_graph().await?;
        let dag_name = eg.current_genesis.read().await.header.timestamp.to_string();

        // Insert 10 events with strictly-increasing timestamps
        let base = UNIX_EPOCH.elapsed().unwrap().as_millis() as u64;
        let mut inserted = vec![];
        for i in 0..10u64 {
            let ev = Event::with_timestamp(base + i, vec![i as u8], &eg).await;
            eg.header_dag_insert(vec![ev.header.clone()], &dag_name).await?;
            eg.dag_insert(slice::from_ref(&ev), &dag_name).await?;
            inserted.push(ev);
        }

        // Backward from u64::MAX -> should get newest events first
        let page = eg.fetch_page(u64::MAX, SyncDirection::Backward, 5).await?;
        assert_eq!(page.len(), 5);
        // Ensure descending timestamps
        for w in page.windows(2) {
            assert!(w[0].header.timestamp >= w[1].header.timestamp);
        }

        // Forward from 0 -> oldest first
        let page = eg.fetch_page(0, SyncDirection::Forward, 5).await?;
        assert!(!page.is_empty());
        for w in page.windows(2) {
            assert!(w[0].header.timestamp <= w[1].header.timestamp);
        }
        Ok(())
    })
}

async fn build_graph() -> Result<(EventGraphPtr, HashMap<&'static str, Event>)> {
    let eg = make_event_graph().await?;
    let dag_name = eg.current_genesis.read().await.header.timestamp.to_string();
    let genesis_hash = eg.current_genesis.read().await.id();
    let base = UNIX_EPOCH.elapsed().unwrap().as_millis() as u64;

    let make = |off: u64, layer: u64, parents: [blake3::Hash; N_EVENT_PARENTS], name: &str| Event {
        header: Header {
            timestamp: base + off,
            layer,
            parents,
            content_hash: blake3::hash(name.as_bytes()),
        },
        content: name.as_bytes().to_vec(),
    };

    //           genesis
    //          / | | \
    //       e1a e1b e1c e1d       (layer 1)
    //        |   |   |   |
    //       e2a e2b e2c e2d        (layer 2)
    let mut p = [NULL_ID; N_EVENT_PARENTS];
    p[0] = genesis_hash;
    let e1a = make(1, 1, p, "e1a");
    let e1b = make(2, 1, p, "e1b");
    let e1c = make(3, 1, p, "e1c");
    let e1d = make(4, 1, p, "e1d");

    let mut p = [NULL_ID; N_EVENT_PARENTS];
    p[0] = e1a.id();
    let e2a = make(5, 2, p, "e2a");
    p[0] = e1b.id();
    let e2b = make(6, 2, p, "e2b");
    p[0] = e1c.id();
    let e2c = make(7, 2, p, "e2c");
    p[0] = e1d.id();
    let e2d = make(8, 2, p, "e2d");

    let l1 = vec![e1a.clone(), e1b.clone(), e1c.clone(), e1d.clone()];
    let l2 = vec![e2a.clone(), e2b.clone(), e2c.clone(), e2d.clone()];

    eg.header_dag_insert(l1.iter().map(|e| e.header.clone()).collect(), &dag_name).await?;
    eg.dag_insert(&l1, &dag_name).await?;
    eg.header_dag_insert(l2.iter().map(|e| e.header.clone()).collect(), &dag_name).await?;
    eg.dag_insert(&l2, &dag_name).await?;

    let mut map = HashMap::new();
    map.insert("e1a", e1a);
    map.insert("e1b", e1b);
    map.insert("e1c", e1c);
    map.insert("e1d", e1d);
    map.insert("e2a", e2a);
    map.insert("e2b", e2b);
    map.insert("e2c", e2c);
    map.insert("e2d", e2d);
    Ok((eg, map))
}

#[test]
fn evgr_ancestor_walk_via_header_tree() -> Result<()> {
    smol::block_on(async {
        let (eg, evs) = build_graph().await?;
        let dag_ts = eg.current_genesis.read().await.header.timestamp;
        let store = eg.dag_store.read().await;
        let slot = store.get_slot(&dag_ts).unwrap();
        let genesis_hash = eg.current_genesis.read().await.id();

        // Layer-1 events should have only genesis as ancestor
        for name in ["e1a", "e1b", "e1c", "e1d"] {
            let mut ancestors = HashSet::new();
            eg.get_ancestors(&mut ancestors, evs[name].header.clone(), &slot.header_tree).await?;
            assert_eq!(ancestors, HashSet::from([genesis_hash]));
        }

        // e2a's ancestors = {genesis, e1a}
        let mut ancestors = HashSet::new();
        eg.get_ancestors(&mut ancestors, evs["e2a"].header.clone(), &slot.header_tree).await?;
        assert_eq!(ancestors, HashSet::from([genesis_hash, evs["e1a"].id()]));
        Ok(())
    })
}

#[test]
fn evgr_order_events_is_chronological() -> Result<()> {
    smol::block_on(async {
        let (eg, _) = build_graph().await?;
        let ordered = eg.order_events().await;
        for w in ordered.windows(2) {
            assert!(w[0].header.timestamp <= w[1].header.timestamp);
        }
        Ok(())
    })
}

macro_rules! test_body {
    ($real_call:ident) => {
        init_logger();
        let ex = Arc::new(Executor::new());
        let ex_ = ex.clone();
        let (signal, shutdown) = channel::unbounded::<()>();
        easy_parallel::Parallel::new()
            .each(0..N_NODES, |_| future::block_on(ex.run(shutdown.recv())))
            .finish(|| {
                future::block_on(async {
                    $real_call(ex_).await;
                    drop(signal);
                })
            });
    };
}

#[test]
fn evgr_eventgraph_propagation() {
    test_body!(eventgraph_propagation_real);
}

async fn eventgraph_propagation_real(ex: Arc<Executor<'static>>) {
    let mut rng: ThreadRng = rand::thread_rng();
    let idxs: Vec<usize> = (0..N_NODES).collect();

    // Bootstrap a small network
    let mut nodes = vec![];
    for i in 0..N_NODES {
        let mut pi = idxs.clone();
        pi.remove(i);
        let conns: Vec<_> = pi.choose_multiple(&mut rng, N_CONNS).collect();
        let peers: Vec<_> = conns
            .iter()
            .map(|p| Url::parse(&format!("tcp://127.0.0.1:{}", 13200 + *p)).unwrap())
            .collect();
        let inbound = vec![Url::parse(&format!("tcp://127.0.0.1:{}", 13200 + i)).unwrap()];
        nodes.push(spawn_node(inbound, peers, ex.clone()).await);
    }
    for eg in &nodes {
        eg.p2p.clone().start().await.unwrap();
    }
    sleep(5).await;

    // Broadcast an event from a random node
    let dag_name = nodes[0].current_genesis.read().await.header.timestamp.to_string();
    let node = nodes.choose(&mut rng).unwrap();
    let ev = Event::new(vec![1, 2, 3, 4], node).await;
    node.header_dag_insert(vec![ev.header.clone()], &dag_name).await.unwrap();
    node.dag_insert(slice::from_ref(&ev), &dag_name).await.unwrap();
    node.p2p.broadcast(&EventPut(ev.clone(), vec![])).await;
    sleep(5).await;

    // Every node should now have at least genesis + the new event
    for (i, eg) in nodes.iter().enumerate() {
        let ts = eg.current_genesis.read().await.header.timestamp;
        let store = eg.dag_store.read().await;
        let slot = store.get_slot(&ts).unwrap();
        assert!(
            slot.main_tree.len() >= 2,
            "Node {i} has only {} events in main_tree",
            slot.main_tree.len()
        );
    }

    for eg in &nodes {
        eg.p2p.clone().stop().await;
    }
}
