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
    collections::HashSet,
    slice,
    sync::Arc,
    time::{Duration, UNIX_EPOCH},
};

use darkfi_serial::serialize_async;
use sled_overlay::sled;
use smol::Executor;

use crate::{
    error::Result,
    event_graph::{
        compute_unreferenced_tips,
        event::Header,
        proto::{EventPut, SyncDirection},
        test_helpers::{
            archive_config, bounded_dag_store_config, init_logger, make_eg, make_network,
            run_multi_node_test, shutdown_network, TestIdentity,
        },
        util::next_hour_timestamp,
        DagStore, Event, EventGraphPtr, TimeIndex, NULL_ID, NULL_PARENTS, N_EVENT_PARENTS,
    },
    system::{sleep, timeout::timeout},
};

#[test]
fn evgr_time_index_queries_and_saturating_cursor() {
    // Forward, backward, newest, oldest queries plus the saturating
    // cursor at u64 boundaries.
    let mut idx = TimeIndex::new();
    for ts in [100_u64, 200, 200, 300, 400, 500] {
        let id = blake3::hash(&ts.to_be_bytes());
        idx.insert(ts, id);
    }
    assert_eq!(idx.len(), 6);
    assert_eq!(idx.newest(3).len(), 3);
    assert_eq!(idx.oldest(2).len(), 2);
    assert_eq!(idx.before(300, 10).len(), 3);
    assert_eq!(idx.after(200, 10).len(), 3);

    // Saturating cursor: before(0) shouldn't underflow,
    // after(u64::MAX) shouldn't overflow.
    let mut idx2 = TimeIndex::new();
    idx2.insert(100, blake3::hash(b"x"));
    assert_eq!(idx2.before(0, 10).len(), 0);
    assert_eq!(idx2.after(u64::MAX, 10).len(), 0);
}

async fn make_dag_store() -> Result<DagStore> {
    let sled_db = sled::Config::new().temporary(true).open().unwrap();
    Ok(DagStore::new(sled_db, &bounded_dag_store_config()).await)
}

#[test]
fn evgr_dag_store_eviction_policy() {
    // Bounded vs archive mode in one test:
    //   (a) bounded: adding a 25th DAG drops the oldest, total stays 24.
    //   (b) archive: adding 30 DAGs leaves all 30 plus the originals.
    smol::block_on(async {
        // (a) bounded
        let mut store = make_dag_store().await.unwrap();
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

        // (b) archive
        let sled_db = sled::Config::new().temporary(true).open().unwrap();
        let mut archive = DagStore::new(sled_db, &archive_config()).await;
        let initial = archive.dag_timestamps().len();
        for i in 1..=30i64 {
            let ts = next_hour_timestamp(i);
            let hdr = Header {
                timestamp: ts,
                parents: NULL_PARENTS,
                layer: 0,
                content_hash: blake3::hash(b"test-graph-v1"),
            };
            let genesis = Event { header: hdr, content: b"test-graph-v1".to_vec() };
            archive.add_dag(&genesis, None).await;
        }
        assert_eq!(archive.dag_timestamps().len(), initial + 30);
    })
}

#[test]
fn evgr_dag_store_archive_mode_discovers_existing_trees() {
    smol::block_on(async {
        let sled_db = sled::Config::new().temporary(true).open().unwrap();
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

        let store = DagStore::new(sled_db, &archive_config()).await;
        assert!(
            store.get_slot(&historical_ts).is_some(),
            "Archive mode should discover historical DAGs on restart"
        );
    })
}

#[test]
fn evgr_compute_unreferenced_tips_single_pass() {
    smol::block_on(async {
        let store = make_dag_store().await.unwrap();
        let ts = *store.dag_timestamps().last().unwrap();
        let slot = store.get_slot(&ts).unwrap();
        let genesis_hash = *slot.tips.get(&0).unwrap().iter().next().unwrap();

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
        slot.main_tree.insert(e2.id().as_bytes(), serialize_async(&e2).await).unwrap();

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
        slot.main_tree.insert(e3.id().as_bytes(), serialize_async(&e3).await).unwrap();

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
        slot.main_tree.insert(e4.id().as_bytes(), serialize_async(&e4).await).unwrap();

        assert_ne!(e2.id(), e4.id(), "e2 and e4 must have distinct IDs");

        let tips = compute_unreferenced_tips(&slot.main_tree).await;

        assert!(tips.get(&2).unwrap().contains(&e3.id()));
        assert!(tips.get(&1).unwrap().contains(&e4.id()));
        assert!(!tips.values().any(|set| set.contains(&e2.id())));
    })
}

#[test]
fn evgr_dag_insert_valid_and_duplicate() {
    // First insert: returns the id, updates tips, fires the
    // subscriber. Second (duplicate) insert: returns an empty list,
    // doesn't re-fire.
    smol::block_on(async {
        let eg = make_eg().await;
        let dag_ts = eg.current_genesis.read().await.header.timestamp;
        let dag_name = dag_ts.to_string();
        let sub = eg.event_pub.clone().subscribe().await;

        let event = Event::new(b"hello".to_vec(), &eg).await;
        eg.header_dag_insert(vec![event.header.clone()], &dag_name).await.unwrap();
        let ids = eg.dag_insert(slice::from_ref(&event), &dag_name).await.unwrap();
        assert_eq!(ids.len(), 1);

        let store = eg.dag_store.read().await;
        let slot = store.get_slot(&dag_ts).unwrap();
        assert!(slot.tips.get(&1).unwrap().contains(&event.id()));
        drop(store);

        let Ok(notified) = timeout(Duration::from_secs(1), sub.receive()).await else {
            panic!("Event notification not received");
        };
        assert_eq!(notified.id(), event.id());

        // Re-insert is a no-op.
        assert!(eg.dag_insert(slice::from_ref(&event), &dag_name).await.unwrap().is_empty());
    })
}

#[test]
fn evgr_dag_insert_without_header_skipped() {
    smol::block_on(async {
        let eg = make_eg().await;
        let dag_name = eg.current_genesis.read().await.header.timestamp.to_string();
        let event = Event::new(b"orphan".to_vec(), &eg).await;
        let ids = eg.dag_insert(slice::from_ref(&event), &dag_name).await.unwrap();
        assert!(ids.is_empty());
    })
}

#[test]
fn evgr_fetch_page_both_directions() {
    smol::block_on(async {
        let eg = make_eg().await;
        let dag_name = eg.current_genesis.read().await.header.timestamp.to_string();
        let base = UNIX_EPOCH.elapsed().unwrap().as_millis() as u64;
        for i in 0..10u64 {
            let ev = Event::with_timestamp(base + i, vec![i as u8], &eg).await;
            eg.header_dag_insert(vec![ev.header.clone()], &dag_name).await.unwrap();
            eg.dag_insert(slice::from_ref(&ev), &dag_name).await.unwrap();
        }

        let page = eg.fetch_page(u64::MAX, SyncDirection::Backward, 5).await.unwrap();
        assert_eq!(page.len(), 5);
        for w in page.windows(2) {
            assert!(w[0].header.timestamp >= w[1].header.timestamp);
        }

        let page = eg.fetch_page(0, SyncDirection::Forward, 5).await.unwrap();
        assert!(!page.is_empty());
        for w in page.windows(2) {
            assert!(w[0].header.timestamp <= w[1].header.timestamp);
        }
    })
}

async fn build_graph() -> Result<(EventGraphPtr, std::collections::HashMap<&'static str, Event>)> {
    let eg = make_eg().await;
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

    eg.header_dag_insert(l1.iter().map(|e| e.header.clone()).collect(), &dag_name).await.unwrap();
    eg.dag_insert(&l1, &dag_name).await.unwrap();
    eg.header_dag_insert(l2.iter().map(|e| e.header.clone()).collect(), &dag_name).await.unwrap();
    eg.dag_insert(&l2, &dag_name).await.unwrap();

    let mut map = std::collections::HashMap::new();
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
fn evgr_ancestor_walk_via_header_tree() {
    smol::block_on(async {
        let (eg, evs) = build_graph().await.unwrap();
        let dag_ts = eg.current_genesis.read().await.header.timestamp;
        let store = eg.dag_store.read().await;
        let slot = store.get_slot(&dag_ts).unwrap();
        let genesis_hash = eg.current_genesis.read().await.id();

        for name in ["e1a", "e1b", "e1c", "e1d"] {
            let mut ancestors = HashSet::new();
            eg.get_ancestors(&mut ancestors, evs[name].header.clone(), &slot.header_tree)
                .await
                .unwrap();
            assert_eq!(ancestors, HashSet::from([genesis_hash]));
        }

        let mut ancestors = HashSet::new();
        eg.get_ancestors(&mut ancestors, evs["e2a"].header.clone(), &slot.header_tree)
            .await
            .unwrap();
        assert_eq!(ancestors, HashSet::from([genesis_hash, evs["e1a"].id()]));
    })
}

#[test]
fn evgr_multi_node_propagation_with_real_blob() {
    init_logger();
    run_multi_node_test(propagation_with_real_blob);
}
async fn propagation_with_real_blob(ex: Arc<Executor<'static>>) {
    let nodes = make_network(ex).await;

    let mut alice = TestIdentity::new();
    for eg in &nodes {
        alice.register_directly(eg).await.expect("register alice");
    }

    let dag_ts = nodes[0].current_genesis.read().await.header.timestamp;
    let dag_name = dag_ts.to_string();
    let event = Event::new(b"hello-via-rln".to_vec(), &nodes[0]).await;

    let message_id =
        alice.next_message_id(event.header.timestamp).expect("budget available on first signal");
    let blob_struct = alice.create_signal(&event, message_id, &nodes[0]).await.unwrap();
    let blob = serialize_async(&blob_struct).await;

    nodes[0].header_dag_insert(vec![event.header.clone()], &dag_name).await.unwrap();
    nodes[0].dag_insert(slice::from_ref(&event), &dag_name).await.unwrap();
    nodes[0].dag_blob_store(&event.id(), &blob).unwrap();
    nodes[0].p2p.broadcast(&EventPut(event.clone(), blob.clone())).await;

    sleep(5).await;

    for (i, eg) in nodes.iter().enumerate() {
        let store = eg.dag_store.read().await;
        let slot = store.get_slot(&dag_ts).unwrap();
        assert!(
            slot.main_tree.contains_key(event.id().as_bytes()).unwrap(),
            "node {i} missing event in main_tree",
        );
        drop(store);
        assert!(
            eg.dag_blob_fetch(&event.id()).unwrap().is_some(),
            "node {i} missing blob in dag_blobs - late-joiners would have nothing to verify against",
        );
    }

    shutdown_network(&nodes).await;
}

#[test]
fn evgr_multi_node_empty_blob_rejected() {
    init_logger();
    run_multi_node_test(empty_blob_rejected);
}
async fn empty_blob_rejected(ex: Arc<Executor<'static>>) {
    // An attacker broadcasts `EventPut(ev, vec![])` for a non-genesis event.
    // Every recipient must strike the sender and refuse to insert.
    let nodes = make_network(ex).await;

    let dag_ts = nodes[0].current_genesis.read().await.header.timestamp;
    let event = Event::new(b"unauthenticated".to_vec(), &nodes[0]).await;

    nodes[0].p2p.broadcast(&EventPut(event.clone(), vec![])).await;
    sleep(5).await;

    for (i, eg) in nodes.iter().enumerate().skip(1) {
        let store = eg.dag_store.read().await;
        let slot = store.get_slot(&dag_ts).unwrap();
        assert!(
            !slot.main_tree.contains_key(event.id().as_bytes()).unwrap(),
            "Vector-1 breach: node {i} accepted an empty-blob non-genesis event",
        );
    }

    shutdown_network(&nodes).await;
}

#[test]
fn evgr_multi_node_genesis_with_blob_rejected() {
    init_logger();
    run_multi_node_test(genesis_with_blob_rejected);
}
async fn genesis_with_blob_rejected(ex: Arc<Executor<'static>>) {
    // Symmetric defense: a genesis-shaped event arriving with a
    // non-empty blob is also misbehavior. Genesis events are
    // deterministic and don't carry signals.
    let nodes = make_network(ex).await;

    let dag_ts = nodes[0].current_genesis.read().await.header.timestamp;

    let header = Header {
        timestamp: dag_ts,
        parents: NULL_PARENTS,
        layer: 0,
        content_hash: blake3::hash(b"forged-genesis"),
    };
    let event = Event { header, content: b"forged-genesis".to_vec() };
    let fake_blob = b"this-should-not-be-here".to_vec();

    nodes[0].p2p.broadcast(&EventPut(event.clone(), fake_blob)).await;
    sleep(5).await;

    for (i, eg) in nodes.iter().enumerate().skip(1) {
        let store = eg.dag_store.read().await;
        let slot = store.get_slot(&dag_ts).unwrap();
        assert!(
            !slot.main_tree.contains_key(event.id().as_bytes()).unwrap(),
            "node {i} accepted a genesis-shaped event with a blob",
        );
    }

    shutdown_network(&nodes).await;
}

#[test]
fn evgr_multi_node_dag_sync_with_blob() {
    init_logger();
    run_multi_node_test(dag_sync_with_blob);
}
async fn dag_sync_with_blob(ex: Arc<Executor<'static>>) {
    // End-to-end Vector-2 propagation test. Nodes 0..3 receive a
    // signal via direct insert (with the blob in their dag_blobs
    // side-table). Node 4 catches up via dag_sync - it should
    // receive both the event and the blob from a peer, and re-verify
    // the proof at sync time.
    let nodes = make_network(ex).await;

    let mut alice = TestIdentity::new();
    for eg in &nodes {
        alice.register_directly(eg).await.unwrap();
    }

    let dag_ts = nodes[0].current_genesis.read().await.header.timestamp;
    let dag_name = dag_ts.to_string();

    let event = Event::new(b"synced-message".to_vec(), &nodes[0]).await;
    let message_id = alice.next_message_id(event.header.timestamp).expect("budget");
    let blob_struct = alice.create_signal(&event, message_id, &nodes[0]).await.unwrap();
    let blob = serialize_async(&blob_struct).await;

    for eg in nodes.iter().take(4) {
        eg.header_dag_insert(vec![event.header.clone()], &dag_name).await.unwrap();
        eg.dag_insert(slice::from_ref(&event), &dag_name).await.unwrap();
        eg.dag_blob_store(&event.id(), &blob).unwrap();
    }

    {
        let store = nodes[4].dag_store.read().await;
        let slot = store.get_slot(&dag_ts).unwrap();
        assert!(!slot.main_tree.contains_key(event.id().as_bytes()).unwrap());
    }

    nodes[4].dag_sync(dag_ts).await.expect("dag_sync should succeed");
    sleep(2).await;

    let store = nodes[4].dag_store.read().await;
    let slot = store.get_slot(&dag_ts).unwrap();
    assert!(
        slot.main_tree.contains_key(event.id().as_bytes()).unwrap(),
        "sync should bring the event over",
    );
    drop(store);
    assert!(
        nodes[4].dag_blob_fetch(&event.id()).unwrap().is_some(),
        "sync should bring the blob over too - without it, node 4 can't serve future late-joiners",
    );

    shutdown_network(&nodes).await;
}

#[test]
fn evgr_multi_node_dag_sync_rejects_bad_blob() {
    init_logger();
    run_multi_node_test(dag_sync_rejects_bad_blob);
}
async fn dag_sync_rejects_bad_blob(ex: Arc<Executor<'static>>) {
    // Vector-2 defense: a peer in the 2/3 quorum serves a tampered
    // blob during sync. The recipient's dag_insert_with_blobs runs
    // RLN re-verification, which rejects, and the event does NOT
    // end up in the recipient's main_tree.
    let nodes = make_network(ex).await;

    let alice = TestIdentity::new();
    for eg in &nodes {
        alice.register_directly(eg).await.unwrap();
    }

    let dag_ts = nodes[0].current_genesis.read().await.header.timestamp;
    let dag_name = dag_ts.to_string();
    let event = Event::new(b"crafted-injection".to_vec(), &nodes[0]).await;

    // Garbage bytes - won't deserialize as a real RLN signal, won't
    // verify. We don't need a real (failing) proof to exercise the
    // rejection path.
    let bad_blob = b"definitely-not-a-real-rln-blob".to_vec();

    for eg in nodes.iter().take(4) {
        eg.header_dag_insert(vec![event.header.clone()], &dag_name).await.unwrap();
        eg.dag_insert(slice::from_ref(&event), &dag_name).await.unwrap();
        eg.dag_blob_store(&event.id(), &bad_blob).unwrap();
    }

    nodes[4].dag_sync(dag_ts).await.unwrap();
    sleep(2).await;

    let store = nodes[4].dag_store.read().await;
    let slot = store.get_slot(&dag_ts).unwrap();
    assert!(
        !slot.main_tree.contains_key(event.id().as_bytes()).unwrap(),
        "Vector-2 breach: node 4 accepted an event with a tampered blob during sync",
    );

    shutdown_network(&nodes).await;
}

#[test]
fn evgr_multi_node_dormant_user_can_post_after_long_silence() {
    init_logger();
    run_multi_node_test(dormant_user_can_post_after_long_silence);
}
async fn dormant_user_can_post_after_long_silence(ex: Arc<Executor<'static>>) {
    // Alice registers, then 17+ other identities also register. By
    // the time Alice tries to send a signal, her registration root
    // has long fallen out of the in-memory recent_roots window. The
    // historical-roots side-table makes verification succeed anyway.
    //
    // Note: in this test Alice's signal still references the CURRENT
    // root (because `rln_membership_path` returns the live root and
    // Alice is still a member). That's fine - what we're validating
    // here is end-to-end: a deeply-historical state of the SMT
    // doesn't break verification. The unit tests in tests_rln.rs
    // (rln_is_root_valid_at_*) cover the predicate's exact semantics
    // for old-root references.
    let nodes = make_network(ex).await;

    let mut alice = TestIdentity::new();
    for eg in &nodes {
        alice.register_directly(eg).await.unwrap();
    }

    // Push the recent_roots window past Alice's registration.
    for seed in 100..117_u64 {
        let other = TestIdentity::with_seed(seed);
        for eg in &nodes {
            other.register_directly(eg).await.unwrap();
        }
    }

    let event = Event::new(b"long-silent-but-still-registered".to_vec(), &nodes[0]).await;
    let message_id = alice.next_message_id(event.header.timestamp).expect("budget");
    let blob_struct = alice.create_signal(&event, message_id, &nodes[0]).await.unwrap();
    let blob = serialize_async(&blob_struct).await;

    let dag_ts = nodes[0].current_genesis.read().await.header.timestamp;
    let dag_name = dag_ts.to_string();
    nodes[0].header_dag_insert(vec![event.header.clone()], &dag_name).await.unwrap();
    nodes[0].dag_insert(slice::from_ref(&event), &dag_name).await.unwrap();
    nodes[0].dag_blob_store(&event.id(), &blob).unwrap();
    nodes[0].p2p.broadcast(&EventPut(event.clone(), blob)).await;

    sleep(5).await;

    for (i, eg) in nodes.iter().enumerate() {
        let store = eg.dag_store.read().await;
        let slot = store.get_slot(&dag_ts).unwrap();
        assert!(
            slot.main_tree.contains_key(event.id().as_bytes()).unwrap(),
            "node {i} rejected Alice's signal even though she's a valid registered identity \
             - historical-roots fallback is broken",
        );
    }

    shutdown_network(&nodes).await;
}
