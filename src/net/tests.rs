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

use std::{collections::HashSet, net::TcpListener, panic, sync::Arc};

use log::{error, info, warn};
use rand::{prelude::SliceRandom, rngs::ThreadRng, Rng};
use smol::{channel, future, Executor};
use url::Url;

use crate::{
    net::{hosts::HostColor, P2p, Settings},
    system::sleep,
};

// Number of nodes to spawn and number of peers each node connects to
const N_NODES: usize = 5;
const N_CONNS: usize = 4;

fn init_logger() {
    let mut cfg = simplelog::ConfigBuilder::new();
    cfg.add_filter_ignore("sled".to_string());
    cfg.add_filter_ignore("net::protocol_ping".to_string());
    cfg.add_filter_ignore("net::channel::subscribe_stop()".to_string());
    cfg.add_filter_ignore("net::hosts".to_string());
    cfg.add_filter_ignore("net::session".to_string());
    cfg.add_filter_ignore("net::outbound_session".to_string());
    cfg.add_filter_ignore("net::inbound_session".to_string());
    cfg.add_filter_ignore("net::message_publisher".to_string());
    cfg.add_filter_ignore("net::protocol_address".to_string());
    cfg.add_filter_ignore("net::protocol_version".to_string());
    cfg.add_filter_ignore("net::protocol_registry".to_string());
    cfg.add_filter_ignore("net::protocol_jobs_manager".to_string());
    cfg.add_filter_ignore("net::channel::send()".to_string());
    cfg.add_filter_ignore("net::channel::start()".to_string());
    cfg.add_filter_ignore("net::channel::handle_stop()".to_string());
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

fn get_random_available_port() -> usize {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    port.into()
}

fn get_unique_ports() -> Vec<usize> {
    let mut ports = HashSet::new();

    while ports.len() < N_NODES {
        ports.insert(get_random_available_port());
    }

    ports.into_iter().collect()
}

async fn spawn_seed_session(seed_addr: Url, ex: Arc<Executor<'static>>) -> Vec<Arc<P2p>> {
    info!("========================================================");
    info!("Initializing outbound nodes...");
    info!("========================================================");

    let mut outbound_instances = vec![];
    let ports = get_unique_ports();

    for port in ports {
        let settings = Settings {
            localnet: true,
            inbound_addrs: vec![Url::parse(&format!("tcp://127.0.0.1:{}", port)).unwrap()],
            external_addrs: vec![Url::parse(&format!("tcp://127.0.0.1:{}", port)).unwrap()],
            outbound_connections: 2,
            outbound_peer_discovery_cooloff_time: 2,
            outbound_connect_timeout: 2,
            inbound_connections: usize::MAX,
            greylist_refinery_interval: 15,
            peers: vec![],
            seeds: vec![seed_addr.clone()],
            node_id: (port).to_string(),
            allowed_transports: vec!["tcp".to_string()],
            ..Default::default()
        };

        let p2p = P2p::new(settings, ex.clone()).await.unwrap();
        outbound_instances.push(p2p);
    }

    outbound_instances
}

async fn spawn_manual_session(ex: Arc<Executor<'static>>) -> Vec<Arc<P2p>> {
    info!("========================================================");
    info!("Initializing manual nodes...");
    info!("========================================================");
    let mut manual_instances = vec![];
    let mut rng = rand::thread_rng();
    let ports = get_unique_ports();

    for i in 0..N_NODES {
        let mut peer_indexes_copy: Vec<usize> = (0..N_NODES).collect();
        peer_indexes_copy.remove(i);
        let peer_indexes_to_connect: Vec<_> =
            peer_indexes_copy.choose_multiple(&mut rng, N_CONNS).collect();

        let mut peers = vec![];
        for &peer_index in peer_indexes_to_connect {
            let port = ports[peer_index];
            peers.push(Url::parse(&format!("tcp://127.0.0.1:{}", port)).unwrap());
        }

        let inbound_port = ports[i];
        let settings = Settings {
            localnet: true,
            inbound_addrs: vec![Url::parse(&format!("tcp://127.0.0.1:{}", inbound_port)).unwrap()],
            external_addrs: vec![Url::parse(&format!("tcp://127.0.0.1:{}", inbound_port)).unwrap()],
            outbound_connections: 2,
            outbound_peer_discovery_cooloff_time: 2,
            outbound_connect_timeout: 2,
            inbound_connections: usize::MAX,
            greylist_refinery_interval: 15,
            peers,
            seeds: vec![],
            node_id: inbound_port.to_string(),
            allowed_transports: vec!["tcp".to_string()],
            ..Default::default()
        };

        let p2p = P2p::new(settings, ex.clone()).await.unwrap();
        manual_instances.push(p2p);
    }
    manual_instances
}

async fn get_random_gold_host(
    outbound_instances: &[Arc<P2p>],
    index: usize,
) -> ((Url, u64), usize) {
    let random_node = &outbound_instances[index];
    let hosts = random_node.hosts();
    let external_addr = random_node.settings().read().await.external_addrs[0].clone();

    info!("========================================================");
    info!("Getting gold addr from node={}", external_addr);
    info!("========================================================");

    let list = hosts.container.hostlists[HostColor::Gold as usize].read().unwrap();
    assert!(!list.is_empty());
    let position = rand::thread_rng().gen_range(0..list.len());
    let entry = &list[position];
    (entry.clone(), position)
}

async fn _check_random_hostlist(outbound_instances: &Vec<Arc<P2p>>, rng: &mut ThreadRng) {
    let mut urls = HashSet::new();
    let random_node = outbound_instances.choose(rng).unwrap();
    let external_addr = random_node.settings().read().await.external_addrs[0].clone();

    info!("========================================================");
    info!("Checking node={}", external_addr);
    info!("========================================================");

    let greylist = random_node.hosts().container.fetch_all(HostColor::Grey);
    let whitelist = random_node.hosts().container.fetch_all(HostColor::White);
    let goldlist = random_node.hosts().container.fetch_all(HostColor::Gold);

    for (url, _) in greylist {
        assert!(urls.insert(url));
    }
    for (url, _) in whitelist {
        assert!(urls.insert(url));
    }
    for (url, _) in goldlist {
        assert!(urls.insert(url));
    }
    assert!(!urls.is_empty());
}

async fn check_all_hostlist(outbound_instances: &Vec<Arc<P2p>>) {
    for node in outbound_instances {
        let external_addr = &node.settings().read().await.external_addrs[0].clone();
        info!("========================================================");
        info!("Checking node={}", external_addr);
        info!("========================================================");

        let mut urls = HashSet::new();
        let greylist = node.hosts().container.fetch_all(HostColor::Grey);
        let whitelist = node.hosts().container.fetch_all(HostColor::White);
        let goldlist = node.hosts().container.fetch_all(HostColor::Gold);

        for (url, _) in greylist {
            assert!(urls.insert(url));
        }
        for (url, _) in whitelist {
            assert!(urls.insert(url));
        }
        for (url, _) in goldlist {
            assert!(urls.insert(url));
        }
        assert!(!urls.is_empty());
    }
}
async fn kill_node(outbound_instances: &Vec<Arc<P2p>>, node: Url) {
    for p2p in outbound_instances {
        if p2p.settings().read().await.external_addrs[0] == node {
            info!("========================================================");
            info!("Shutting down node: {}", p2p.settings().read().await.external_addrs[0]);
            info!("========================================================");
            p2p.stop().await;
        }
    }
}

macro_rules! test_body {
    ($real_call:ident) => {
        init_logger();

        let ex = Arc::new(Executor::new());
        let ex_ = ex.clone();
        let (signal, shutdown) = channel::unbounded::<()>();

        panic::set_hook(Box::new(|panic_info| {
            error!("Panic occurred: {:?}", panic_info);
        }));

        // Run a thread for each node.
        easy_parallel::Parallel::new()
            .each(0..N_NODES, |_| {
                let result = std::panic::catch_unwind(|| {
                    let res = future::block_on(ex.run(shutdown.recv()));
                    res
                });
                if let Err(err) = result {
                    error!("Thread panicked: {:?}", err);
                }
            })
            .finish(|| {
                future::block_on(async {
                    $real_call(ex_).await;
                    drop(signal);
                });
            });
    };
}

#[test]
fn p2p_test() {
    test_body!(p2p_test_real);
}

async fn p2p_test_real(ex: Arc<Executor<'static>>) {
    // ============================================================
    // 1. Create a new seed node.
    // ============================================================
    let seed_port = get_random_available_port();
    let seed_addr = Url::parse(&format!("tcp://127.0.0.1:{}", seed_port)).unwrap();

    let settings = Settings {
        localnet: true,
        inbound_addrs: vec![seed_addr.clone()],
        outbound_connections: 0,
        inbound_connections: usize::MAX,
        seeds: vec![],
        peers: vec![],
        allowed_transports: vec!["tcp".to_string()],
        greylist_refinery_interval: 12,
        node_id: "seed".to_string(),
        ..Default::default()
    };

    let seed = P2p::new(settings, ex.clone()).await.unwrap();
    info!("========================================================");
    info!("Starting seed node on {}", seed_addr);
    info!("========================================================");
    seed.clone().start().await.unwrap();

    // ============================================================
    // 2. Spawn outbound nodes that will connect to the seed node.
    // ============================================================
    let outbound_instances = spawn_seed_session(seed_addr, ex.clone()).await;

    for p2p in &outbound_instances {
        info!("========================================================");
        info!("Starting node={}", p2p.settings().read().await.external_addrs[0]);
        info!("========================================================");
        p2p.clone().start().await.unwrap();
    }

    info!("========================================================");
    info!("Waiting 10s for all peers to reach the seed node");
    info!("========================================================");
    sleep(10).await;

    // ===========================================================
    // 3. Assert that all nodes have shared their external addr
    //    with the seed node.
    // ===========================================================
    let greylist = seed.hosts().container.fetch_all(HostColor::Grey);
    assert!(greylist.len() == N_NODES);
    info!("========================================================");
    info!("Seedsync session successful!");
    info!("========================================================");

    info!("========================================================");
    info!("Waiting 5s for seed node refinery to kick in...");
    info!("========================================================");
    sleep(5).await;

    // ===========================================================
    // 4. Assert that seed node has at least one whitelist entry,
    //    indicating that the refinery process is happening correctly.
    // ===========================================================
    assert!(!seed.hosts().container.is_empty(HostColor::White));

    info!("========================================================");
    info!("Checking seed={}", seed.settings().read().await.inbound_addrs[0]);
    info!("========================================================");

    let mut urls = HashSet::new();
    let greylist = seed.hosts().container.fetch_all(HostColor::Grey);
    let whitelist = seed.hosts().container.fetch_all(HostColor::White);
    let goldlist = seed.hosts().container.fetch_all(HostColor::Gold);

    for (url, _) in greylist {
        info!("Found grey url: {}", url);
        assert!(urls.insert(url));
    }
    for (url, _) in whitelist {
        info!("Found white url: {}", url);
        assert!(urls.insert(url));
    }
    for (url, _) in goldlist {
        info!("Found gold url: {}", url);
        assert!(urls.insert(url));
    }
    assert!(!urls.is_empty());

    info!("========================================================");
    info!("Seed node refinery operating successfully!");
    info!("========================================================");

    info!("========================================================");
    info!("Waiting 10s for seed refinery...");
    info!("========================================================");
    sleep(10).await;

    let whitelist = seed.hosts().container.fetch_all(HostColor::White);
    assert!(whitelist.len() >= 2);
    // ===========================================================
    // 5. Select a random peer and ensure that its hostlist is not
    //    empty. This ensures the seed node is sharing whitelisted
    //    nodes around the network.
    // ===========================================================
    check_all_hostlist(&outbound_instances).await;
    info!("========================================================");
    info!("Peers successfully received addrs!");
    info!("========================================================");

    info!("========================================================");
    info!("Waiting 5s for outbound loop to connect...");
    info!("========================================================");
    sleep(5).await;

    // ===========================================================
    // 6. Select a random gold peer from one of the nodes and kill
    //    it.
    // ===========================================================
    info!("========================================================");
    info!("Selecting a random gold entry...");
    info!("========================================================");

    let random_node_index = rand::thread_rng().gen_range(0..outbound_instances.len());
    let ((addr, _), _) = get_random_gold_host(&outbound_instances, random_node_index).await;

    kill_node(&outbound_instances, addr.clone()).await;

    info!("========================================================");
    info!("Waiting for greylist downgrade sequence to occur...");
    info!("========================================================");

    // ===========================================================
    // 7. Verify the peer has been removed from the Gold list.
    // ===========================================================
    outbound_instances[random_node_index]
        .hosts()
        .container
        .contains(HostColor::Grey as usize, &addr);
    info!("========================================================");
    info!("Greylist downgrade occured successfully!");
    info!("========================================================");

    info!("========================================================");
    info!("Seed session successful! Shutting down seed test...");
    info!("========================================================");
    // ===========================================================
    // 8. Stop the P2P network
    // ===========================================================
    for p2p in outbound_instances.iter() {
        p2p.clone().stop().await;
    }
    seed.clone().stop().await;

    info!("========================================================");
    info!("Seed test shutdown complete! Starting manual test...");
    info!("========================================================");

    let manual_instances = spawn_manual_session(ex.clone()).await;

    for p2p in &manual_instances {
        info!("========================================================");
        info!("Starting node={}", p2p.settings().read().await.external_addrs[0]);
        info!("========================================================");
        p2p.clone().start().await.unwrap();
    }

    info!("========================================================");
    info!("Waiting 5s for all manual peers to connect");
    info!("========================================================");
    sleep(5).await;

    info!("========================================================");
    info!("Checking manual nodes connected successfully...");
    info!("========================================================");

    for p2p in manual_instances.clone() {
        // We should have (N_CONNS outbound + N_CONNS inbound)
        // connections at this point.
        info!("========================================================");
        info!("Checking manual node={}", p2p.settings().read().await.node_id);
        info!("========================================================");
        let channels = p2p.hosts().channels();
        assert!(channels.len() == N_CONNS * 2);
    }

    info!("========================================================");
    info!("Manual session successful! Shutting down manual test...");
    info!("========================================================");
    // ===========================================================
    // 8. Stop the P2P network
    // ===========================================================
    for p2p in manual_instances.clone() {
        p2p.clone().stop().await;
    }
}
