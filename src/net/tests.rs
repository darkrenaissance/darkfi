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

// cargo +nightly test --release --all-features --lib p2p -- --include-ignored

use std::{sync::Arc, time::SystemTime};

use log::info;
use rand::{prelude::SliceRandom, Rng};
use smol::{channel, future, Executor};
use url::Url;

use crate::{
    net::{P2p, Settings, SESSION_ALL},
    system::sleep,
};

// Number of nodes to spawn and number of peers each node connects to
const N_NODES: usize = 5;
const N_CONNS: usize = 2;

// TODO: test whitelist propagation between peers
// TODO: test whitelist propagation from lilith to peers
// TODO: test greylist storage and sorting
// TODO: test greylist/ whitelist refining and refreshing
#[test]
fn p2p_test() {
    let mut cfg = simplelog::ConfigBuilder::new();
    //cfg.add_filter_ignore("sled".to_string());
    cfg.add_filter_ignore("net::channel::subscribe_stop()".to_string());
    //cfg.add_filter_ignore("net::hosts".to_string());
    //cfg.add_filter_ignore("net::session".to_string());
    cfg.add_filter_ignore("net::message_subscriber".to_string());
    //cfg.add_filter_ignore("net::protocol_ping".to_string());
    //cfg.add_filter_ignore("net::protocol_version".to_string());
    //cfg.add_filter_ignore("net::protocol_jobs_manager".to_string());
    //cfg.add_filter_ignore("net::protocol_registry".to_string());
    //cfg.add_filter_ignore("net::channel::send()".to_string());
    //cfg.add_filter_ignore("net::channel::start()".to_string());
    //cfg.add_filter_ignore("net::channel::stop()".to_string());
    //cfg.add_filter_ignore("net::channel::handle_stop()".to_string());
    //cfg.add_filter_ignore("net::channel::subscribe_msg()".to_string());
    //cfg.add_filter_ignore("net::channel::main_receive_loop()".to_string());
    //cfg.add_filter_ignore("net::greylist_refinery::run()".to_string());
    //cfg.add_filter_ignore("net::outbound_session::try_connect()".to_string());

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
                hostlist_propagation(ex_).await;
                drop(signal);
            })
        });
}

async fn hostlist_propagation(ex: Arc<Executor<'static>>) {
    let seed_addr = Url::parse(&format!("tcp://127.0.0.1:{}", 51505)).unwrap();

    let mut p2p_instances = vec![];
    let mut rng = rand::thread_rng();

    info!("Initializing seed network");
    let settings = Settings {
        localnet: true,
        inbound_addrs: vec![seed_addr.clone()],
        external_addrs: vec![seed_addr.clone()],
        outbound_connections: 0,
        outbound_connect_timeout: 2,
        inbound_connections: usize::MAX,
        peers: vec![],
        allowed_transports: vec!["tcp".to_string()],
        node_id: "seed".to_string(),
        //advertise: true,
        ..Default::default()
    };

    let p2p = P2p::new(settings, ex.clone()).await;
    p2p_instances.push(p2p);

    info!("Initializing outbound nodes");
    for i in 0..N_NODES {
        let settings = Settings {
            localnet: true,
            inbound_addrs: vec![Url::parse(&format!("tcp://127.0.0.1:{}", 13200 + i)).unwrap()],
            external_addrs: vec![Url::parse(&format!("tcp://127.0.0.1:{}", 13200 + i)).unwrap()],
            outbound_connections: 2,
            outbound_connect_timeout: 10,
            inbound_connections: usize::MAX,
            seeds: vec![seed_addr.clone()],
            peers: vec![],
            allowed_transports: vec!["tcp".to_string()],
            node_id: i.to_string(),
            //advertise: true,
            ..Default::default()
        };

        let p2p = P2p::new(settings, ex.clone()).await;
        p2p_instances.push(p2p);
    }
    // Start the P2P network
    for p2p in p2p_instances.iter() {
        assert!(p2p.settings().advertise == true);
        p2p.clone().start().await.unwrap();
    }

    info!("Waiting 30s until all peers connect");
    sleep(30).await;

    info!("Inspecting peerlists...");
    for p2p in p2p_instances.iter() {
        let hosts = p2p.hosts();
        info!("START peerlist {}", p2p.settings().node_id);
        assert!(!hosts.is_empty_greylist().await);
        let greylist = hosts.greylist.read().await;
        for (url, last_seen) in greylist.iter() {
            info!("{}", url);
            info!("{}", last_seen);
        }
        info!("END peerlist {}", p2p.settings().node_id);
    }

    // Stop the P2P network
    for p2p in p2p_instances.iter() {
        p2p.clone().stop().await;
    }
}
