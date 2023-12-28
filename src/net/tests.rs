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

// cargo +nightly test --release --features=net --lib p2p -- --include-ignored

use std::sync::Arc;

use log::info;
use rand::Rng;
use smol::{channel, future, Executor};
use url::Url;

use crate::{
    net::{P2p, Settings},
    system::sleep,
};

// Number of nodes to spawn and number of peers each node connects to
const N_NODES: usize = 3;
const N_CONNS: usize = 2;

#[test]
fn p2p_test() {
    let mut cfg = simplelog::ConfigBuilder::new();

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

    info!("Initializing outbound nodes");
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
            external_addrs: vec![Url::parse(&format!("tcp://127.0.0.1:{}", 13200 + i)).unwrap()],
            outbound_connections: 2,
            outbound_connect_timeout: 10,
            inbound_connections: usize::MAX,
            seeds: vec![seed_addr.clone()],
            hostlist: String::from(format!(".config/darkfi/hosts{}.tsv", i)),
            peers,
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

    info!("Waiting until all peers connect");
    sleep(60).await;

    info!("Inspecting hostlists...");
    for p2p in p2p_instances.iter() {
        let hosts = p2p.hosts();
        //assert!(!hosts.is_empty_greylist().await);
        //assert!(!hosts.is_empty_whitelist().await);
        //assert!(!hosts.is_empty_anchorlist().await);

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
        p2p.clone().stop().await;
    }
}
