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

use std::collections::HashMap;

use async_std::{
    fs,
    net::TcpListener,
    sync::{Arc, RwLock},
};
use darkfi::{
    dht2::{Dht, MAX_CHUNK_SIZE},
    net::{self, P2p},
    util::async_util::{msleep, sleep},
    system::StoppableTask,
    Error, Result,
};
use rand::{rngs::OsRng, RngCore};
use smol::Executor;
use url::Url;
use log::{error, warn};

use super::{proto::ProtocolDht, Dhtd};

#[allow(dead_code)]
async fn dht_remote_get_insert_real(ex: Arc<Executor<'_>>) -> Result<()> {
    const NET_SIZE: usize = 5;

    let mut dhtds = vec![];
    let mut base_path = std::env::temp_dir();
    base_path.push("dht");

    let mut addrs = vec![];
    for i in 0..NET_SIZE {
        // Find an available port
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let sockaddr = listener.local_addr()?;
        let url = Url::parse(&format!("tcp://127.0.0.1:{}", sockaddr.port()))?;
        drop(listener);

        let settings = net::Settings {
            inbound_addrs: vec![url.clone()],
            peers: addrs.clone(),
            allowed_transports: vec!["tcp".into()],
            localnet: true,
            ..Default::default()
        };

        addrs.push(url);

        let p2p = P2p::new(settings).await;
        let mut node_path = base_path.clone();
        node_path.push(format!("node_{}", i));
        let dht = Dht::new(&node_path.into(), p2p.clone()).await?;
        let dhtd = Arc::new(RwLock::new(Dhtd { dht, routing_table: HashMap::new() }));

        // Register P2P protocol
        let registry = p2p.protocol_registry();

        let _dhtd = dhtd.clone();
        registry
            .register(net::SESSION_NET, move |channel, p2p| {
                let dhtd = _dhtd.clone();
                async move { ProtocolDht::init(channel, p2p, dhtd).await.unwrap() }
            })
            .await;

        p2p.clone().start(ex.clone()).await?;
        StoppableTask::new().start(
            p2p.run(ex.clone()),
            |res| async {
                match res {
                    Ok(()) | Err(Error::P2PNetworkStopped) => { /* Do nothing */ }
                    Err(e) => error!("Failed starting P2P network: {}", e),
                }
            },
            Error::P2PNetworkStopped,
            ex,
        );

        dhtds.push(dhtd);

        sleep(1).await;
    }

    // Now the P2P network is set up. Try some stuff.
    for dhtd in dhtds.iter_mut() {
        dhtd.write().await.dht.garbage_collect().await?;
    }

    let dhtd = &mut dhtds[NET_SIZE - 1];
    let rng = &mut OsRng;
    let mut data = vec![0u8; MAX_CHUNK_SIZE];
    rng.fill_bytes(&mut data);
    let (file_hash, chunk_hashes) = dhtd.write().await.dht.insert(&data).await?;
    msleep(1000).await;

    for (i, node) in dhtds.iter().enumerate() {
        if i == NET_SIZE - 1 {
            continue
        }
        assert!(node.read().await.routing_table.contains_key(&file_hash));
    }

    let dhtd = &mut dhtds[NET_SIZE - 1];
    let mut chunk_path = dhtd.read().await.dht.chunks_path();
    chunk_path.push(chunk_hashes[0].to_hex().as_str());
    fs::remove_file(chunk_path).await?;
    dhtd.write().await.dht.garbage_collect().await?;
    msleep(1000).await;

    for (i, node) in dhtds.iter().enumerate() {
        if i == NET_SIZE - 1 {
            continue
        }

        let peers = node.read().await.routing_table.get(&file_hash).unwrap().clone();
        assert!(peers.is_empty());
    }

    fs::remove_dir_all(base_path).await?;

    Ok(())
}

#[test]
fn dht_remote_get_insert() -> Result<()> {
    let mut cfg = simplelog::ConfigBuilder::new();
    cfg.add_filter_ignore("net::protocol_version".to_string());
    cfg.add_filter_ignore("net::protocol_ping".to_string());

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

    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = async_std::channel::unbounded::<()>();

    easy_parallel::Parallel::new().each(0..4, |_| smol::block_on(ex.run(shutdown.recv()))).finish(
        || {
            smol::block_on(async {
                dht_remote_get_insert_real(ex.clone()).await.unwrap();
                drop(signal);
            })
        },
    );

    Ok(())
}
