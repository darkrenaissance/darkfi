/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
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

// cargo +nightly test --release --features=event-graph --lib eventgraph_propagation -- --include-ignored

use std::sync::Arc;

use log::info;
use rand::{prelude::SliceRandom, Rng};
use smol::{channel, future, Executor};
use url::Url;

use crate::{
    event_graph2::{
        proto::{EventPut, ProtocolEventGraph},
        Event, EventGraph, NULL_ID,
    },
    net::{P2p, Settings, SESSION_ALL},
    system::sleep,
};

// Number of nodes to spawn and number of peers each node connects to
const N_NODES: usize = 5;
const N_CONNS: usize = 2;
//const N_NODES: usize = 50;
//const N_CONNS: usize = N_NODES / 3;

#[test]
#[ignore]
fn eventgraph_propagation() {
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

    simplelog::TermLogger::init(
        //simplelog::LevelFilter::Info,
        simplelog::LevelFilter::Debug,
        //simplelog::LevelFilter::Trace,
        cfg.build(),
        simplelog::TerminalMode::Mixed,
        simplelog::ColorChoice::Auto,
    )
    .unwrap();

    let ex = Arc::new(Executor::new());
    let ex_ = ex.clone();
    let (signal, shutdown) = channel::unbounded::<()>();

    // Run a thread for each node.
    easy_parallel::Parallel::new()
        .each(0..N_NODES, |_| future::block_on(ex.run(shutdown.recv())))
        .finish(|| {
            future::block_on(async {
                eventgraph_propagation_real(ex_).await;
                drop(signal);
            })
        });
}

async fn eventgraph_propagation_real(ex: Arc<Executor<'static>>) {
    let mut eg_instances = vec![];
    let mut rng = rand::thread_rng();

    let mut genesis_event_id = NULL_ID;

    // Initialize the nodes
    for i in 0..N_NODES {
        // Everyone will connect to N_CONNS random peers.
        let mut peers = vec![];
        for _ in 0..N_CONNS {
            let mut port = 13200 + i;
            while port == 13200 + i {
                port = 13200 + rng.gen_range(0..N_NODES);
            }
            peers.push(Url::parse(&format!("tcp://127.0.0.1:{}", port)).unwrap());
        }

        let settings = Settings {
            localnet: true,
            inbound_addrs: vec![Url::parse(&format!("tcp://127.0.0.1:{}", 13200 + i)).unwrap()],
            outbound_connections: 0,
            outbound_connect_timeout: 2,
            inbound_connections: usize::MAX,
            peers,
            allowed_transports: vec!["tcp".to_string()],
            ..Default::default()
        };

        let p2p = P2p::new(settings, ex.clone()).await;
        let sled_db = sled::Config::new().temporary(true).open().unwrap();
        let event_graph =
            EventGraph::new(p2p.clone(), &sled_db, "dag", 1, ex.clone()).await.unwrap();
        let event_graph_ = event_graph.clone();

        // Take the last sled item since there's only 1
        if genesis_event_id == NULL_ID {
            let (id, _) = event_graph.dag.last().unwrap().unwrap();
            genesis_event_id = blake3::Hash::from_bytes((&id as &[u8]).try_into().unwrap());
        }

        // Register the P2P protocols
        let registry = p2p.protocol_registry();
        registry
            .register(SESSION_ALL, move |channel, _| {
                let event_graph_ = event_graph_.clone();
                async move { ProtocolEventGraph::init(event_graph_, channel).await.unwrap() }
            })
            .await;

        eg_instances.push(event_graph);
    }

    // Start the P2P network
    for eg in eg_instances.iter() {
        eg.p2p.clone().start().await.unwrap();
    }

    info!("Waiting 10s until all peers connect");
    sleep(10).await;

    // =========================================
    // 1. Assert that everyone's DAG is the same
    // =========================================
    for (i, eg) in eg_instances.iter().enumerate() {
        let tips = eg.unreferenced_tips.read().await;
        assert!(eg.dag.len() == 1, "Node {}", i);
        assert!(tips.len() == 1, "Node {}", i);
        assert!(tips.get(&genesis_event_id).is_some(), "Node {}", i);
    }

    // ==========================================
    // 2. Create an event in one node and publish
    // ==========================================
    let random_node = eg_instances.choose(&mut rand::thread_rng()).unwrap();
    let event = Event::new(vec![1, 2, 3, 4], random_node.clone()).await;
    assert!(event.parents.contains(&genesis_event_id));
    // The node adds it to their DAG.
    let event_id = random_node.dag_insert(&event).await.unwrap();
    let tips = random_node.unreferenced_tips.read().await;
    assert!(tips.len() == 1);
    assert!(tips.get(&event_id).is_some());
    drop(tips);
    info!("Broadcasting event {}", event_id);
    random_node.p2p.broadcast(&EventPut(event)).await;
    info!("Waiting 10s for event propagation");
    sleep(10).await;

    // ====================================================
    // 3. Assert that everyone has the new event in the DAG
    // ====================================================
    for (i, eg) in eg_instances.iter().enumerate() {
        let tips = eg.unreferenced_tips.read().await;
        assert!(eg.dag.len() == 2, "Node {}", i);
        assert!(tips.len() == 1, "Node {}", i);
        assert!(tips.get(&event_id).is_some(), "Node {}", i);
    }

    // ==============================================================
    // 4. Create multiple events on a node and broadcast the last one
    //    The `EventPut` logic should manage to fetch all of them,
    //    provided that the last one references the earlier ones.
    // ==============================================================
    let random_node = eg_instances.choose(&mut rand::thread_rng()).unwrap();
    let event0 = Event::new(vec![1, 2, 3, 4, 0], random_node.clone()).await;
    let event0_id = random_node.dag_insert(&event0).await.unwrap();
    let event1 = Event::new(vec![1, 2, 3, 4, 1], random_node.clone()).await;
    let event1_id = random_node.dag_insert(&event1).await.unwrap();
    let event2 = Event::new(vec![1, 2, 3, 4, 2], random_node.clone()).await;
    let event2_id = random_node.dag_insert(&event2).await.unwrap();
    // Genesis event + event from 2. + upper 3 events
    assert!(random_node.dag.len() == 5);
    let tips = random_node.unreferenced_tips.read().await;
    assert!(tips.len() == 1);
    assert!(tips.get(&event2_id).is_some());
    drop(tips);

    let event_chain =
        vec![(event0_id, event0.parents), (event1_id, event1.parents), (event2_id, event2.parents)];

    info!("Broadcasting event {}", event2_id);
    info!("Event chain: {:#?}", event_chain);
    random_node.p2p.broadcast(&EventPut(event2)).await;
    info!("Waiting 10s for event propagation");
    sleep(10).await;

    // ==========================================
    // 5. Assert that everyone has all the events
    // ==========================================
    for (i, eg) in eg_instances.iter().enumerate() {
        let tips = eg.unreferenced_tips.read().await;
        assert!(eg.dag.len() == 5, "Node {}, expected 5 events, have {}", i, eg.dag.len());
        assert!(tips.len() == 1, "Node {}, expected 1 tip, have {}", i, tips.len());
        assert!(tips.get(&event2_id).is_some(), "Node {}, expected tip to be {}", i, event2_id);
    }

    // ===========================================
    // 6. Create multiple events on multiple nodes
    // ===========================================

    // ==========================================
    // 7. Assert that everyone has all the events
    // ==========================================

    // Stop the P2P network
    for eg in eg_instances.iter() {
        eg.p2p.clone().stop().await;
    }
}
