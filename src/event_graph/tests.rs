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

// cargo test --release --features=event-graph --lib eventgraph_propagation -- --include-ignored

use std::sync::Arc;

use log::{info, warn};
use rand::{prelude::SliceRandom, rngs::ThreadRng};
use sled_overlay::sled;
use smol::{channel, future, Executor};
use url::Url;

use crate::{
    event_graph::{
        proto::{EventPut, ProtocolEventGraph},
        Event, EventGraph,
    },
    net::{session::SESSION_DEFAULT, P2p, Settings},
    system::{msleep, sleep},
};

// Number of nodes to spawn and number of peers each node connects to
const N_NODES: usize = 5;
const N_CONNS: usize = 2;
//const N_NODES: usize = 50;
//const N_CONNS: usize = N_NODES / 3;

fn init_logger() {
    let mut cfg = simplelog::ConfigBuilder::new();
    cfg.add_filter_ignore("sled".to_string());
    cfg.add_filter_ignore("net::protocol_ping".to_string());
    cfg.add_filter_ignore("net::channel::subscribe_stop()".to_string());
    cfg.add_filter_ignore("net::hosts".to_string());
    cfg.add_filter_ignore("net::session".to_string());
    cfg.add_filter_ignore("net::message_subscriber".to_string());
    cfg.add_filter_ignore("net::protocol_address".to_string());
    cfg.add_filter_ignore("net::protocol_version".to_string());
    cfg.add_filter_ignore("net::protocol_registry".to_string());
    cfg.add_filter_ignore("net::channel::send()".to_string());
    cfg.add_filter_ignore("net::channel::start()".to_string());
    cfg.add_filter_ignore("net::channel::subscribe_msg()".to_string());
    cfg.add_filter_ignore("net::channel::main_receive_loop()".to_string());
    cfg.add_filter_ignore("net::tcp".to_string());

    // We check this error so we can execute same file tests in parallel,
    // otherwise second one fails to init logger here.
    if simplelog::TermLogger::init(
        simplelog::LevelFilter::Info,
        //simplelog::LevelFilter::Debug,
        //simplelog::LevelFilter::Trace,
        cfg.build(),
        simplelog::TerminalMode::Mixed,
        simplelog::ColorChoice::Auto,
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
    let settings = Settings {
        localnet: true,
        inbound_addrs,
        outbound_connections: 0,
        outbound_connect_timeout: 2,
        inbound_connections: usize::MAX,
        peers,
        allowed_transports: vec!["tcp".to_string()],
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
            peers.push(Url::parse(&format!("tcp://127.0.0.1:{}", port)).unwrap());
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
            "Node {}, expected {} events, have {}",
            i,
            expected_len,
            dag.len()
        );
        assert_eq!(
            node_last_layer_tips, last_layer_tips,
            "Node {} contains malformed unreferenced tips",
            i
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
    let event_id = random_node.dag_insert(&[event.clone()], &dag_name).await.unwrap()[0];

    let store = random_node.dag_store.read().await;
    let (_, tips_layers) = store.header_dags.get(&current_genesis.header.timestamp).unwrap();

    // Since genesis was referenced, its layer (0) have been removed
    assert_eq!(tips_layers.len(), 1);
    assert!(tips_layers.last_key_value().unwrap().1.get(&event_id).is_some());
    drop(store);
    drop(current_genesis);
    info!("Broadcasting event {}", event_id);
    random_node.p2p.broadcast(&EventPut(event)).await;
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
    let event0_id = random_node.dag_insert(&[event0.clone()], &dag_name).await.unwrap()[0];
    let event1 = Event::new(vec![1, 2, 3, 4, 1], random_node).await;
    random_node.header_dag_insert(vec![event1.header.clone()], &dag_name).await.unwrap();
    let event1_id = random_node.dag_insert(&[event1.clone()], &dag_name).await.unwrap()[0];
    let event2 = Event::new(vec![1, 2, 3, 4, 2], random_node).await;
    random_node.header_dag_insert(vec![event2.header.clone()], &dag_name).await.unwrap();
    let event2_id = random_node.dag_insert(&[event2.clone()], &dag_name).await.unwrap()[0];
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

    info!("Broadcasting event {}", event2_id);
    info!("Event chain: {:#?}", event_chain);
    random_node.p2p.broadcast(&EventPut(event2)).await;
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
    node1.dag_insert(&[event0_1.clone()], &dag_name).await.unwrap();
    node1.p2p.broadcast(&EventPut(event0_1)).await;
    msleep(300).await;

    let event1_1 = Event::new(vec![1, 2, 3, 4, 4], node1).await;
    node1.header_dag_insert(vec![event1_1.header.clone()], &dag_name).await.unwrap();
    node1.dag_insert(&[event1_1.clone()], &dag_name).await.unwrap();
    node1.p2p.broadcast(&EventPut(event1_1)).await;
    msleep(300).await;

    let event2_1 = Event::new(vec![1, 2, 3, 4, 5], node1).await;
    node1.header_dag_insert(vec![event2_1.header.clone()], &dag_name).await.unwrap();
    node1.dag_insert(&[event2_1.clone()], &dag_name).await.unwrap();
    node1.p2p.broadcast(&EventPut(event2_1)).await;
    msleep(300).await;

    // =======
    // node 2
    // =======
    let node2 = eg_instances.choose(&mut rng).unwrap();
    let event0_2 = Event::new(vec![1, 2, 3, 4, 6], node2).await;
    node2.header_dag_insert(vec![event0_2.header.clone()], &dag_name).await.unwrap();
    node2.dag_insert(&[event0_2.clone()], &dag_name).await.unwrap();
    node2.p2p.broadcast(&EventPut(event0_2)).await;
    msleep(300).await;

    let event1_2 = Event::new(vec![1, 2, 3, 4, 7], node2).await;
    node2.header_dag_insert(vec![event1_2.header.clone()], &dag_name).await.unwrap();
    node2.dag_insert(&[event1_2.clone()], &dag_name).await.unwrap();
    node2.p2p.broadcast(&EventPut(event1_2)).await;
    msleep(300).await;

    let event2_2 = Event::new(vec![1, 2, 3, 4, 8], node2).await;
    node2.header_dag_insert(vec![event2_2.header.clone()], &dag_name).await.unwrap();
    node2.dag_insert(&[event2_2.clone()], &dag_name).await.unwrap();
    node2.p2p.broadcast(&EventPut(event2_2)).await;
    msleep(300).await;

    // =======
    // node 3
    // =======
    let node3 = eg_instances.choose(&mut rng).unwrap();
    let event0_3 = Event::new(vec![1, 2, 3, 4, 9], node3).await;
    node3.header_dag_insert(vec![event0_3.header.clone()], &dag_name).await.unwrap();
    node3.dag_insert(&[event0_3.clone()], &dag_name).await.unwrap();
    node3.p2p.broadcast(&EventPut(event0_3)).await;
    msleep(300).await;

    let event1_3 = Event::new(vec![1, 2, 3, 4, 10], node3).await;
    node3.header_dag_insert(vec![event1_3.header.clone()], &dag_name).await.unwrap();
    node3.dag_insert(&[event1_3.clone()], &dag_name).await.unwrap();
    node3.p2p.broadcast(&EventPut(event1_3)).await;
    msleep(300).await;

    let event2_3 = Event::new(vec![1, 2, 3, 4, 11], node3).await;
    node3.header_dag_insert(vec![event2_3.header.clone()], &dag_name).await.unwrap();
    node3.dag_insert(&[event2_3.clone()], &dag_name).await.unwrap();
    node3.p2p.broadcast(&EventPut(event2_3)).await;
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
            peers.push(Url::parse(&format!("tcp://127.0.0.1:{}", port)).unwrap());
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
        random_node.dag_insert(&[event.clone()], &dag_name).await.unwrap();
        random_node.p2p.broadcast(&EventPut(event)).await;
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
            peers.push(Url::parse(&format!("tcp://127.0.0.1:{}", port)).unwrap());
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
