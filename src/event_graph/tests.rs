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
        filter_requested_event_rep, merge_static_sync_event_rep,
        proto::{
            cap_layer_tips, count_layer_tips, filter_parent_event_rep, EventPut, SyncDirection,
            MAX_HEADER_REP_HEADERS, MAX_RANGE_PAGE_SIZE,
        },
        rln::epoch_of,
        test_helpers::{
            archive_config, bounded_dag_store_config, init_logger, make_eg, make_eg_with_config,
            make_network, run_multi_node_test, shutdown_network, test_config, TestIdentity,
        },
        util::{millis_until_next_rotation, next_hour_timestamp, next_rotation_timestamp},
        DagStore, Event, EventGraphConfig, EventGraphPtr, LayerUTips, TimeIndex, NULL_ID,
        NULL_PARENTS, N_EVENT_PARENTS,
    },
    system::{sleep, timeout::timeout},
};

fn test_event(content: &[u8], layer: u64) -> Event {
    Event {
        header: Header {
            timestamp: 1_704_067_200_000 + layer,
            parents: NULL_PARENTS,
            layer,
            content_hash: blake3::hash(content),
        },
        content: content.to_vec(),
    }
}

#[test]
fn evgr_event_rep_filter_matches_only_requested_ids() {
    let event_a = test_event(b"requested-a", 1);
    let event_b = test_event(b"requested-b", 2);
    let unrelated = test_event(b"unrelated", 3);
    let requested = vec![event_a.id(), event_b.id()];

    let (events, blobs, missing) =
        filter_requested_event_rep(&requested, vec![event_b.clone()], vec![b"blob-b".to_vec()])
            .unwrap();
    assert_eq!(events.iter().map(Event::id).collect::<Vec<_>>(), vec![event_b.id()]);
    assert_eq!(blobs, vec![b"blob-b".to_vec()]);
    assert_eq!(missing, vec![event_a.id()]);

    assert!(filter_requested_event_rep(
        &requested,
        vec![unrelated],
        vec![b"unrelated-blob".to_vec()],
    )
    .is_err());

    assert!(filter_requested_event_rep(
        &requested,
        vec![event_a.clone(), event_a.clone()],
        vec![b"blob-a".to_vec(), b"duplicate-blob".to_vec()],
    )
    .is_err());

    assert!(filter_requested_event_rep(&requested, vec![event_a], Vec::new()).is_err());
}

#[test]
fn evgr_parent_event_rep_requires_progress() {
    let parent_a = test_event(b"parent-a", 21);
    let parent_b = test_event(b"parent-b", 22);
    let unrelated = test_event(b"unrelated-parent", 23);
    let requested = vec![parent_a.id(), parent_b.id()];

    let mut missing: HashSet<_> = requested.iter().copied().collect();
    let mut known = HashSet::new();
    assert!(filter_parent_event_rep(&requested, &mut missing, &mut known, vec![], vec![]).is_err());
    assert_eq!(missing.len(), 2);
    assert!(known.is_empty());

    assert!(filter_parent_event_rep(
        &requested,
        &mut missing,
        &mut known,
        vec![unrelated],
        vec![b"blob".to_vec()],
    )
    .is_err());
    assert_eq!(missing.len(), 2);
    assert!(known.is_empty());

    assert!(filter_parent_event_rep(
        &requested,
        &mut missing,
        &mut known,
        vec![parent_a.clone(), parent_a.clone()],
        vec![b"blob-a".to_vec(), b"blob-a-duplicate".to_vec()],
    )
    .is_err());
    assert_eq!(missing.len(), 2);
    assert!(known.is_empty());

    let resolved = filter_parent_event_rep(
        &requested,
        &mut missing,
        &mut known,
        vec![parent_a.clone()],
        vec![b"blob-a".to_vec()],
    )
    .unwrap();
    assert_eq!(
        resolved.iter().map(|(event, _)| event.id()).collect::<Vec<_>>(),
        vec![parent_a.id()]
    );
    assert!(!missing.contains(&parent_a.id()));
    assert!(missing.contains(&parent_b.id()));
    assert!(known.contains(&parent_a.id()));

    let current_request = vec![parent_b.id()];
    assert!(filter_parent_event_rep(
        &current_request,
        &mut missing,
        &mut known,
        vec![parent_a.clone()],
        vec![b"stale-blob-a".to_vec()],
    )
    .is_err());
    assert!(missing.contains(&parent_b.id()));

    let mut empty_missing = HashSet::new();
    let mut empty_known = HashSet::new();
    assert!(filter_parent_event_rep(
        &[parent_a.id()],
        &mut empty_missing,
        &mut empty_known,
        vec![parent_a],
        vec![b"blob-a".to_vec()],
    )
    .is_err());
}

#[test]
fn evgr_static_sync_merge_tracks_partial_requested_batches() {
    let event_a = test_event(b"static-requested-a", 11);
    let event_b = test_event(b"static-requested-b", 12);
    let unrelated = test_event(b"static-unrelated", 13);
    let requested = vec![event_a.id(), event_b.id()];
    let mut pending: HashSet<_> = requested.iter().copied().collect();
    let mut known = HashSet::new();
    let mut want = HashSet::new();
    let mut fetched = Vec::new();

    assert!(merge_static_sync_event_rep(
        &requested,
        &mut pending,
        &mut known,
        &mut want,
        &mut fetched,
        vec![unrelated],
        vec![b"unrelated-blob".to_vec()],
    )
    .is_err());
    assert_eq!(pending.len(), 2);
    assert!(fetched.is_empty());

    let matched = merge_static_sync_event_rep(
        &requested,
        &mut pending,
        &mut known,
        &mut want,
        &mut fetched,
        vec![event_b.clone()],
        vec![b"blob-b".to_vec()],
    )
    .unwrap();
    assert_eq!(matched, 1);
    assert_eq!(pending, HashSet::from([event_a.id()]));

    let matched = merge_static_sync_event_rep(
        &requested,
        &mut pending,
        &mut known,
        &mut want,
        &mut fetched,
        vec![event_a.clone()],
        vec![b"blob-a".to_vec()],
    )
    .unwrap();
    assert_eq!(matched, 1);
    assert!(pending.is_empty());

    let fetched_ids: HashSet<_> = fetched.iter().map(|(ev, _)| ev.id()).collect();
    assert_eq!(fetched_ids, HashSet::from([event_a.id(), event_b.id()]));
}

#[test]
fn evgr_layer_tip_cap_is_bounded() {
    let mut tips = LayerUTips::new();
    tips.entry(0).or_default().insert(blake3::hash(b"tip-0"));
    tips.entry(1).or_default().insert(blake3::hash(b"tip-1"));
    tips.entry(1).or_default().insert(blake3::hash(b"tip-2"));

    let capped = cap_layer_tips(&tips, 2);
    assert_eq!(count_layer_tips(&capped), 2);
    assert!(capped.get(&0).is_some_and(|layer| layer.len() == 1));
}

#[test]
fn evgr_parent_selection_does_not_wrap_saturated_layer() {
    let tip = blake3::hash(b"saturated-tip");
    let tips = LayerUTips::from([(u64::MAX, HashSet::from([tip]))]);

    let (layer, parents) = super::select_parents_from_tips(&tips);

    assert_eq!(layer, u64::MAX);
    assert_eq!(parents[0], tip);
}

#[test]
fn evgr_config_rejects_invalid_rotation_settings() {
    let zero_dags = EventGraphConfig { max_dags: Some(0), ..test_config() };
    assert!(zero_dags.validate().is_err());

    let overflowing_rotation = EventGraphConfig { hours_rotation: u64::MAX, ..test_config() };
    assert!(overflowing_rotation.validate().is_err());

    let overflowing_genesis =
        EventGraphConfig { initial_genesis: u64::MAX, hours_rotation: 1, ..test_config() };
    assert!(overflowing_genesis.validate().is_err());
}

#[test]
fn evgr_invalid_config_does_not_open_dag_trees() {
    smol::block_on(async {
        let sled_db = sled::Config::new().temporary(true).open().unwrap();
        let before = sled_db.tree_names();
        let config = EventGraphConfig { hours_rotation: 1, max_dags: Some(0), ..test_config() };

        let result = DagStore::new(sled_db.clone(), &config).await;

        assert!(matches!(result, Err(crate::Error::Custom(_))));
        assert_eq!(sled_db.tree_names(), before);
    })
}

#[test]
fn evgr_rotation_helpers_are_total() {
    const HOUR_MS: u64 = 3_600_000;

    assert!(next_rotation_timestamp(0, 0).is_err());

    let now = UNIX_EPOCH.elapsed().unwrap().as_millis() as u64;
    let future_start = now + HOUR_MS;
    assert_eq!(next_rotation_timestamp(future_start, 1).unwrap(), future_start);
    assert!(millis_until_next_rotation(now.saturating_sub(1)).is_err());
    assert_eq!(super::util::hours_since(future_start).unwrap(), 0);
}

#[test]
fn evgr_time_index_queries_and_saturating_cursor() {
    // Forward, backward, newest, oldest queries plus the saturating
    // cursor at u64 boundaries.
    let mut idx = TimeIndex::new();
    for (i, ts) in [100_u64, 200, 200, 300, 400, 500].into_iter().enumerate() {
        let id = blake3::hash(&[ts.to_be_bytes(), (i as u64).to_be_bytes()].concat());
        idx.insert(ts, id);
    }
    assert_eq!(idx.len(), 6);
    assert_eq!(idx.newest(3).len(), 3);
    assert_eq!(idx.oldest(2).len(), 2);
    assert_eq!(idx.before(300, 10).len(), 3);
    assert_eq!(idx.after(200, 10).len(), 3);

    let duplicate = blake3::hash(b"duplicate-time-index-entry");
    idx.insert(600, duplicate);
    idx.insert(600, duplicate);
    assert_eq!(idx.len(), 7);
    assert_eq!(idx.newest(10).iter().filter(|id| **id == duplicate).count(), 1);

    // Saturating cursor: before(0) shouldn't underflow,
    // after(u64::MAX) shouldn't overflow.
    let mut idx2 = TimeIndex::new();
    idx2.insert(100, blake3::hash(b"x"));
    assert_eq!(idx2.before(0, 10).len(), 0);
    assert_eq!(idx2.after(u64::MAX, 10).len(), 0);
}

async fn make_dag_store() -> Result<DagStore> {
    let sled_db = sled::Config::new().temporary(true).open().unwrap();
    DagStore::new(sled_db, &bounded_dag_store_config()).await
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
        let new_ts = next_hour_timestamp(1).unwrap();
        let hdr = Header {
            timestamp: new_ts,
            parents: NULL_PARENTS,
            layer: 0,
            content_hash: blake3::hash(b"test-graph-v1"),
        };
        let genesis = Event { header: hdr, content: b"test-graph-v1".to_vec() };
        store.add_dag(&genesis, Some(24)).await.unwrap();
        assert_eq!(store.dag_timestamps().len(), 24);
        assert!(store.get_slot(&new_ts).is_some());
        assert!(store.get_slot(&oldest_ts).is_none());

        // (b) archive
        let sled_db = sled::Config::new().temporary(true).open().unwrap();
        let mut archive = DagStore::new(sled_db, &archive_config()).await.unwrap();
        let initial = archive.dag_timestamps().len();
        for i in 1..=30i64 {
            let ts = next_hour_timestamp(i).unwrap();
            let hdr = Header {
                timestamp: ts,
                parents: NULL_PARENTS,
                layer: 0,
                content_hash: blake3::hash(b"test-graph-v1"),
            };
            let genesis = Event { header: hdr, content: b"test-graph-v1".to_vec() };
            archive.add_dag(&genesis, None).await.unwrap();
        }
        assert_eq!(archive.dag_timestamps().len(), initial + 30);
    })
}

#[test]
fn evgr_dag_store_archive_mode_discovers_existing_trees() {
    smol::block_on(async {
        let sled_db = sled::Config::new().temporary(true).open().unwrap();
        let historical_ts = next_hour_timestamp(-100).unwrap();
        {
            let mut store = DagStore::new(sled_db.clone(), &archive_config()).await.unwrap();
            let hdr = Header {
                timestamp: historical_ts,
                parents: NULL_PARENTS,
                layer: 0,
                content_hash: blake3::hash(b"test-graph-v1"),
            };
            let genesis = Event { header: hdr, content: b"test-graph-v1".to_vec() };
            store.add_dag(&genesis, None).await.unwrap();
            drop(store);
        }

        let store = DagStore::new(sled_db, &archive_config()).await.unwrap();
        assert!(
            store.get_slot(&historical_ts).is_some(),
            "Archive mode should discover historical DAGs on restart"
        );
    })
}

#[test]
fn evgr_dag_store_rejects_corrupt_header_index_on_open() {
    smol::block_on(async {
        let sled_db = sled::Config::new().temporary(true).open().unwrap();
        let config = bounded_dag_store_config();
        let store = DagStore::new(sled_db.clone(), &config).await.unwrap();
        let ts = *store.dag_timestamps().last().unwrap();
        drop(store);

        let bad_id = [0u8; 32];
        let headers = sled_db.open_tree(format!("headers_{ts}")).unwrap();
        headers.insert(bad_id.as_slice(), b"not-a-header".as_slice()).unwrap();

        let result = DagStore::new(sled_db, &config).await;
        assert!(result.is_err(), "corrupt header bytes should fail DAG startup");
    })
}

#[test]
fn evgr_dag_store_rejects_corrupt_event_tree_on_open() {
    smol::block_on(async {
        let sled_db = sled::Config::new().temporary(true).open().unwrap();
        let config = bounded_dag_store_config();
        let store = DagStore::new(sled_db.clone(), &config).await.unwrap();
        let ts = *store.dag_timestamps().last().unwrap();
        drop(store);

        let bad_id = [0u8; 32];
        let events = sled_db.open_tree(ts.to_string()).unwrap();
        events.insert(bad_id.as_slice(), b"not-an-event".as_slice()).unwrap();

        let result = DagStore::new(sled_db, &config).await;
        assert!(result.is_err(), "corrupt event bytes should fail DAG startup");
    })
}

#[test]
fn evgr_rotating_event_creation_rejects_missing_current_dag_slot() {
    smol::block_on(async {
        let eg = make_eg().await;
        let dag_ts = eg.current_genesis.read().await.header.timestamp;
        eg.dag_store.write().await.dags.remove(&dag_ts);

        let result = Event::new(b"missing-current-slot".to_vec(), &eg).await;
        assert!(matches!(result, Err(crate::Error::Custom(_))));
    })
}

#[test]
fn evgr_static_event_creation_rejects_corrupt_static_dag() {
    smol::block_on(async {
        let eg = make_eg().await;
        let bad_id = [0u8; 32];
        eg.static_dag.insert(bad_id.as_slice(), b"not-an-event".as_slice()).unwrap();

        let result = Event::new_static(b"static-after-corruption".to_vec(), &eg).await;
        assert!(result.is_err(), "corrupt static DAG should fail static event creation");
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

        let tips = compute_unreferenced_tips(&slot.main_tree).await.unwrap();

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

        let event = Event::new(b"hello".to_vec(), &eg).await.unwrap();
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
fn evgr_duplicate_header_insert_does_not_duplicate_time_index() {
    smol::block_on(async {
        let eg = make_eg().await;
        let dag_ts = eg.current_genesis.read().await.header.timestamp;
        let dag_name = dag_ts.to_string();
        let event = Event::new(b"time-index-dedup".to_vec(), &eg).await.unwrap();

        eg.header_dag_insert(vec![event.header.clone()], &dag_name).await.unwrap();
        let indexed_after_first = {
            let store = eg.dag_store.read().await;
            store.get_slot(&dag_ts).unwrap().time_index.len()
        };

        eg.header_dag_insert(vec![event.header.clone(), event.header.clone()], &dag_name)
            .await
            .unwrap();
        let indexed_after_duplicates = {
            let store = eg.dag_store.read().await;
            store.get_slot(&dag_ts).unwrap().time_index.len()
        };
        assert_eq!(indexed_after_duplicates, indexed_after_first);

        let ids = eg.dag_insert(slice::from_ref(&event), &dag_name).await.unwrap();
        assert_eq!(ids, vec![event.id()]);
        let page = eg.fetch_page(u64::MAX, SyncDirection::Backward, 10).await.unwrap();
        assert_eq!(page.iter().filter(|ev| ev.id() == event.id()).count(), 1);
    })
}

#[test]
fn evgr_dag_insert_without_header_skipped() {
    smol::block_on(async {
        let eg = make_eg().await;
        let dag_name = eg.current_genesis.read().await.header.timestamp.to_string();
        let event = Event::new(b"orphan".to_vec(), &eg).await.unwrap();
        let ids = eg.dag_insert(slice::from_ref(&event), &dag_name).await.unwrap();
        assert!(ids.is_empty());
    })
}

#[test]
fn evgr_header_insert_rejects_layer_jump() {
    smol::block_on(async {
        let eg = make_eg().await;
        let genesis = eg.current_genesis.read().await.clone();
        let dag_name = genesis.header.timestamp.to_string();
        let mut parents = [NULL_ID; N_EVENT_PARENTS];
        parents[0] = genesis.id();

        let event = Event {
            header: Header {
                timestamp: UNIX_EPOCH.elapsed().unwrap().as_millis() as u64,
                parents,
                layer: 2,
                content_hash: blake3::hash(b"layer-jump"),
            },
            content: b"layer-jump".to_vec(),
        };

        let err = eg.header_dag_insert(vec![event.header], &dag_name).await.unwrap_err();
        assert!(matches!(err, crate::Error::HeaderIsInvalid));
    })
}

#[test]
fn evgr_header_insert_rejects_duplicate_parents() {
    smol::block_on(async {
        let eg = make_eg().await;
        let genesis = eg.current_genesis.read().await.clone();
        let dag_name = genesis.header.timestamp.to_string();
        let mut parents = [NULL_ID; N_EVENT_PARENTS];
        parents[0] = genesis.id();
        parents[1] = genesis.id();

        let event = Event {
            header: Header {
                timestamp: UNIX_EPOCH.elapsed().unwrap().as_millis() as u64,
                parents,
                layer: 1,
                content_hash: blake3::hash(b"duplicate-parents"),
            },
            content: b"duplicate-parents".to_vec(),
        };

        let err = eg.header_dag_insert(vec![event.header], &dag_name).await.unwrap_err();
        assert!(matches!(err, crate::Error::HeaderIsInvalid));
    })
}

#[test]
fn evgr_header_validate_rejects_layer_overflow_parent() {
    smol::block_on(async {
        let db = sled::Config::new().temporary(true).open().unwrap();
        let tree = db.open_tree("headers").unwrap();
        let timestamp = UNIX_EPOCH.elapsed().unwrap().as_millis() as u64;
        let parent = Header {
            timestamp,
            parents: NULL_PARENTS,
            layer: u64::MAX,
            content_hash: blake3::hash(b"overflow-parent"),
        };
        tree.insert(parent.id().as_bytes(), serialize_async(&parent).await).unwrap();

        let mut parents = [NULL_ID; N_EVENT_PARENTS];
        parents[0] = parent.id();
        let child = Header {
            timestamp,
            parents,
            layer: u64::MAX,
            content_hash: blake3::hash(b"overflow-child"),
        };

        assert!(!child.validate(&tree, &test_config(), timestamp, None).await.unwrap());
    })
}

#[test]
fn evgr_header_validate_uses_target_slot_bounds() {
    smol::block_on(async {
        const HOUR_MS: u64 = 3_600_000;

        let db = sled::Config::new().temporary(true).open().unwrap();
        let tree = db.open_tree("headers").unwrap();
        let dag_ts = 1_704_067_200_000;
        let drift = crate::event_graph::EVENT_TIME_DRIFT;
        let config = EventGraphConfig { hours_rotation: 6, ..test_config() };
        let genesis = Header {
            timestamp: dag_ts,
            parents: NULL_PARENTS,
            layer: 0,
            content_hash: blake3::hash(&config.genesis_contents),
        };
        tree.insert(genesis.id().as_bytes(), serialize_async(&genesis).await).unwrap();

        let mut parents = [NULL_ID; N_EVENT_PARENTS];
        parents[0] = genesis.id();
        let make_header = |timestamp, content: &[u8]| Header {
            timestamp,
            parents,
            layer: 1,
            content_hash: blake3::hash(content),
        };

        let lower_edge = make_header(dag_ts.saturating_sub(drift), b"lower-edge");
        assert!(lower_edge.validate(&tree, &config, dag_ts, None).await.unwrap());

        let upper_edge = make_header(dag_ts + 6 * HOUR_MS + drift - 1, b"upper-edge");
        assert!(upper_edge.validate(&tree, &config, dag_ts, None).await.unwrap());

        let too_early = make_header(dag_ts - drift - 1, b"too-early");
        assert!(!too_early.validate(&tree, &config, dag_ts, None).await.unwrap());

        let too_late = make_header(dag_ts + 6 * HOUR_MS + drift, b"too-late");
        assert!(!too_late.validate(&tree, &config, dag_ts, None).await.unwrap());
    })
}

#[test]
fn evgr_header_validate_ignores_future_initial_genesis() {
    smol::block_on(async {
        const HOUR_MS: u64 = 3_600_000;

        let db = sled::Config::new().temporary(true).open().unwrap();
        let tree = db.open_tree("headers").unwrap();
        let now = UNIX_EPOCH.elapsed().unwrap().as_millis() as u64;
        let dag_ts = now.saturating_sub(HOUR_MS);
        let config =
            EventGraphConfig { initial_genesis: now + HOUR_MS, hours_rotation: 1, ..test_config() };
        let genesis = Header {
            timestamp: dag_ts,
            parents: NULL_PARENTS,
            layer: 0,
            content_hash: blake3::hash(&config.genesis_contents),
        };
        tree.insert(genesis.id().as_bytes(), serialize_async(&genesis).await).unwrap();

        let mut parents = [NULL_ID; N_EVENT_PARENTS];
        parents[0] = genesis.id();
        let child = Header {
            timestamp: dag_ts + 1,
            parents,
            layer: 1,
            content_hash: blake3::hash(b"future-initial-genesis"),
        };

        assert!(child.validate(&tree, &config, dag_ts, None).await.unwrap());
    })
}

#[test]
fn evgr_header_validate_no_rotation_rejects_far_future() {
    smol::block_on(async {
        let db = sled::Config::new().temporary(true).open().unwrap();
        let tree = db.open_tree("headers").unwrap();
        let config = test_config();
        let dag_ts = config.initial_genesis;
        let genesis = Header {
            timestamp: dag_ts,
            parents: NULL_PARENTS,
            layer: 0,
            content_hash: blake3::hash(&config.genesis_contents),
        };
        tree.insert(genesis.id().as_bytes(), serialize_async(&genesis).await).unwrap();

        let mut parents = [NULL_ID; N_EVENT_PARENTS];
        parents[0] = genesis.id();
        let old_history = Header {
            timestamp: dag_ts + 1,
            parents,
            layer: 1,
            content_hash: blake3::hash(b"old-no-rotation-history"),
        };
        assert!(old_history.validate(&tree, &config, dag_ts, None).await.unwrap());

        let future = Header {
            timestamp: UNIX_EPOCH.elapsed().unwrap().as_millis() as u64 +
                crate::event_graph::EVENT_TIME_DRIFT +
                1,
            parents,
            layer: 1,
            content_hash: blake3::hash(b"future-no-rotation-header"),
        };
        assert!(!future.validate(&tree, &config, dag_ts, None).await.unwrap());
    })
}

#[test]
fn evgr_header_insert_rejects_unloaded_dag_slot() {
    smol::block_on(async {
        let config = EventGraphConfig { hours_rotation: 1, max_dags: Some(2), ..test_config() };
        let eg = make_eg_with_config(config).await;
        let dag_ts = next_hour_timestamp(-100).unwrap();
        let dag_name = dag_ts.to_string();
        let genesis = Header {
            timestamp: dag_ts,
            parents: NULL_PARENTS,
            layer: 0,
            content_hash: blake3::hash(&eg.config.genesis_contents),
        };
        let mut parents = [NULL_ID; N_EVENT_PARENTS];
        parents[0] = genesis.id();
        let header = Header {
            timestamp: dag_ts + 1,
            parents,
            layer: 1,
            content_hash: blake3::hash(b"unloaded-slot"),
        };

        let err = eg.header_dag_insert(vec![header], &dag_name).await.unwrap_err();
        assert!(matches!(err, crate::Error::DagSyncFailed));
    })
}

#[test]
fn evgr_fetch_headers_with_tips_is_bounded_and_layer_ordered() {
    smol::block_on(async {
        let eg = make_eg().await;
        let dag_ts = eg.current_genesis.read().await.header.timestamp;
        let dag_name = dag_ts.to_string();
        let base = UNIX_EPOCH.elapsed().unwrap().as_millis() as u64;
        let mut parent = eg.current_genesis.read().await.id();
        let mut headers = Vec::with_capacity(MAX_HEADER_REP_HEADERS + 32);

        for i in 0..(MAX_HEADER_REP_HEADERS + 32) {
            let mut parents = [NULL_ID; N_EVENT_PARENTS];
            parents[0] = parent;
            let content = format!("bounded-header-{i}");
            let header = Header {
                timestamp: base + i as u64,
                parents,
                layer: i as u64 + 1,
                content_hash: blake3::hash(content.as_bytes()),
            };
            parent = header.id();
            headers.push(header);
        }

        eg.header_dag_insert(headers, &dag_name).await.unwrap();

        let hostile_empty_tips = LayerUTips::new();
        let response = eg.fetch_headers_with_tips(&dag_name, &hostile_empty_tips).await.unwrap();
        assert_eq!(response.len(), MAX_HEADER_REP_HEADERS);
        for pair in response.windows(2) {
            assert!(pair[0].layer <= pair[1].layer);
        }
        assert_eq!(response.first().unwrap().layer, 0);
        assert!(response.last().unwrap().layer < MAX_HEADER_REP_HEADERS as u64);
    })
}

#[test]
fn evgr_fetch_page_both_directions() {
    smol::block_on(async {
        let eg = make_eg().await;
        let dag_name = eg.current_genesis.read().await.header.timestamp.to_string();
        let base = UNIX_EPOCH.elapsed().unwrap().as_millis() as u64;
        for i in 0..(MAX_RANGE_PAGE_SIZE as u64 + 10) {
            let ev = Event::with_timestamp(base + i, vec![(i % 251) as u8], &eg).await.unwrap();
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

        let capped = eg
            .fetch_page(u64::MAX, SyncDirection::Backward, MAX_RANGE_PAGE_SIZE + 10)
            .await
            .unwrap();
        assert_eq!(capped.len(), MAX_RANGE_PAGE_SIZE);
    })
}

#[test]
fn evgr_order_events_rejects_corrupt_event_record() {
    smol::block_on(async {
        let eg = make_eg().await;
        let dag_ts = eg.current_genesis.read().await.header.timestamp;
        let bad_id = [0u8; 32];
        {
            let store = eg.dag_store.read().await;
            let slot = store.get_slot(&dag_ts).unwrap();
            slot.main_tree.insert(bad_id.as_slice(), b"not-an-event".as_slice()).unwrap();
        }

        let result = eg.order_events().await;
        assert!(result.is_err(), "corrupt event records should fail history ordering");
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
    let event = Event::new(b"hello-via-rln".to_vec(), &nodes[0]).await.unwrap();

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
    let event = Event::new(b"unauthenticated".to_vec(), &nodes[0]).await.unwrap();

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
fn evgr_multi_node_malformed_event_rejected_before_rln() {
    init_logger();
    run_multi_node_test(malformed_event_rejected_before_rln);
}
async fn malformed_event_rejected_before_rln(ex: Arc<Executor<'static>>) {
    let nodes = make_network(ex).await;

    let mut alice = TestIdentity::new();
    for eg in &nodes {
        alice.register_directly(eg).await.unwrap();
    }

    let dag_ts = nodes[0].current_genesis.read().await.header.timestamp;
    let event = Event::new(b"preflight-live".to_vec(), &nodes[0]).await.unwrap();
    let message_id = alice.next_message_id(event.header.timestamp).expect("budget");
    let blob = alice.create_signal(&event, message_id, &nodes[0]).await.unwrap();
    let internal_nullifier = blob.internal_nullifier;
    let blob = serialize_async(&blob).await;

    let mut malformed = event.clone();
    malformed.content.extend_from_slice(b"-tampered");
    assert!(!malformed.content_matches_header());

    nodes[0].p2p.broadcast(&EventPut(malformed.clone(), blob)).await;
    sleep(5).await;

    let epoch = epoch_of(malformed.header.timestamp);
    for (i, eg) in nodes.iter().enumerate().skip(1) {
        let store = eg.dag_store.read().await;
        let slot = store.get_slot(&dag_ts).unwrap();
        assert!(
            !slot.main_tree.contains_key(malformed.id().as_bytes()).unwrap(),
            "node {i} accepted a structurally invalid event",
        );
        drop(store);

        let state = eg.rln_state.read().await;
        assert!(
            !state.metadata.is_reused(epoch, &internal_nullifier),
            "node {i} ran RLN verification before structural rejection",
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
fn evgr_fetch_missing_events_rejects_corrupt_header_record() {
    smol::block_on(async {
        let eg = make_eg().await;
        let dag_ts = eg.current_genesis.read().await.header.timestamp;
        let dag_name = dag_ts.to_string();
        let bad_id = [0u8; 32];
        {
            let store = eg.dag_store.read().await;
            let slot = store.get_slot(&dag_ts).unwrap();
            slot.header_tree.insert(bad_id.as_slice(), b"not-a-header".as_slice()).unwrap();
        }

        let result = eg.fetch_missing_events(dag_ts, &dag_name, 1).await;
        assert!(result.is_err(), "corrupt header records should fail DAG content sync");
    })
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

    let event = Event::new(b"synced-message".to_vec(), &nodes[0]).await.unwrap();
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
    let event = Event::new(b"crafted-injection".to_vec(), &nodes[0]).await.unwrap();

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

    let event = Event::new(b"long-silent-but-still-registered".to_vec(), &nodes[0]).await.unwrap();
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
