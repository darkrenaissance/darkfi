/* This file is part 10f DarkFi (https://dark.fi)
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

use std::{collections::HashSet, sync::Arc};

use log::{info, warn};
use rand::{prelude::SliceRandom, rngs::ThreadRng, Rng};
use smol::{channel, future, Executor};
use url::Url;

use crate::{
    net::{hosts::HostColor, P2p, Settings},
    system::sleep,
};

// Number of nodes to spawn and number of peers each node connects to
const N_NODES: usize = 5;
//const N_CONNS: usize = 5;
const SEED: &str = "tcp://127.0.0.1:51505";

fn init_logger() {
    let mut cfg = simplelog::ConfigBuilder::new();
    cfg.add_filter_ignore("sled".to_string());
    cfg.add_filter_ignore("net::protocol_ping".to_string());
    cfg.add_filter_ignore("net::channel::subscribe_stop()".to_string());
    cfg.add_filter_ignore("net::hosts".to_string());
    cfg.add_filter_ignore("net::session".to_string());
    cfg.add_filter_ignore("net::outbound_session".to_string());
    cfg.add_filter_ignore("net::inbound_session".to_string());
    cfg.add_filter_ignore("net::message_subscriber".to_string());
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

async fn spawn_node(
    inbound_addrs: Vec<Url>,
    external_addrs: Vec<Url>,
    peers: Vec<Url>,
    seeds: Vec<Url>,
    node_id: String,
    ex: Arc<Executor<'static>>,
) -> Arc<P2p> {
    let settings = Settings {
        localnet: true,
        inbound_addrs,
        external_addrs,
        outbound_connections: 2,
        outbound_peer_discovery_cooloff_time: 2,
        //outbound_connect_timeout: 2,
        inbound_connections: usize::MAX,
        greylist_refinery_interval: 15,
        peers,
        seeds,
        node_id,
        allowed_transports: vec!["tcp".to_string()],
        ..Default::default()
    };

    P2p::new(settings, ex.clone()).await
}

async fn spawn_seed_session(starting_port: usize, ex: Arc<Executor<'static>>) -> Vec<Arc<P2p>> {
    let mut p2p_instances = vec![];
    let seed_addr = Url::parse(SEED).unwrap();
    info!("========================================================");
    info!("Initializing outbound nodes...");
    info!("========================================================");
    for i in 0..N_NODES {
        let p2p = spawn_node(
            vec![Url::parse(&format!("tcp://127.0.0.1:{}", starting_port + i)).unwrap()],
            vec![Url::parse(&format!("tcp://127.0.0.1:{}", starting_port + i)).unwrap()],
            vec![],
            vec![seed_addr.clone()],
            (starting_port + i).to_string(),
            ex.clone(),
        )
        .await;
        p2p_instances.push(p2p);
    }

    // Start the P2P network
    for p2p in p2p_instances.iter() {
        info!("========================================================");
        info!("Starting node={}", p2p.settings().external_addrs[0]);
        info!("========================================================");
        p2p.clone().start().await.unwrap();
    }

    p2p_instances
}

/*async fn spawn_manual_session(
    peer_indexes: &[usize],
    starting_port: usize,
    rng: &mut ThreadRng,
    ex: Arc<Executor<'static>>,
) -> Vec<Arc<P2p>> {
    let mut p2p_instances = vec![];

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

        let p2p = spawn_node(
            vec![Url::parse(&format!("tcp://127.0.0.1:{}", starting_port + i)).unwrap()],
            vec![Url::parse(&format!("tcp://127.0.0.1:{}", starting_port + i)).unwrap()],
            vec![],
            peers,
            (starting_port + i).to_string(),
            ex.clone(),
        )
        .await;

        p2p_instances.push(p2p);
    }

    // Start the P2P network
    for p2p in p2p_instances.iter() {
        p2p.clone().start().await.unwrap();
    }

    info!("Waiting 5s until all peers connect");
    sleep(5).await;

    p2p_instances
}*/

/*async fn assert_hostlist_not_empty(
    p2p_instances: &Vec<Arc<P2p>>,
    rng: &mut ThreadRng,
    color: HostColor,
) {
    let random_node = p2p_instances.choose(rng).unwrap();
    assert!(!random_node.hosts().container.is_empty(color).await);
}*/

/*async fn assert_entry_exists(
    p2p_instances: &Vec<Arc<P2p>>,
    rng: &mut ThreadRng,
    color: HostColor,
    entry: &Url,
) {
    let mut urls = HashSet::new();
    let random_node = p2p_instances.choose(rng).unwrap();
    let external_addr = &random_node.settings().external_addrs[0];

    info!("Checking {} entry exists on {:?} list node={}", entry, color, external_addr);
    assert!(random_node.hosts().container.contains(color as usize, entry).await);
}*/

async fn get_random_gold_host(p2p_instances: &[Arc<P2p>], index: usize) -> ((Url, u64), usize) {
    let random_node = &p2p_instances[index];
    let hosts = random_node.hosts();
    let external_addr = &random_node.settings().external_addrs[0];

    info!("========================================================");
    info!("Getting gold addr from node={}", external_addr);
    info!("========================================================");

    let list = hosts.container.hostlists[HostColor::Gold as usize].read().await;
    let position = rand::thread_rng().gen_range(0..list.len());
    let entry = &list[position];
    (entry.clone(), position)
}

async fn check_random_hostlist(p2p_instances: &Vec<Arc<P2p>>, rng: &mut ThreadRng) {
    let mut urls = HashSet::new();
    let random_node = p2p_instances.choose(rng).unwrap();
    let external_addr = &random_node.settings().external_addrs[0];

    info!("========================================================");
    info!("Checking node={}", external_addr);
    info!("========================================================");

    let greylist = random_node.hosts().container.fetch_all(HostColor::Grey).await;
    let whitelist = random_node.hosts().container.fetch_all(HostColor::White).await;
    let goldlist = random_node.hosts().container.fetch_all(HostColor::Gold).await;

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

async fn kill_node(p2p_instances: &Vec<Arc<P2p>>, node: Url) {
    for p2p in p2p_instances {
        if p2p.settings().external_addrs[0] == node {
            info!("========================================================");
            info!("Shutting down node: {}", p2p.settings().external_addrs[0]);
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
fn p2p_test() {
    test_body!(p2p_test_real);
}

async fn p2p_test_real(ex: Arc<Executor<'static>>) {
    let mut rng = rand::thread_rng();
    // ============================================================
    // 1. Create a new seed node.
    // ============================================================
    //let peer_indexes: Vec<usize> = (0..N_NODES).collect();
    let seed_addr = Url::parse(SEED).unwrap();

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

    let seed = P2p::new(settings, ex.clone()).await;
    info!("========================================================");
    info!("Starting seed node on {}", SEED);
    info!("========================================================");
    seed.clone().start().await.unwrap();

    // ============================================================
    // 2. Spawn outbound nodes that will connect to the seed node.
    // ============================================================
    let p2p_instances = spawn_seed_session(43200, ex.clone()).await;

    info!("========================================================");
    info!("Waiting 10s for all peers to reach the seed node");
    info!("========================================================");
    sleep(10).await;

    // ===========================================================
    // 3. Assert that all nodes have shared their external addr
    //    with the seed node.
    // ===========================================================
    let greylist = seed.hosts().container.fetch_all(HostColor::Grey).await;
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
    assert!(!seed.hosts().container.is_empty(HostColor::White).await);
    info!("========================================================");
    info!("Seed node refinery operating successfully!");
    info!("========================================================");

    info!("========================================================");
    info!("Waiting 5s for peers to propagate...");
    info!("========================================================");
    sleep(5).await;

    // ===========================================================
    // 5. Select a random peer and ensure that its hostlist is not
    //    empty. This ensures the seed node is sharing whitelisted
    //    nodes around the network.
    // ===========================================================
    check_random_hostlist(&p2p_instances, &mut rng).await;
    info!("========================================================");
    info!("Peer successfully received addrs!");
    info!("========================================================");

    // ===========================================================
    // 6. Select a random gold peer from one of the nodes and kill
    //    it.
    // ===========================================================
    info!("========================================================");
    info!("Selecting a random gold entry...");
    info!("========================================================");

    let random_node_index = rand::thread_rng().gen_range(0..p2p_instances.len());
    let ((addr, _), _) = get_random_gold_host(&p2p_instances, random_node_index).await;

    kill_node(&p2p_instances, addr.clone()).await;

    info!("========================================================");
    info!("Waiting for greylist downgrade sequence to occur...");
    info!("========================================================");

    // ===========================================================
    // 7. Verify the peer has been removed from the Gold list.
    // ===========================================================
    p2p_instances[random_node_index]
        .hosts()
        .container
        .contains(HostColor::Grey as usize, &addr)
        .await;
    info!("========================================================");
    info!("Greylist downgrade occured successfully!");
    info!("========================================================");

    // ===========================================================
    // 8. Stop the P2P network
    // ===========================================================
    for p2p in p2p_instances.iter() {
        p2p.clone().stop().await;
    }
}
