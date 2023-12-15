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

// cargo +nightly test --release --all-features --lib p2p_propagation-- --include-ignored

use std::sync::Arc;

use log::info;
use rand::{prelude::SliceRandom, Rng};
use smol::{channel, future, Executor};
use url::Url;

use crate::{
    net::{P2p, Settings, SESSION_ALL},
    system::sleep,
};

// Number of nodes to spawn and number of peers each node connects to
const N_NODES: usize = 10;
const N_CONNS: usize = 2;

// TODO: test whitelist propagation between peers
// TODO: test whitelist propagation from lilith to peers
// TODO: test greylist storage and sorting
// TODO: test greylist/ whitelist refining and refreshing
#[test]
fn p2p_test() {
    let mut cfg = simplelog::ConfigBuilder::new();
    cfg.add_filter_ignore("sled".to_string());
    cfg.add_filter_ignore("net::protocol_ping".to_string());
    cfg.add_filter_ignore("net::channel::subscribe_stop()".to_string());
    cfg.add_filter_ignore("net::hosts".to_string());
    cfg.add_filter_ignore("net::session".to_string());
    cfg.add_filter_ignore("net::message_subscriber".to_string());
    cfg.add_filter_ignore("net::protocol_version".to_string());
    cfg.add_filter_ignore("net::protocol_jobs_manager".to_string());
    cfg.add_filter_ignore("net::protocol_registry".to_string());
    cfg.add_filter_ignore("net::channel::send()".to_string());
    cfg.add_filter_ignore("net::channel::start()".to_string());
    cfg.add_filter_ignore("net::channel::stop()".to_string());
    cfg.add_filter_ignore("net::channel::handle_stop()".to_string());
    cfg.add_filter_ignore("net::channel::subscribe_msg()".to_string());
    cfg.add_filter_ignore("net::channel::main_receive_loop()".to_string());
    cfg.add_filter_ignore("net::greylist_refinery::run()".to_string());
    cfg.add_filter_ignore("net::outbound_session::try_connect()".to_string());

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
                p2p_propagation_real(ex_).await;
                drop(signal);
            })
        });
}

async fn spawn_seed(ex: Arc<Executor<'static>>) {
    let inbound_addrs = vec![Url::parse(&format!("tcp://127.0.0.1:{}", 51505)).unwrap()];
    let settings = Settings {
        localnet: true,
        inbound_addrs: inbound_addrs.clone(),
        outbound_connections: N_NODES,
        outbound_connect_timeout: 2,
        inbound_connections: usize::MAX,
        peers: vec![],
        allowed_transports: vec!["tcp".to_string()],
        ..Default::default()
    };

    let p2p = P2p::new(settings, ex.clone()).await;
    info!("Starting seed network node for on {:?}", inbound_addrs);
    p2p.clone().start().await.unwrap();
}

async fn p2p_propagation_real(ex: Arc<Executor<'static>>) {
    spawn_seed(ex.clone()).await;

    let mut p2p_instances = vec![];
    let mut rng = rand::thread_rng();

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
            outbound_connections: 5,
            outbound_connect_timeout: 2,
            inbound_connections: usize::MAX,
            seeds: vec![Url::parse(&format!("tcp://127.0.0.1:{}", 51505)).unwrap()],
            peers,
            allowed_transports: vec!["tcp".to_string()],
            ..Default::default()
        };

        let p2p = P2p::new(settings, ex.clone()).await;
        p2p_instances.push(p2p);
    }
    // Start the P2P network
    for p2p in p2p_instances.iter() {
        p2p.clone().start().await.unwrap();
    }

    info!("Waiting 10s until all peers connect");
    sleep(10).await;

    // Check the greylist...
    for p2p in p2p_instances.iter() {
        let hosts = p2p.hosts();
        info!("Whitelist {:?}", hosts.whitelist.read().await);
        info!("Greylist {:?}", hosts.whitelist.read().await);

        sleep(10).await;

        info!("Whitelist {:?}", hosts.whitelist.read().await);
        info!("Greylist {:?}", hosts.whitelist.read().await);
    }

    // Stop the P2P network
    for p2p in p2p_instances.iter() {
        p2p.clone().stop().await;
    }
}
