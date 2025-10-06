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

// cargo test --release --features=event-graph --lib eventgraph_propagation -- --include-ignored

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    slice,
    sync::Arc,
    time::{Duration, UNIX_EPOCH},
};

use darkfi_serial::{deserialize_async, serialize_async};
use rand::{prelude::SliceRandom, rngs::ThreadRng};
use sled_overlay::sled;
use smol::{channel, future, Executor};
use tracing::{info, warn};
use url::Url;

use crate::{
    error::Result,
    event_graph::{
        event::Header,
        proto::{EventPut, ProtocolEventGraph},
        util::next_rotation_timestamp,
        DAGStore, Event, EventGraph, EventGraphPtr, DAGS_MAX_NUMBER, GENESIS_CONTENTS,
        INITIAL_GENESIS, NULL_ID, N_EVENT_PARENTS,
    },
    net::{session::SESSION_DEFAULT, settings::NetworkProfile, P2p, Settings},
    system::{msleep, sleep, timeout::timeout},
    util::logger::{setup_test_logger, Level},
    Error,
};

// Number of nodes to spawn and number of peers each node connects to
const N_NODES: usize = 5;
const N_CONNS: usize = 2;
//const N_NODES: usize = 50;
//const N_CONNS: usize = N_NODES / 3;

fn init_logger() {
    let ignored_targets = [
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
    // We check this error so we can execute same file tests in parallel,
    // otherwise second one fails to init logger here.
    if setup_test_logger(
        &ignored_targets,
        false,
        Level::Info,
        //Level::Verbose,
        //Level::Debug,
        //Level::Tracing,
    )
    .is_err()
    {
        warn!(target: "test_harness", "Logger already initialized");
    }
}

async fn spawn_node(
    inbound_addrs: Vec<Url>,
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
        inbound_addrs,
        outbound_connections: 0,
        inbound_connections: usize::MAX,
        peers,
        active_profiles: vec!["tcp".to_string()],
        profiles,
        ..Default::default()
    };

    let p2p = P2p::new(settings, ex.clone()).await.unwrap();
    let sled_db = sled::Config::new().temporary(true).open().unwrap();
    let event_graph =
        EventGraph::new(p2p.clone(), sled_db, "/tmp".into(), false, false, 1, ex.clone())
            .await
            .unwrap();
    *event_graph.synced.write().await = true;
    let event_graph_ = event_graph.clone();

    // Register the P2P protocols
    let registry = p2p.protocol_registry();
    registry
        .register(SESSION_DEFAULT, move |channel, _| {
            let event_graph_ = event_graph_.clone();
            async move { ProtocolEventGraph::init(event_graph_, channel).await.unwrap() }
        })
        .await;

    event_graph
}

async fn bootstrap_nodes(
    peer_indexes: &[usize],
    starting_port: usize,
    rng: &mut ThreadRng,
    ex: Arc<Executor<'static>>,
) -> Vec<Arc<EventGraph>> {
    let mut eg_instances = vec![];

    // Initialize the nodes
    for i in 0..N_NODES {
        // Everyone will connect to N_CONNS random peers.
        let mut peer_indexes_copy = peer_indexes.to_owned();
        peer_indexes_copy.remove(i);
        let peer_indexes_to_connect: Vec<_> =
            peer_indexes_copy.choose_multiple(rng, N_CONNS).collect();

        let mut peers = vec![];
        for peer_index in peer_indexes_to_connect {
            let port = starting_port + peer_index;
            peers.push(Url::parse(&format!("tcp://127.0.0.1:{port}")).unwrap());
        }

        let event_graph = spawn_node(
            vec![Url::parse(&format!("tcp://127.0.0.1:{}", starting_port + i)).unwrap()],
            peers,
            ex.clone(),
        )
        .await;

        eg_instances.push(event_graph);
    }

    // Start the P2P network
    for eg in eg_instances.iter() {
        eg.p2p.clone().start().await.unwrap();
    }

    info!("Waiting 5s until all peers connect");
    sleep(5).await;

    eg_instances
}

async fn assert_dags(eg_instances: &[Arc<EventGraph>], expected_len: usize, rng: &mut ThreadRng) {
    let random_node = eg_instances.choose(rng).unwrap();
    let random_node_genesis = random_node.current_genesis.read().await.header.timestamp;
    let store = random_node.dag_store.read().await;
    let (_, unreferenced_tips) = store.main_dags.get(&random_node_genesis).unwrap();
    let last_layer_tips = unreferenced_tips.last_key_value().unwrap().1.clone();
    for (i, eg) in eg_instances.iter().enumerate() {
        let current_genesis = eg.current_genesis.read().await;
        let dag_name = current_genesis.header.timestamp.to_string();
        let dag = eg.dag_store.read().await.get_dag(&dag_name);
        let unreferenced_tips = eg.dag_store.read().await.find_unreferenced_tips(&dag).await;
        let node_last_layer_tips = unreferenced_tips.last_key_value().unwrap().1.clone();
        assert!(
            dag.len() == expected_len,
            "Node {i}, expected {expected_len} events, have {}",
            dag.len()
        );
        assert_eq!(
            node_last_layer_tips, last_layer_tips,
            "Node {i} contains malformed unreferenced tips"
        );
    }
}

macro_rules! test_body {
    ($real_call:ident) => {
        init_logger();

        let ex = Arc::new(Executor::new());
        let ex_ = ex.clone();
        let (signal, shutdown) = channel::unbounded::<()>();

        // Run a thread for each node.
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
fn eventgraph_propagation() {
    test_body!(eventgraph_propagation_real);
}

async fn eventgraph_propagation_real(ex: Arc<Executor<'static>>) {
    let mut rng = rand::thread_rng();
    let peer_indexes: Vec<usize> = (0..N_NODES).collect();

    // Bootstrap nodes
    let mut eg_instances = bootstrap_nodes(&peer_indexes, 13200, &mut rng, ex.clone()).await;

    // Grab genesis event
    let random_node = eg_instances.choose(&mut rng).unwrap();
    let current_genesis = random_node.current_genesis.read().await;
    let dag_name = current_genesis.header.timestamp.to_string();
    let (id, _) = random_node.dag_store.read().await.get_dag(&dag_name).last().unwrap().unwrap();
    let genesis_event_id = blake3::Hash::from_bytes((&id as &[u8]).try_into().unwrap());

    drop(current_genesis);

    // =========================================
    // 1. Assert that everyone's DAG is the same
    // =========================================
    assert_dags(&eg_instances, 1, &mut rng).await;

    // ==========================================
    // 2. Create an event in one node and publish
    // ==========================================
    let random_node = eg_instances.choose(&mut rng).unwrap();
    let current_genesis = random_node.current_genesis.read().await;
    let dag_name = current_genesis.header.timestamp.to_string();
    let event = Event::new(vec![1, 2, 3, 4], random_node).await;
    assert!(event.header.parents.contains(&genesis_event_id));
    // The node adds it to their DAG, on layer 1.
    random_node.header_dag_insert(vec![event.header.clone()], &dag_name).await.unwrap();
    let event_id = random_node.dag_insert(slice::from_ref(&event), &dag_name).await.unwrap()[0];

    let store = random_node.dag_store.read().await;
    let (_, tips_layers) = store.header_dags.get(&current_genesis.header.timestamp).unwrap();

    // Since genesis was referenced, its layer (0) have been removed
    assert_eq!(tips_layers.len(), 1);
    assert!(tips_layers.last_key_value().unwrap().1.get(&event_id).is_some());
    drop(store);
    drop(current_genesis);
    info!("Broadcasting event {event_id}");
    random_node.p2p.broadcast(&EventPut(event, vec![])).await;
    info!("Waiting 5s for event propagation");
    sleep(5).await;

    // ====================================================
    // 3. Assert that everyone has the new event in the DAG
    // ====================================================
    assert_dags(&eg_instances, 2, &mut rng).await;

    // ==============================================================
    // 4. Create multiple events on a node and broadcast the last one
    //    The `EventPut` logic should manage to fetch all of them,
    //    provided that the last one references the earlier ones.
    // ==============================================================
    let random_node = eg_instances.choose(&mut rng).unwrap();
    let event0 = Event::new(vec![1, 2, 3, 4, 0], random_node).await;
    random_node.header_dag_insert(vec![event0.header.clone()], &dag_name).await.unwrap();
    let event0_id = random_node.dag_insert(slice::from_ref(&event0), &dag_name).await.unwrap()[0];
    let event1 = Event::new(vec![1, 2, 3, 4, 1], random_node).await;
    random_node.header_dag_insert(vec![event1.header.clone()], &dag_name).await.unwrap();
    let event1_id = random_node.dag_insert(slice::from_ref(&event1), &dag_name).await.unwrap()[0];
    let event2 = Event::new(vec![1, 2, 3, 4, 2], random_node).await;
    random_node.header_dag_insert(vec![event2.header.clone()], &dag_name).await.unwrap();
    let event2_id = random_node.dag_insert(slice::from_ref(&event2), &dag_name).await.unwrap()[0];
    // Genesis event + event from 2. + upper 3 events (layer 4)
    let current_genesis = random_node.current_genesis.read().await;
    let dag_name = current_genesis.header.timestamp.to_string();
    assert_eq!(random_node.dag_store.read().await.get_dag(&dag_name).len(), 5);
    let random_node_genesis = random_node.current_genesis.read().await.header.timestamp;
    let store = random_node.dag_store.read().await;
    let (_, tips_layers) = store.header_dags.get(&random_node_genesis).unwrap();
    assert_eq!(tips_layers.len(), 1);
    assert!(tips_layers.get(&4).unwrap().get(&event2_id).is_some());
    drop(current_genesis);
    drop(store);

    let event_chain = vec![
        (event0_id, event0.header.parents),
        (event1_id, event1.header.parents),
        (event2_id, event2.header.parents),
    ];

    info!("Broadcasting event {event2_id}");
    info!("Event chain: {event_chain:#?}");
    random_node.p2p.broadcast(&EventPut(event2, vec![])).await;
    info!("Waiting 5s for event propagation");
    sleep(5).await;

    // ==========================================
    // 5. Assert that everyone has all the events
    // ==========================================
    assert_dags(&eg_instances, 5, &mut rng).await;

    // ===========================================
    // 6. Create multiple events on multiple nodes
    // ===========================================
    // node 1
    // =======
    let node1 = eg_instances.choose(&mut rng).unwrap();
    let event0_1 = Event::new(vec![1, 2, 3, 4, 3], node1).await;
    node1.header_dag_insert(vec![event0_1.header.clone()], &dag_name).await.unwrap();
    node1.dag_insert(slice::from_ref(&event0_1), &dag_name).await.unwrap();
    node1.p2p.broadcast(&EventPut(event0_1, vec![])).await;
    msleep(300).await;

    let event1_1 = Event::new(vec![1, 2, 3, 4, 4], node1).await;
    node1.header_dag_insert(vec![event1_1.header.clone()], &dag_name).await.unwrap();
    node1.dag_insert(slice::from_ref(&event1_1), &dag_name).await.unwrap();
    node1.p2p.broadcast(&EventPut(event1_1, vec![])).await;
    msleep(300).await;

    let event2_1 = Event::new(vec![1, 2, 3, 4, 5], node1).await;
    node1.header_dag_insert(vec![event2_1.header.clone()], &dag_name).await.unwrap();
    node1.dag_insert(slice::from_ref(&event2_1), &dag_name).await.unwrap();
    node1.p2p.broadcast(&EventPut(event2_1, vec![])).await;
    msleep(300).await;

    // =======
    // node 2
    // =======
    let node2 = eg_instances.choose(&mut rng).unwrap();
    let event0_2 = Event::new(vec![1, 2, 3, 4, 6], node2).await;
    node2.header_dag_insert(vec![event0_2.header.clone()], &dag_name).await.unwrap();
    node2.dag_insert(slice::from_ref(&event0_2), &dag_name).await.unwrap();
    node2.p2p.broadcast(&EventPut(event0_2, vec![])).await;
    msleep(300).await;

    let event1_2 = Event::new(vec![1, 2, 3, 4, 7], node2).await;
    node2.header_dag_insert(vec![event1_2.header.clone()], &dag_name).await.unwrap();
    node2.dag_insert(slice::from_ref(&event1_2), &dag_name).await.unwrap();
    node2.p2p.broadcast(&EventPut(event1_2, vec![])).await;
    msleep(300).await;

    let event2_2 = Event::new(vec![1, 2, 3, 4, 8], node2).await;
    node2.header_dag_insert(vec![event2_2.header.clone()], &dag_name).await.unwrap();
    node2.dag_insert(slice::from_ref(&event2_2), &dag_name).await.unwrap();
    node2.p2p.broadcast(&EventPut(event2_2, vec![])).await;
    msleep(300).await;

    // =======
    // node 3
    // =======
    let node3 = eg_instances.choose(&mut rng).unwrap();
    let event0_3 = Event::new(vec![1, 2, 3, 4, 9], node3).await;
    node3.header_dag_insert(vec![event0_3.header.clone()], &dag_name).await.unwrap();
    node3.dag_insert(slice::from_ref(&event0_3), &dag_name).await.unwrap();
    node3.p2p.broadcast(&EventPut(event0_3, vec![])).await;
    msleep(300).await;

    let event1_3 = Event::new(vec![1, 2, 3, 4, 10], node3).await;
    node3.header_dag_insert(vec![event1_3.header.clone()], &dag_name).await.unwrap();
    node3.dag_insert(slice::from_ref(&event1_3), &dag_name).await.unwrap();
    node3.p2p.broadcast(&EventPut(event1_3, vec![])).await;
    msleep(300).await;

    let event2_3 = Event::new(vec![1, 2, 3, 4, 11], node3).await;
    node3.header_dag_insert(vec![event2_3.header.clone()], &dag_name).await.unwrap();
    node3.dag_insert(slice::from_ref(&event2_3), &dag_name).await.unwrap();
    node3.p2p.broadcast(&EventPut(event2_3, vec![])).await;
    msleep(300).await;

    // /////
    // //
    // let node4 = eg_instances.choose(&mut rng).unwrap();
    // let event0_4 = Event::new(vec![1, 2, 3, 4, 12], node4).await;
    // node4.dag_insert(&[event0_4.clone()]).await.unwrap();
    // node4.p2p.broadcast(&EventPut(event0_4)).await;
    // sleep(1).await;

    // let event1_4 = Event::new(vec![1, 2, 3, 4, 13], node4).await;
    // node4.dag_insert(&[event1_4.clone()]).await.unwrap();
    // node4.p2p.broadcast(&EventPut(event1_4)).await;
    // sleep(1).await;

    // let event2_4 = Event::new(vec![1, 2, 3, 4, 14], node4).await;
    // node4.dag_insert(&[event2_4.clone()]).await.unwrap();
    // node4.p2p.broadcast(&EventPut(event2_4)).await;
    // // sleep(1).await;

    // ==========================================
    // 7. Assert that everyone has all the events
    // ==========================================
    // 5 events from 2. and 4. + 9 events from 6. = 14
    assert_dags(&eg_instances, 14, &mut rng).await;

    // ============================================================
    // 8. Start a new node and try to sync the DAG from other peers
    // ============================================================
    {
        // Connect to N_CONNS random peers.
        let peer_indexes_to_connect: Vec<_> =
            peer_indexes.choose_multiple(&mut rng, N_CONNS).collect();

        let mut peers = vec![];
        for peer_index in peer_indexes_to_connect {
            let port = 13200 + peer_index;
            peers.push(Url::parse(&format!("tcp://127.0.0.1:{port}")).unwrap());
        }

        let event_graph = spawn_node(
            vec![Url::parse(&format!("tcp://127.0.0.1:{}", 13200 + N_NODES + 1)).unwrap()],
            peers,
            ex.clone(),
        )
        .await;

        eg_instances.push(event_graph.clone());

        event_graph.p2p.clone().start().await.unwrap();

        info!("Waiting 5s for new node connection");
        sleep(5).await;

        event_graph.sync_selected(1, false).await.unwrap();
    }

    // ============================================================
    // 9. Assert the new synced DAG has the same contents as others
    // ============================================================
    // 5 events from 2. and 4. + 9 events from 6. = 14
    assert_dags(&eg_instances, 14, &mut rng).await;

    // Stop the P2P network
    for eg in eg_instances.iter() {
        eg.p2p.clone().stop().await;
    }
}

#[test]
#[ignore]
fn eventgraph_chaotic_propagation() {
    test_body!(eventgraph_chaotic_propagation_real);
}

async fn eventgraph_chaotic_propagation_real(ex: Arc<Executor<'static>>) {
    let mut rng = rand::thread_rng();
    let peer_indexes: Vec<usize> = (0..N_NODES).collect();
    let n_events: usize = 100000;

    // Bootstrap nodes
    let mut eg_instances = bootstrap_nodes(&peer_indexes, 14200, &mut rng, ex.clone()).await;

    // =========================================
    // 1. Assert that everyone's DAG is the same
    // =========================================
    assert_dags(&eg_instances, 1, &mut rng).await;

    // ===========================================
    // 2. Create multiple events on multiple nodes
    for i in 0..n_events {
        let random_node = eg_instances.choose(&mut rng).unwrap();
        let event = Event::new(i.to_be_bytes().to_vec(), random_node).await;
        let current_genesis = random_node.current_genesis.read().await;
        let dag_name = current_genesis.header.timestamp.to_string();
        random_node.header_dag_insert(vec![event.header.clone()], &dag_name).await.unwrap();
        random_node.dag_insert(slice::from_ref(&event), &dag_name).await.unwrap();
        random_node.p2p.broadcast(&EventPut(event, vec![])).await;
    }
    info!("Waiting 5s for events propagation");
    sleep(5).await;

    // ==========================================
    // 3. Assert that everyone has all the events
    // ==========================================
    assert_dags(&eg_instances, n_events + 1, &mut rng).await;

    // ============================================================
    // 4. Start a new node and try to sync the DAG from other peers
    // ============================================================
    {
        // Connect to N_CONNS random peers.
        let peer_indexes_to_connect: Vec<_> =
            peer_indexes.choose_multiple(&mut rng, N_CONNS).collect();

        let mut peers = vec![];
        for peer_index in peer_indexes_to_connect {
            let port = 14200 + peer_index;
            peers.push(Url::parse(&format!("tcp://127.0.0.1:{port}")).unwrap());
        }

        let event_graph = spawn_node(
            vec![Url::parse(&format!("tcp://127.0.0.1:{}", 14200 + N_NODES + 1)).unwrap()],
            peers,
            ex.clone(),
        )
        .await;

        eg_instances.push(event_graph.clone());

        event_graph.p2p.clone().start().await.unwrap();

        info!("Waiting 5s for new node connection");
        sleep(5).await;

        event_graph.sync_selected(2, false).await.unwrap()
    }

    // ============================================================
    // 5. Assert the new synced DAG has the same contents as others
    // ============================================================
    assert_dags(&eg_instances, n_events + 1, &mut rng).await;

    // Stop the P2P network
    for eg in eg_instances.iter() {
        eg.p2p.clone().stop().await;
    }
}

// DAGStore tests
async fn make_dag_store() -> Result<DAGStore> {
    let sled_db = sled::Config::new().temporary(true).open()?;
    let hours_rotation = 1;

    let dag_store = DAGStore {
        db: sled_db.clone(),
        header_dags: BTreeMap::default(),
        main_dags: BTreeMap::default(),
    }
    .new(sled_db.clone(), hours_rotation)
    .await;

    Ok(dag_store)
}
#[test]
fn header_dags_and_main_dags_length_equals_dags_max_number() -> Result<()> {
    smol::block_on(async {
        let dag_store = make_dag_store().await?;
        assert_eq!(dag_store.header_dags.len() as i8, DAGS_MAX_NUMBER);
        assert_eq!(dag_store.main_dags.len() as i8, DAGS_MAX_NUMBER);

        Ok(())
    })
}

#[test]
fn all_dag_trees_are_created_on_sled_after_dag_store_creation() -> Result<()> {
    smol::block_on(async {
        let dag_store = make_dag_store().await?;
        let dag_trees: Vec<String> =
            dag_store.db.tree_names().iter().map(|n| String::from_utf8_lossy(n).into()).collect();

        // Should have 2 * DAGS_MAX_NUMBER trees + 1 (the default tree)
        assert_eq!(dag_trees.len() as i8, DAGS_MAX_NUMBER * 2 + 1);

        for (dag_timestamp, _) in dag_store.header_dags {
            assert!(dag_trees.contains(&format!("headers_{dag_timestamp}")));
        }

        for (dag_timestamp, _) in dag_store.main_dags {
            assert!(dag_trees.contains(&dag_timestamp.to_string()));
        }

        Ok(())
    })
}

#[test]
fn genesis_events_or_headers_are_added_to_all_trees_and_utips() -> Result<()> {
    smol::block_on(async {
        let dag_store = make_dag_store().await?;

        for (_, (tree, layer_utips)) in dag_store.header_dags {
            let genesis_header = tree.first()?;
            // A Genesis Header is found in sled tree
            assert!(genesis_header.is_some());
            let (genesis_hash, genesis_header) = genesis_header.unwrap();
            let genesis_header: Header = deserialize_async(&genesis_header).await?;
            let genesis_hash: blake3::Hash = deserialize_async(&genesis_hash).await?;
            assert_eq!(genesis_header.layer, 0);
            assert!(genesis_header.parents.iter().all(|p| *p == NULL_ID));
            // The Genesis Header hash is stored as Unreferenced tip
            assert!(layer_utips.contains_key(&0));
            assert!(layer_utips.get(&0).unwrap().contains(&genesis_hash));
        }

        for (_, (tree, layer_utips)) in dag_store.main_dags {
            let genesis_event = tree.first()?;
            // A Genesis Event is found in sled tree
            assert!(genesis_event.is_some());
            let (genesis_hash, genesis_event) = genesis_event.unwrap();
            let genesis_event: Event = deserialize_async(&genesis_event).await?;
            let genesis_hash: blake3::Hash = deserialize_async(&genesis_hash).await?;
            assert_eq!(genesis_event.header.layer, 0);
            assert!(genesis_event.header.parents.iter().all(|p| *p == NULL_ID));
            assert_eq!(genesis_event.content, GENESIS_CONTENTS);
            // The Genesis Header hash is stored as Unreferenced tip
            assert!(layer_utips.contains_key(&0));
            assert!(layer_utips.get(&0).unwrap().contains(&genesis_hash));
        }

        Ok(())
    })
}

#[test]
fn adding_new_dag_removes_oldest_dag_tree() -> Result<()> {
    smol::block_on(async {
        let mut dag_store = make_dag_store().await?;
        let oldest_dag_timestamp = dag_store.main_dags.first_key_value().unwrap().0.to_owned();
        // Next dag to add
        let next_rotation = next_rotation_timestamp(INITIAL_GENESIS, 1);
        let header =
            Header { timestamp: next_rotation, parents: [NULL_ID; N_EVENT_PARENTS], layer: 0 };
        let next_genesis = Event { header, content: GENESIS_CONTENTS.to_vec() };

        dag_store.add_dag(&next_genesis.header.timestamp.to_string(), &next_genesis).await;

        // The length of the dags should stay the same after adding
        assert_eq!(dag_store.main_dags.len() as i8, DAGS_MAX_NUMBER);
        assert_eq!(dag_store.header_dags.len() as i8, DAGS_MAX_NUMBER);
        // We should have an entry with the new dag timestamp
        assert!(dag_store.main_dags.contains_key(&next_rotation));
        assert!(dag_store.header_dags.contains_key(&next_rotation));
        // The oldest dag entry should have been removed
        assert!(!dag_store.main_dags.contains_key(&oldest_dag_timestamp));
        assert!(!dag_store.header_dags.contains_key(&oldest_dag_timestamp));

        let dag_trees: Vec<String> =
            dag_store.db.tree_names().iter().map(|n| String::from_utf8_lossy(n).into()).collect();

        // The number of dag trees should stay the same after adding
        assert_eq!(dag_trees.len() as i8, 2 * DAGS_MAX_NUMBER + 1);
        // We should have a tree with the new dag timestamp value
        assert!(dag_trees.contains(&next_rotation.to_string()));
        assert!(dag_trees.contains(&format!("headers_{next_rotation}")));
        // The oldest dag sled tree should have been removed
        assert!(!dag_trees.contains(&oldest_dag_timestamp.to_string()));
        assert!(!dag_trees.contains(&format!("headers_{oldest_dag_timestamp}")));

        Ok(())
    })
}

#[test]
fn sort_moves_current_dag_to_front() -> Result<()> {
    smol::block_on(async {
        let dag_store = make_dag_store().await?;

        let trees = dag_store.sort_dags().await;
        let first_tree_name: String =
            String::from_utf8_lossy(&trees.first().unwrap().name()).into();
        assert_eq!(
            first_tree_name,
            dag_store.main_dags.last_key_value().unwrap().0.to_owned().to_string()
        );

        Ok(())
    })
}

#[test]
fn unreferenced_tips_are_found() -> Result<()> {
    smol::block_on(async {
        let dag_store = make_dag_store().await?;

        let current_dag_tree = dag_store.main_dags.last_key_value().unwrap().1 .0.clone();
        let current_dag_genesis_hash = *dag_store
            .main_dags
            .last_key_value()
            .unwrap()
            .1
             .1
            .get(&0)
            .unwrap()
            .iter()
            .next()
            .unwrap();

        let mut parents = [NULL_ID; N_EVENT_PARENTS];
        parents[0] = current_dag_genesis_hash;
        let event2 = Event {
            header: Header {
                timestamp: UNIX_EPOCH.elapsed().unwrap().as_millis() as u64,
                parents,
                layer: 1,
            },
            content: "event2".as_bytes().to_vec(),
        };
        let event2_hash = event2.id();
        current_dag_tree.insert(event2_hash.as_bytes(), serialize_async(&event2).await)?;

        let mut parents = [NULL_ID; N_EVENT_PARENTS];
        parents[0] = event2_hash;
        let event3 = Event {
            header: Header {
                timestamp: UNIX_EPOCH.elapsed().unwrap().as_millis() as u64,
                parents,
                layer: 2,
            },
            content: "event3".as_bytes().to_vec(),
        };
        let event3_hash = event3.id();
        current_dag_tree.insert(event3_hash.as_bytes(), serialize_async(&event3).await)?;

        let mut parents = [NULL_ID; N_EVENT_PARENTS];
        parents[0] = current_dag_genesis_hash;
        let event4 = Event {
            header: Header {
                timestamp: UNIX_EPOCH.elapsed().unwrap().as_millis() as u64,
                parents,
                layer: 2,
            },
            content: "event4".as_bytes().to_vec(),
        };
        let event4_hash = event4.id();
        current_dag_tree.insert(event4_hash.as_bytes(), serialize_async(&event4).await)?;

        let layer_utips = dag_store.find_unreferenced_tips(&current_dag_tree).await;
        // We have unreferenced tips only on the 2nd layer
        assert_eq!(layer_utips.len(), 1);
        // We have two unreferenced tips
        let tip_hashes = layer_utips.get(&2).unwrap();
        assert_eq!(tip_hashes.len(), 2);
        // Event3 and Event4 are the only unreferenced tips
        assert!(tip_hashes.contains(&event3_hash));
        assert!(tip_hashes.contains(&event4_hash));

        Ok(())
    })
}

// EventGraph tests
async fn make_event_graph() -> Result<EventGraphPtr> {
    let ex = Arc::new(Executor::new());
    let p2p = P2p::new(Settings::default(), ex.clone()).await?;
    let sled_db = sled::Config::new().temporary(true).open()?;
    EventGraph::new(p2p, sled_db, "/tmp".into(), false, false, 1, ex).await
}

#[test]
fn dag_insert_on_invalid_dag_name() -> Result<()> {
    smol::block_on(async {
        let event_graph = make_event_graph().await?;

        let new_event = Event::new("new_event".as_bytes().to_vec(), &event_graph).await;
        // Using a dag name that is not a u64 timestamp gives an error
        let res = event_graph.dag_insert(&[new_event], "non_timestamp_dag_name").await;
        assert!(res.is_err());
        let err = res.unwrap_err();
        match err {
            Error::ParseIntError(_) => {}
            _ => panic!("expected parse error"),
        }

        Ok(())
    })
}

#[test]
fn invalid_header_dag_insert() -> Result<()> {
    smol::block_on(async {
        let event_graph = make_event_graph().await?;
        let dag_name = event_graph
            .dag_store
            .read()
            .await
            .main_dags
            .last_key_value()
            .unwrap()
            .0
            .clone()
            .to_string();

        let new_event = Event::new("new_event".as_bytes().to_vec(), &event_graph).await;
        // Inserting an invalid event gives an error
        let mut event_timestamp_too_old = new_event.clone();
        event_timestamp_too_old.header.timestamp = 1000;

        let res =
            event_graph.header_dag_insert(vec![event_timestamp_too_old.header], &dag_name).await;
        assert!(res.is_err());

        let err = res.unwrap_err();
        match err {
            Error::HeaderIsInvalid => {}
            _ => panic!("expected invalid header error"),
        }

        Ok(())
    })
}

#[test]
fn dag_insert_without_inserting_header() -> Result<()> {
    smol::block_on(async {
        let event_graph = make_event_graph().await?;
        let dag_name = event_graph
            .dag_store
            .read()
            .await
            .main_dags
            .last_key_value()
            .unwrap()
            .0
            .clone()
            .to_string();

        let new_event = Event::new("new_event".as_bytes().to_vec(), &event_graph).await;
        let res = event_graph.dag_insert(slice::from_ref(&new_event), &dag_name).await;
        // Inserting event without inserting its header first gets skipped
        assert!(res.is_ok() && res.unwrap().is_empty());
        Ok(())
    })
}

#[test]
fn dag_insert_duplicate_event() -> Result<()> {
    smol::block_on(async {
        let event_graph = make_event_graph().await?;
        let dag_name = event_graph
            .dag_store
            .read()
            .await
            .main_dags
            .last_key_value()
            .unwrap()
            .0
            .clone()
            .to_string();

        let new_event = Event::new("new_event".as_bytes().to_vec(), &event_graph).await;
        event_graph.header_dag_insert(vec![new_event.header.clone()], &dag_name).await?;
        let res = event_graph.dag_insert(slice::from_ref(&new_event), &dag_name).await;
        // Proper insertion
        assert!(res.is_ok() && res.unwrap().len() == 1);
        // Inserting duplicate event gets skipped
        let res = event_graph.dag_insert(&[new_event], &dag_name).await;
        assert!(res.is_ok() && res.unwrap().is_empty());

        Ok(())
    })
}

#[test]
fn dag_insert_valid_event() -> Result<()> {
    smol::block_on(async {
        let event_graph = make_event_graph().await?;
        let dag_name = *event_graph.dag_store.read().await.main_dags.last_key_value().unwrap().0;
        let new_event_sub = event_graph.event_pub.clone().subscribe().await;

        let new_event = Event::new("new_event".as_bytes().to_vec(), &event_graph).await;
        event_graph
            .header_dag_insert(vec![new_event.header.clone()], &dag_name.to_string())
            .await?;
        let res = event_graph.dag_insert(slice::from_ref(&new_event), &dag_name.to_string()).await;
        assert!(res.is_ok() && res.unwrap().len() == 1);
        // Unreferenced tips is updated
        let layer_utips =
            event_graph.dag_store.read().await.main_dags.get(&dag_name).unwrap().1.clone();
        assert!(layer_utips.get(&1).unwrap().contains(&new_event.id()));
        // The new event notification is sent to subscriber
        let dur = Duration::from_secs(1);
        let Ok(res) = timeout(dur, new_event_sub.receive()).await else {
            panic!("Event is not sent to subscriber")
        };
        assert_eq!(res.id(), new_event.id());
        Ok(())
    })
}

/*
   This function builds the following graph

   Layer    3           2                    1                    0
        [Event3A]-----[Event2A]-------|
                                      |-----[Event1A]-----|
                                               |          |
                                      ---------|          |
        [Event3B]-----[Event2B]-------|                   |
                                      |                   |
                                      |-----[Event1B]-----|-----[GENESIS]
                                                          |
        [Event3C]-----[Event2C]----|                      |
                                   |  |-----[Event1C]-----|
                                   ---|                   |
                                   |  |                   |
        [Event3D]-----[Event2D]----|  |------[Event1D]----|
*/
async fn build_graph() -> Result<(EventGraphPtr, Vec<Event>)> {
    let event_graph = make_event_graph().await?;
    let mut events = vec![];
    let dag_name = event_graph
        .dag_store
        .read()
        .await
        .main_dags
        .last_key_value()
        .unwrap()
        .0
        .clone()
        .to_string();

    let current_dag_genesis_hash = event_graph.current_genesis.read().await.id();

    // first layer
    let mut parents = [NULL_ID; N_EVENT_PARENTS];
    parents[0] = current_dag_genesis_hash;
    let event1a = Event {
        header: Header {
            timestamp: UNIX_EPOCH.elapsed().unwrap().as_millis() as u64 + 1,
            layer: 1,
            parents,
        },
        content: "Event1A".as_bytes().to_vec(),
    };
    events.push(event1a.clone());

    let event1b = Event {
        header: Header {
            timestamp: UNIX_EPOCH.elapsed().unwrap().as_millis() as u64 + 2,
            layer: 1,
            parents,
        },
        content: "Event1B".as_bytes().to_vec(),
    };
    events.push(event1b.clone());

    let event1c = Event {
        header: Header {
            timestamp: UNIX_EPOCH.elapsed().unwrap().as_millis() as u64 + 3,
            layer: 1,
            parents,
        },
        content: "Event1C".as_bytes().to_vec(),
    };
    events.push(event1c.clone());

    let event1d = Event {
        header: Header {
            timestamp: UNIX_EPOCH.elapsed().unwrap().as_millis() as u64 + 4,
            layer: 1,
            parents,
        },
        content: "Event1D".as_bytes().to_vec(),
    };
    events.push(event1d.clone());

    // second layer
    parents[0] = event1a.id();
    let event2a = Event {
        header: Header {
            timestamp: UNIX_EPOCH.elapsed().unwrap().as_millis() as u64 + 5,
            layer: 2,
            parents,
        },
        content: "Event2A".as_bytes().to_vec(),
    };
    events.push(event2a.clone());

    parents[1] = event1b.id();
    let event2b = Event {
        header: Header {
            timestamp: UNIX_EPOCH.elapsed().unwrap().as_millis() as u64 + 6,
            layer: 2,
            parents,
        },
        content: "Event2B".as_bytes().to_vec(),
    };
    events.push(event2b.clone());

    parents[0] = event1c.id();
    parents[1] = event1d.id();
    let event2c = Event {
        header: Header {
            timestamp: UNIX_EPOCH.elapsed().unwrap().as_millis() as u64 + 7,
            layer: 2,
            parents,
        },
        content: "Event2C".as_bytes().to_vec(),
    };
    events.push(event2c.clone());

    let event2d = Event {
        header: Header {
            timestamp: UNIX_EPOCH.elapsed().unwrap().as_millis() as u64 + 8,
            layer: 2,
            parents,
        },
        content: "Event2D".as_bytes().to_vec(),
    };
    events.push(event2d.clone());

    // third layer
    let mut parents = [NULL_ID; N_EVENT_PARENTS];
    parents[0] = event2a.id();
    let event3a = Event {
        header: Header {
            timestamp: UNIX_EPOCH.elapsed().unwrap().as_millis() as u64 + 9,
            layer: 3,
            parents,
        },
        content: "Event3A".as_bytes().to_vec(),
    };
    events.push(event3a.clone());

    parents[0] = event2b.id();
    let event3b = Event {
        header: Header {
            timestamp: UNIX_EPOCH.elapsed().unwrap().as_millis() as u64 + 10,
            layer: 3,
            parents,
        },
        content: "Event3B".as_bytes().to_vec(),
    };
    events.push(event3b.clone());

    parents[0] = event2c.id();
    let event3c = Event {
        header: Header {
            timestamp: UNIX_EPOCH.elapsed().unwrap().as_millis() as u64 + 11,
            layer: 3,
            parents,
        },
        content: "Event3C".as_bytes().to_vec(),
    };
    events.push(event3c.clone());

    parents[0] = event2d.id();
    let event3d = Event {
        header: Header {
            timestamp: UNIX_EPOCH.elapsed().unwrap().as_millis() as u64 + 12,
            layer: 3,
            parents,
        },
        content: "Event3D".as_bytes().to_vec(),
    };
    events.push(event3d.clone());

    // Insert events 1a to 1d
    event_graph
        .header_dag_insert(
            vec![
                event1a.header.clone(),
                event1b.header.clone(),
                event1c.header.clone(),
                event1d.header.clone(),
            ],
            &dag_name,
        )
        .await?;
    event_graph.dag_insert(&[event1a, event1b, event1c, event1d], &dag_name).await?;
    // Insert events 2a to 2d
    event_graph
        .header_dag_insert(
            vec![
                event2a.header.clone(),
                event2b.header.clone(),
                event2c.header.clone(),
                event2d.header.clone(),
            ],
            &dag_name,
        )
        .await?;
    event_graph.dag_insert(&[event2a, event2b, event2c, event2d], &dag_name).await?;
    // Insert events 3a to 3d
    event_graph
        .header_dag_insert(
            vec![
                event3a.header.clone(),
                event3b.header.clone(),
                event3c.header.clone(),
                event3d.header.clone(),
            ],
            &dag_name,
        )
        .await?;
    event_graph.dag_insert(&[event3a, event3b, event3c, event3d], &dag_name).await?;

    //panic!("REACHED HERE");

    Ok((event_graph, events))
}

#[test]
fn find_ancestors_of_an_event() -> Result<()> {
    smol::block_on(async {
        let (event_graph, events) = build_graph().await?;

        let dag_name = event_graph
            .dag_store
            .read()
            .await
            .main_dags
            .last_key_value()
            .unwrap()
            .0
            .clone()
            .to_string();

        let tree = event_graph.dag_store.read().await.get_dag(&format!("headers_{dag_name}"));

        let events_map: HashMap<String, Event> =
            events.into_iter().map(|e| (String::from_utf8_lossy(&e.content).into(), e)).collect();
        let genesis_header = event_graph.current_genesis.read().await.header.clone();
        let genesis_hash = genesis_header.id();
        // Genesis layer
        let mut genesis_ancestors = HashSet::new();
        event_graph.get_ancestors(&mut genesis_ancestors, genesis_header, &tree).await?;
        assert!(genesis_ancestors.is_empty());

        // 1st layer
        let mut event1a_ancestors = HashSet::new();
        event_graph
            .get_ancestors(
                &mut event1a_ancestors,
                events_map.get("Event1A").unwrap().header.clone(),
                &tree,
            )
            .await?;
        let mut event1b_ancestors = HashSet::new();
        event_graph
            .get_ancestors(
                &mut event1b_ancestors,
                events_map.get("Event1B").unwrap().header.clone(),
                &tree,
            )
            .await?;
        let mut event1c_ancestors = HashSet::new();
        event_graph
            .get_ancestors(
                &mut event1c_ancestors,
                events_map.get("Event1C").unwrap().header.clone(),
                &tree,
            )
            .await?;
        let mut event1d_ancestors = HashSet::new();
        event_graph
            .get_ancestors(
                &mut event1d_ancestors,
                events_map.get("Event1D").unwrap().header.clone(),
                &tree,
            )
            .await?;

        // Only genesis is the ancestor
        assert!(event1a_ancestors.len() == 1 && event1a_ancestors.contains(&genesis_hash));
        assert!(event1b_ancestors.len() == 1 && event1b_ancestors.contains(&genesis_hash));
        assert!(event1c_ancestors.len() == 1 && event1c_ancestors.contains(&genesis_hash));
        assert!(event1d_ancestors.len() == 1 && event1d_ancestors.contains(&genesis_hash));

        // 2nd layer
        let event2a_expected_ancestors =
            HashSet::from([genesis_hash, events_map.get("Event1A").unwrap().id()]);
        let event2b_expected_ancestors = HashSet::from([
            genesis_hash,
            events_map.get("Event1B").unwrap().id(),
            events_map.get("Event1A").unwrap().id(),
        ]);
        let event2cd_expected_ancestors = HashSet::from([
            genesis_hash,
            events_map.get("Event1C").unwrap().id(),
            events_map.get("Event1D").unwrap().id(),
        ]);

        let mut event2a_ancestors = HashSet::new();
        event_graph
            .get_ancestors(
                &mut event2a_ancestors,
                events_map.get("Event2A").unwrap().header.clone(),
                &tree,
            )
            .await?;
        let mut event2b_ancestors = HashSet::new();
        event_graph
            .get_ancestors(
                &mut event2b_ancestors,
                events_map.get("Event2B").unwrap().header.clone(),
                &tree,
            )
            .await?;
        let mut event2c_ancestors = HashSet::new();
        event_graph
            .get_ancestors(
                &mut event2c_ancestors,
                events_map.get("Event2C").unwrap().header.clone(),
                &tree,
            )
            .await?;
        let mut event2d_ancestors = HashSet::new();
        event_graph
            .get_ancestors(
                &mut event2d_ancestors,
                events_map.get("Event2D").unwrap().header.clone(),
                &tree,
            )
            .await?;

        assert_eq!(event2a_ancestors, event2a_expected_ancestors);
        assert_eq!(event2b_ancestors, event2b_expected_ancestors);
        assert_eq!(event2c_ancestors, event2cd_expected_ancestors);
        assert_eq!(event2d_ancestors, event2cd_expected_ancestors);

        // 3rd layer
        let mut event3a_expected_ancestors = event2a_expected_ancestors.clone();
        event3a_expected_ancestors.insert(events_map.get("Event2A").unwrap().header.clone().id());
        let mut event3b_expected_ancestors = event2b_expected_ancestors.clone();
        event3b_expected_ancestors.insert(events_map.get("Event2B").unwrap().header.clone().id());
        let mut event3c_expected_ancestors = event2cd_expected_ancestors.clone();
        event3c_expected_ancestors.insert(events_map.get("Event2C").unwrap().header.clone().id());
        let mut event3d_expected_ancestors = event2cd_expected_ancestors.clone();
        event3d_expected_ancestors.insert(events_map.get("Event2D").unwrap().header.clone().id());

        let mut event3a_ancestors = HashSet::new();
        event_graph
            .get_ancestors(
                &mut event3a_ancestors,
                events_map.get("Event3A").unwrap().header.clone(),
                &tree,
            )
            .await?;
        let mut event3b_ancestors = HashSet::new();
        event_graph
            .get_ancestors(
                &mut event3b_ancestors,
                events_map.get("Event3B").unwrap().header.clone(),
                &tree,
            )
            .await?;
        let mut event3c_ancestors = HashSet::new();
        event_graph
            .get_ancestors(
                &mut event3c_ancestors,
                events_map.get("Event3C").unwrap().header.clone(),
                &tree,
            )
            .await?;
        let mut event3d_ancestors = HashSet::new();
        event_graph
            .get_ancestors(
                &mut event3d_ancestors,
                events_map.get("Event3D").unwrap().header.clone(),
                &tree,
            )
            .await?;

        assert_eq!(event3a_ancestors, event3a_expected_ancestors);
        assert_eq!(event3b_ancestors, event3b_expected_ancestors);
        assert_eq!(event3c_ancestors, event3c_expected_ancestors);
        assert_eq!(event3d_ancestors, event3d_expected_ancestors);

        Ok(())
    })
}

#[test]
fn fetches_headers_with_tips() -> Result<()> {
    smol::block_on(async {
        let (event_graph, events) = build_graph().await?;

        let dag_name = event_graph
            .dag_store
            .read()
            .await
            .main_dags
            .last_key_value()
            .unwrap()
            .0
            .clone()
            .to_string();

        let map: HashMap<blake3::Hash, String> = events
            .into_iter()
            .map(|e| (e.id(), String::from_utf8_lossy(&e.content).into()))
            .collect();
        let name_map: HashMap<String, blake3::Hash> =
            map.iter().map(|(hash, content)| (content.clone(), *hash)).collect();

        let genesis_hash = event_graph.current_genesis.read().await.id();
        let patha = ["Event3A", "Event2A", "Event1A"];
        let pathb = ["Event3B", "Event2B", "Event1B", "Event1A"];
        let pathc = ["Event3C", "Event2C", "Event1C", "Event1D"];
        let pathd = ["Event3D", "Event2D", "Event1C", "Event1D"];

        let patha_tip = BTreeMap::from([(3, HashSet::from([*name_map.get("Event3A").unwrap()]))]);
        // Should be only headers that are not ancestors of Event3A
        let headers = event_graph.fetch_headers_with_tips(&dag_name, &patha_tip).await?;
        assert!(headers.iter().all(
            |h| h.id() != genesis_hash && !patha.contains(&map.get(&h.id()).unwrap().as_str())
        ));

        let pathb_tip = BTreeMap::from([(3, HashSet::from([*name_map.get("Event3B").unwrap()]))]);
        // Should be only headers that are not ancestors of Event3B
        let headers = event_graph.fetch_headers_with_tips(&dag_name, &pathb_tip).await?;
        assert!(headers.iter().all(
            |h| h.id() != genesis_hash && !pathb.contains(&map.get(&h.id()).unwrap().as_str())
        ));

        let pathc_tip = BTreeMap::from([(3, HashSet::from([*name_map.get("Event3C").unwrap()]))]);
        // Should be only headers that are not ancestors of Event3C
        let headers = event_graph.fetch_headers_with_tips(&dag_name, &pathc_tip).await?;
        assert!(headers.iter().all(
            |h| h.id() != genesis_hash && !pathc.contains(&map.get(&h.id()).unwrap().as_str())
        ));

        let pathd_tip = BTreeMap::from([(3, HashSet::from([*name_map.get("Event3D").unwrap()]))]);
        // Should be only headers that are not ancestors of Event3D
        let headers = event_graph.fetch_headers_with_tips(&dag_name, &pathd_tip).await?;
        assert!(headers.iter().all(
            |h| h.id() != genesis_hash && !pathd.contains(&map.get(&h.id()).unwrap().as_str())
        ));

        // Two tips Event3A and Event3D
        let mut comb_tip = BTreeMap::new();
        comb_tip.extend(patha_tip);
        comb_tip.get_mut(&3).unwrap().extend(pathd_tip.get(&3).unwrap());

        // Should be only headers that are not ancestors of Event3A and Event3D
        let headers = event_graph.fetch_headers_with_tips(&dag_name, &comb_tip).await?;
        assert!(headers.iter().all(|h| h.id() != genesis_hash &&
            !patha.contains(&map.get(&h.id()).unwrap().as_str()) &&
            !pathd.contains(&map.get(&h.id()).unwrap().as_str())));

        Ok(())
    })
}
