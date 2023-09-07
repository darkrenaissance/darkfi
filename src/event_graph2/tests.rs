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

use std::{collections::HashMap, sync::Arc};

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

/// Number of nodes to spawn
const N_NODES: usize = 50;
//const N_NODES: usize = 2;
/// Number of peers each node connects to
const N_CONNS: usize = N_NODES / 3;
//const N_CONNS: usize = 1;

#[test]
#[ignore]
fn eventgraph_propagation() {
    let mut cfg = simplelog::ConfigBuilder::new();
    cfg.add_filter_ignore("sled".to_string());
    cfg.add_filter_ignore("net::protocol_ping".to_string());
    cfg.add_filter_ignore("net::channel::subscribe_stop()".to_string());
    cfg.add_filter_ignore("net::hosts".to_string());
    cfg.add_filter_ignore("net::message_subscriber".to_string());
    cfg.add_filter_ignore("net::protocol_address".to_string());
    cfg.add_filter_ignore("net::protocol_version".to_string());
    cfg.add_filter_ignore("net::channel::send()".to_string());

    simplelog::TermLogger::init(
        simplelog::LevelFilter::Info,
        //simplelog::LevelFilter::Debug,
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

    // Now we get to the logic.
    //for i in 1..1001_u16 {
    for i in 1..3_u16 {
        // A random node creates an event
        let random_node = eg_instances.choose(&mut rand::thread_rng()).unwrap();
        let event = Event::new(i.to_le_bytes().to_vec(), random_node.clone()).await;

        // The node adds it to their DAG
        let event_id = random_node.dag_insert(&event).await.unwrap();
        // The node broadcasts it
        info!("Broadcasting {}", event_id);
        random_node.p2p.broadcast(&EventPut(event)).await;

        // Another random node creates five events and sends them out of order.
        let random_node = eg_instances.choose(&mut rand::thread_rng()).unwrap();

        let event0 = Event::new(i.to_le_bytes().to_vec(), random_node.clone()).await;
        let event0_id = random_node.dag_insert(&event0).await.unwrap();

        let event1 = Event::new(i.to_le_bytes().to_vec(), random_node.clone()).await;
        let event1_id = random_node.dag_insert(&event1).await.unwrap();

        let event2 = Event::new(i.to_le_bytes().to_vec(), random_node.clone()).await;
        let event2_id = random_node.dag_insert(&event2).await.unwrap();

        let event3 = Event::new(i.to_le_bytes().to_vec(), random_node.clone()).await;
        let event3_id = random_node.dag_insert(&event3).await.unwrap();

        let event4 = Event::new(i.to_le_bytes().to_vec(), random_node.clone()).await;
        let event4_id = random_node.dag_insert(&event4).await.unwrap();

        info!("Broadcasting {}", event3_id);
        random_node.p2p.broadcast(&EventPut(event3)).await;
        info!("Broadcasting {}", event2_id);
        random_node.p2p.broadcast(&EventPut(event2)).await;
        info!("Broadcasting {}", event4_id);
        random_node.p2p.broadcast(&EventPut(event4)).await;
        info!("Broadcasting {}", event1_id);
        random_node.p2p.broadcast(&EventPut(event1)).await;
        info!("Broadcasting {}", event0_id);
        random_node.p2p.broadcast(&EventPut(event0)).await;
    }

    info!("Waiting 20s until the p2p broadcasts settle");
    sleep(20).await;

    // Assert that everyone has the same DAG
    let mut contents = HashMap::new();
    for (i, eg) in eg_instances.iter().enumerate() {
        let mut ids = vec![];
        for r in eg.dag.iter() {
            let (id, _) = r.unwrap();
            ids.push(blake3::Hash::from_bytes((&id as &[u8]).try_into().unwrap()));
        }

        contents.insert(i, ids);
    }
    let value = contents.values().next().unwrap();
    assert!(contents.values().all(|v| v == value));

    // Assert that everyone's DAG sorts the same.
    let mut orders = HashMap::new();
    for (i, eg) in eg_instances.iter().enumerate() {
        let order = eg.order_events().await;
        orders.insert(i, order);
    }
    let value = orders.values().next().unwrap();
    for (i, order) in orders.iter() {
        assert!(
            order == value,
            "Node {} has wrong order:\n{:#?}\nvs{:#?}\nGENESIS:{}",
            i,
            order,
            value,
            genesis_event_id,
        );
    }

    // Stop the P2P network
    for eg in eg_instances.iter() {
        eg.p2p.clone().stop().await;
    }
}
