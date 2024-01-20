/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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

// cargo +nightly test --release --features=net --lib p2p -- --include-ignored

use std::sync::Arc;

use log::{debug, info, warn};
use rand::Rng;
use smol::{channel, future, Executor};
use url::Url;

use crate::{
    net::{P2p, Settings},
    system::sleep,
};

// Number of nodes to spawn and number of peers each node connects to
const N_NODES: usize = 10;
const N_CONNS: usize = 5;

#[test]
fn p2p_test() {
    let mut cfg = simplelog::ConfigBuilder::new();

    cfg.add_filter_ignore("sled".to_string());
    cfg.add_filter_ignore("net::protocol_ping".to_string());
    cfg.add_filter_ignore("net::channel::subscribe_stop()".to_string());
    cfg.add_filter_ignore("net::hosts".to_string());
    cfg.add_filter_ignore("net::inbound_session".to_string());
    cfg.add_filter_ignore("net::outbound_session".to_string());
    cfg.add_filter_ignore("net::session".to_string());
    cfg.add_filter_ignore("net::refinery".to_string());
    cfg.add_filter_ignore("net::message_subscriber".to_string());
    cfg.add_filter_ignore("net::protocol_address".to_string());
    cfg.add_filter_ignore("net::protocol_jobs_manager".to_string());
    cfg.add_filter_ignore("net::protocol_version".to_string());
    cfg.add_filter_ignore("net::protocol_registry".to_string());
    cfg.add_filter_ignore("net::protocol_seed".to_string());
    cfg.add_filter_ignore("net::channel".to_string());
    cfg.add_filter_ignore("net::p2p::seed".to_string());
    cfg.add_filter_ignore("net::p2p::start".to_string());
    cfg.add_filter_ignore("store".to_string());
    cfg.add_filter_ignore("net::store".to_string());
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
        warn!(target: "net::test", "Logger already initialized");
    }

    let ex = Arc::new(Executor::new());
    let ex_ = ex.clone();
    let (signal, shutdown) = channel::unbounded::<()>();

    // Run a thread for each node.
    easy_parallel::Parallel::new()
        .each(0..N_NODES, |_| future::block_on(ex.run(shutdown.recv())))
        .finish(|| {
            future::block_on(async {
                hostlist_propagation(ex_).await;
                drop(signal);
            })
        });
}

async fn hostlist_propagation(ex: Arc<Executor<'static>>) {
    let seed_addr = Url::parse(&format!("tcp://127.0.0.1:{}", 51505)).unwrap();

    let mut p2p_instances = vec![];
    let mut rng = rand::thread_rng();

    let settings = Settings {
        localnet: true,
        inbound_addrs: vec![seed_addr.clone()],
        external_addrs: vec![seed_addr.clone()],
        outbound_connections: 0,
        //outbound_connect_timeout: 10,
        inbound_connections: usize::MAX,
        seeds: vec![],
        hostlist: String::from("~/.local/darkfi/p2p-test/seed.tsv"),
        peers: vec![],
        allowed_transports: vec!["tcp".to_string()],
        node_id: "seed".to_string(),
        ..Default::default()
    };

    let p2p = P2p::new(settings, ex.clone()).await;
    p2p_instances.push(p2p);

    info!("Initializing outbound nodes");
    for i in 0..N_NODES {
        // Everyone will connect to N_CONNS random peers.
        let mut peers = vec![];
        for _ in 0..N_CONNS {
            let mut port = 53200 + i;
            while port == 53200 + i {
                port = 53200 + rng.gen_range(0..N_NODES);
            }
            peers.push(Url::parse(&format!("tcp://127.0.0.1:{}", port)).unwrap());
        }
        let settings = Settings {
            localnet: true,
            inbound_addrs: vec![Url::parse(&format!("tcp://127.0.0.1:{}", 53200 + i)).unwrap()],
            external_addrs: vec![Url::parse(&format!("tcp://127.0.0.1:{}", 53200 + i)).unwrap()],
            outbound_connections: 8,
            //outbound_connect_timeout: 10,
            inbound_connections: usize::MAX,
            seeds: vec![seed_addr.clone()],
            hostlist: format!("~/.local/darkfi/p2p-test/hosts{}.tsv", i),
            peers,
            allowed_transports: vec!["tcp".to_string()],
            node_id: i.to_string(),
            anchor_connection_count: 2,
            ..Default::default()
        };

        let p2p = P2p::new(settings, ex.clone()).await;
        p2p_instances.push(p2p);
    }
    // Start the P2P network
    for p2p in p2p_instances.iter() {
        p2p.clone().start().await.unwrap();
    }

    info!("Waiting until all peers connect");
    sleep(10).await;

    info!("Inspecting hostlists...");
    for p2p in p2p_instances.iter() {
        let hosts = p2p.hosts();

        let greylist = hosts.greylist.read().await;
        let whitelist = hosts.whitelist.read().await;
        let anchorlist = hosts.anchorlist.read().await;

        info!("Node {}", p2p.settings().node_id);
        for (i, (url, last_seen)) in greylist.iter().enumerate() {
            info!("Greylist entry {}: {}, {}", i, url, last_seen);
        }

        for (i, (url, last_seen)) in whitelist.iter().enumerate() {
            info!("Whitelist entry {}: {}, {}", i, url, last_seen);
        }

        for (i, (url, last_seen)) in anchorlist.iter().enumerate() {
            info!("Anchorlist entry {}: {}, {}", i, url, last_seen);
        }
    }

    // Stop the P2P network
    for p2p in p2p_instances.iter() {
        debug!("Stopping P2P instances...");
        p2p.clone().stop().await;
        debug!("Node {} stopped!", p2p.settings().node_id);
    }
}
