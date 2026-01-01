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

// cargo test --release --features=net --lib p2p -- --include-ignored

use std::{
    collections::{HashMap, HashSet},
    net::TcpListener,
    panic,
    sync::Arc,
};

use darkfi_serial::{async_trait, SerialDecodable, SerialEncodable};
use rand::{prelude::SliceRandom, rngs::ThreadRng, Rng};
use smol::{channel, future, Executor};
use tracing::{error, info, warn};
use url::Url;

use crate::{
    net::{
        hosts::HostColor,
        message::{GetAddrsMessage, Message},
        metering::{MeteringConfiguration, DEFAULT_METERING_CONFIGURATION},
        settings::NetworkProfile,
        P2p, Settings,
    },
    system::sleep,
    util::logger::{setup_test_logger, Level},
};

fn init_logger() {
    let ignored_targets = [
        "sled",
        "net::protocol_ping",
        "net::channel::subscribe_stop()",
        "net::hosts",
        "net::session",
        "net::outbound_session",
        "net::inbound_session",
        "net::message_publisher",
        "net::protocol_address",
        "net::protocol_version",
        "net::protocol_registry",
        "net::protocol_jobs_manager",
        "net::channel::send()",
        "net::channel::start()",
        "net::channel::handle_stop()",
        "net::channel::subscribe_msg()",
        "net::channel::main_receive_loop()",
        "net::tcp",
    ];
    // We check this error so we can execute same file tests in parallel,
    // otherwise second one fails to init logger here.
    if setup_test_logger(
        &ignored_targets,
        false,
        //Level::Info,
        Level::Verbose,
        //Level::Debug,
        //Level::Trace,
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

fn get_unique_ports(n_nodes: usize) -> Vec<usize> {
    let mut ports = HashSet::new();

    while ports.len() < n_nodes {
        ports.insert(get_random_available_port());
    }

    ports.into_iter().collect()
}

async fn spawn_seed_session(
    seed_addr: Url,
    ex: Arc<Executor<'static>>,
    n_nodes: usize,
) -> Vec<Arc<P2p>> {
    info!("========================================================");
    info!("Initializing outbound nodes...");
    info!("========================================================");

    let mut outbound_instances = vec![];
    let ports = get_unique_ports(n_nodes);

    let mut profiles = HashMap::new();
    profiles.insert(
        "tcp".to_string(),
        NetworkProfile { outbound_connect_timeout: 2, ..Default::default() },
    );

    for port in ports {
        let settings = Settings {
            localnet: true,
            inbound_addrs: vec![Url::parse(&format!("tcp://127.0.0.1:{port}")).unwrap()],
            external_addrs: vec![Url::parse(&format!("tcp://127.0.0.1:{port}")).unwrap()],
            outbound_connections: 2,
            outbound_peer_discovery_cooloff_time: 2,
            inbound_connections: usize::MAX,
            greylist_refinery_interval: 15,
            peers: vec![],
            seeds: vec![seed_addr.clone()],
            node_id: (port).to_string(),
            active_profiles: vec!["tcp".to_string()],
            profiles: profiles.clone(),
            ..Default::default()
        };

        let p2p = P2p::new(settings, ex.clone()).await.unwrap();
        outbound_instances.push(p2p);
    }

    outbound_instances
}

async fn spawn_manual_session(
    ex: Arc<Executor<'static>>,
    n_nodes: usize,
    n_conns: usize,
) -> Vec<Arc<P2p>> {
    info!("========================================================");
    info!("Initializing manual nodes...");
    info!("========================================================");
    let mut manual_instances = vec![];
    let mut rng = rand::thread_rng();
    let ports = get_unique_ports(n_nodes);

    let mut profiles = HashMap::new();
    profiles.insert(
        "tcp".to_string(),
        NetworkProfile { outbound_connect_timeout: 2, ..Default::default() },
    );

    for i in 0..n_nodes {
        let mut peer_indexes_copy: Vec<usize> = (0..n_nodes).collect();
        peer_indexes_copy.remove(i);
        let peer_indexes_to_connect: Vec<_> =
            peer_indexes_copy.choose_multiple(&mut rng, n_conns).collect();

        let mut peers = vec![];
        for &peer_index in peer_indexes_to_connect {
            let port = ports[peer_index];
            peers.push(Url::parse(&format!("tcp://127.0.0.1:{port}")).unwrap());
        }

        let inbound_port = ports[i];
        let settings = Settings {
            localnet: true,
            inbound_addrs: vec![Url::parse(&format!("tcp://127.0.0.1:{inbound_port}")).unwrap()],
            external_addrs: vec![Url::parse(&format!("tcp://127.0.0.1:{inbound_port}")).unwrap()],
            outbound_connections: 2,
            outbound_peer_discovery_cooloff_time: 2,
            inbound_connections: usize::MAX,
            greylist_refinery_interval: 15,
            peers,
            seeds: vec![],
            node_id: inbound_port.to_string(),
            active_profiles: vec!["tcp".to_string()],
            profiles: profiles.clone(),
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
    info!("Getting gold addr from node={external_addr}");
    info!("========================================================");

    let list = hosts.container.hostlists[HostColor::Gold as usize].read().unwrap();
    assert!(!list.is_empty());
    let position = rand::thread_rng().gen_range(0..list.len());
    let entry = &list[position];
    (entry.clone(), position)
}

async fn _check_random_hostlist(outbound_instances: &[Arc<P2p>], rng: &mut ThreadRng) {
    let mut urls = HashSet::new();
    let random_node = outbound_instances.choose(rng).unwrap();
    let external_addr = random_node.settings().read().await.external_addrs[0].clone();

    info!("========================================================");
    info!("Checking node={external_addr}");
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
        info!("Checking node={external_addr}");
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
    ($real_call:ident, $threads:expr) => {
        init_logger();

        let ex = Arc::new(Executor::new());
        let ex_ = ex.clone();
        let (signal, shutdown) = channel::unbounded::<()>();

        panic::set_hook(Box::new(|panic_info| {
            error!("Panic occurred: {:?}", panic_info);
        }));

        // Run a thread for each node.
        easy_parallel::Parallel::new()
            .each(0..$threads, |_| {
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
    test_body!(p2p_test_real, 5);
}

async fn p2p_test_real(ex: Arc<Executor<'static>>) {
    // Number of nodes to spawn and number of peers each node connects to
    const N_NODES: usize = 5;
    const N_CONNS: usize = 4;
    // ============================================================
    // 1. Create a new seed node.
    // ============================================================
    let seed_port = get_random_available_port();
    let seed_addr = Url::parse(&format!("tcp://127.0.0.1:{seed_port}")).unwrap();

    let settings = Settings {
        localnet: true,
        inbound_addrs: vec![seed_addr.clone()],
        outbound_connections: 0,
        inbound_connections: usize::MAX,
        seeds: vec![],
        peers: vec![],
        active_profiles: vec!["tcp".to_string()],
        greylist_refinery_interval: 12,
        node_id: "seed".to_string(),
        ..Default::default()
    };

    let seed = P2p::new(settings, ex.clone()).await.unwrap();
    info!("========================================================");
    info!("Starting seed node on {seed_addr}");
    info!("========================================================");
    seed.clone().start().await.unwrap();

    // ============================================================
    // 2. Spawn outbound nodes that will connect to the seed node.
    // ============================================================
    let outbound_instances = spawn_seed_session(seed_addr, ex.clone(), N_NODES).await;

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
        info!("Found grey url: {url}");
        assert!(urls.insert(url));
    }
    for (url, _) in whitelist {
        info!("Found white url: {url}");
        assert!(urls.insert(url));
    }
    for (url, _) in goldlist {
        info!("Found gold url: {url}");
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

    let manual_instances = spawn_manual_session(ex.clone(), N_NODES, N_CONNS).await;

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
        let peers = p2p.hosts().peers();
        assert!(peers.len() == N_CONNS * 2);
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

#[test]
fn p2p_channel_unsupported_message_type_gets_banned() {
    test_body!(p2p_channel_unsupported_message_type_gets_banned_real, 2);
}

async fn p2p_channel_unsupported_message_type_gets_banned_real(ex: Arc<Executor<'static>>) {
    // Test with two nodes directly connected to each other
    let manual_instances = spawn_manual_session(ex.clone(), 2, 1).await;
    for p2p in &manual_instances {
        p2p.clone().start().await.unwrap();
    }

    // Let's wait for the nodes to connect to each other
    sleep(5).await;

    let node1_p2p = manual_instances[0].clone();
    let node2_p2p = manual_instances[1].clone();
    let channel = node1_p2p.hosts().channels().first().unwrap().clone();

    // Create a new message type
    #[derive(SerialEncodable, SerialDecodable)]
    struct CustomMessage(u32);
    crate::impl_p2p_message!(
        CustomMessage,
        "UnsupportedMessage",
        0,
        0,
        DEFAULT_METERING_CONFIGURATION
    );

    let instance = CustomMessage(23);
    channel.send(&instance).await.unwrap();
    sleep(1).await;

    // Node1 should be banned by Node2
    assert_eq!(node2_p2p.hosts().container.fetch_all(HostColor::Black).len(), 1);
    node1_p2p.stop().await;
    node2_p2p.stop().await;
}

#[test]
fn p2p_channel_invalid_command_length_gets_banned() {
    test_body!(p2p_channel_invalid_command_length_gets_banned_real, 2);
}

async fn p2p_channel_invalid_command_length_gets_banned_real(ex: Arc<Executor<'static>>) {
    // Test with two nodes directly connected to each other
    let manual_instances = spawn_manual_session(ex.clone(), 2, 1).await;
    for p2p in &manual_instances {
        p2p.clone().start().await.unwrap();
    }

    // Let's wait for the nodes to connect to each other
    sleep(5).await;

    let node1_p2p = manual_instances[0].clone();
    let node2_p2p = manual_instances[1].clone();
    let channel = node1_p2p.hosts().channels().first().unwrap().clone();

    // Create a custom message that has invalid length command name
    #[derive(SerialEncodable, SerialDecodable)]
    struct CustomMessage(u32);
    // The length of COMMAND_NAME is greater than message::MAX_COMMAND_LENGTH, this one is 256
    const COMMAND_NAME: &str =
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\
    AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\
    AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

    crate::impl_p2p_message!(CustomMessage, &COMMAND_NAME, 0, 0, DEFAULT_METERING_CONFIGURATION);

    let instance = CustomMessage(23);
    channel.send(&instance).await.unwrap();
    sleep(1).await;

    // Node1 should be banned by Node2
    assert_eq!(node2_p2p.hosts().container.fetch_all(HostColor::Black).len(), 1);
    node1_p2p.stop().await;
    node2_p2p.stop().await;
}

#[test]
fn p2p_channel_invalid_message_length_gets_banned() {
    test_body!(p2p_channel_invalid_message_length_gets_banned_real, 2);
}

async fn p2p_channel_invalid_message_length_gets_banned_real(ex: Arc<Executor<'static>>) {
    // Test with two nodes directly connected to each other
    let manual_instances = spawn_manual_session(ex.clone(), 2, 1).await;
    for p2p in &manual_instances {
        p2p.clone().start().await.unwrap();
    }

    // Let's wait for the nodes to connect to each other
    sleep(5).await;

    let node1_p2p = manual_instances[0].clone();
    let node2_p2p = manual_instances[1].clone();
    let channel = node1_p2p.hosts().channels().first().unwrap().clone();

    // Let's create a GetAddrsMessage that will be over the GET_ADDRS_MAX_BYTES threshold
    let message = GetAddrsMessage { max: 20, transports: vec!["tor".to_string(); 256] };
    channel.send(&message).await.unwrap();
    sleep(1).await;

    // Node1 should be banned by Node2
    assert_eq!(node2_p2p.hosts().container.fetch_all(HostColor::Black).len(), 1);
    node1_p2p.stop().await;
    node2_p2p.stop().await;
}
