/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2022 Dyne.org foundation
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

use std::sync::Arc;

use clap::Parser;
use easy_parallel::Parallel;
use log::info;
use smol::Executor;
use url::Url;

use darkfi::{
    consensus::{
        constants::{TESTNET_GENESIS_HASH_BYTES, TESTNET_GENESIS_TIMESTAMP},
        ouroboros::{EpochConsensus, Stakeholder},
        proto::{ProtocolSync, ProtocolTx},
        ValidatorState,
    },
    net,
    net::Settings,
    util::{path::expand_path, time::Timestamp},
    wallet::walletdb::init_wallet,
    Result,
};

#[derive(Parser)]
struct NetCli {
    #[clap(long, value_parser)]
    addr: Vec<String>,
    #[clap(long, value_parser, default_value = "/tmp/db")]
    path: String,
    #[clap(long, value_parser)]
    peers: Vec<String>,
    #[clap(long, value_parser)]
    seeds: Vec<String>,
    #[clap(long, value_parser, default_value = "0")]
    slots: u32,
    #[clap(long, value_parser)]
    wallet_path: String,
    #[clap(long, value_parser)]
    wallet_pass: String,
}

#[async_std::main]
async fn main() -> Result<()> {
    env_logger::init();
    let args = NetCli::parse();

    let (signal, shutdown) = smol::channel::unbounded::<()>();

    let ex = Arc::new(Executor::new());
    let ex2 = ex.clone();
    let ex3 = ex2.clone();

    let (_, result) = Parallel::new()
        .each(0..4, |_| smol::future::block_on(ex2.run(shutdown.recv())))
        .finish(|| {
            smol::future::block_on(async move {
                start(args, ex3).await?;
                drop(signal);
                Ok(())
            })
        });

    result
}

async fn start(args: NetCli, ex: Arc<Executor<'_>>) -> Result<()> {
    let mut addr = vec![];
    for i in 0..args.addr.len() {
        addr.push(Url::parse(args.addr[i].as_str()).unwrap());
    }

    let mut peers = vec![];
    for i in 0..args.peers.len() {
        peers.push(Url::parse(args.peers[i].as_str()).unwrap());
    }

    let mut seeds = vec![];
    for i in 0..args.seeds.len() {
        seeds.push(Url::parse(args.seeds[i].as_str()).unwrap());
    }

    // initialize n stakeholders
    let settings = Settings {
        inbound: addr.clone(),
        outbound_connections: args.slots,
        manual_attempt_limit: 0,
        seed_query_timeout_seconds: 8,
        connect_timeout_seconds: 10,
        channel_handshake_seconds: 4,
        channel_heartbeat_seconds: 10,
        external_addr: addr,
        peers,
        seeds,
        ..Default::default()
    };

    let p2p = net::P2p::new(settings.clone()).await;

    //////////////////////////////

    // Initialize or load wallet
    let wallet = init_wallet(&args.wallet_path, &args.wallet_pass).await?;

    // Initialize or open sled database
    let db_path = format!("{}/{}", expand_path(&args.path)?.to_str().unwrap(), "testnet");
    let sled_db = sled::open(&db_path)?;

    // Initialize validator state
    let (genesis_ts, genesis_data) = (*TESTNET_GENESIS_TIMESTAMP, *TESTNET_GENESIS_HASH_BYTES);

    // Parse faucet addresses (not needed here probably)
    let faucet_pubkeys = vec![];

    // Initialize validator state
    let state =
        ValidatorState::new(&sled_db, genesis_ts, genesis_data, wallet.clone(), faucet_pubkeys)
            .await?;

    let registry = p2p.protocol_registry();

    info!("Registering block sync P2P protocols...");
    let _state = state.clone();
    registry
        .register(net::SESSION_ALL, move |channel, p2p| {
            let state = _state.clone();
            async move { ProtocolSync::init(channel, state, p2p, false).await.unwrap() }
        })
        .await;

    let _state = state.clone();
    registry
        .register(net::SESSION_ALL, move |channel, p2p| {
            let state = _state.clone();
            async move { ProtocolTx::init(channel, state, p2p).await.unwrap() }
        })
        .await;

    //////////////////////////////

    let ex2 = ex.clone();

    p2p.clone().start(ex.clone()).await?;
    ex2.spawn(p2p.clone().run(ex.clone())).detach();

    let slots = 3;
    let epochs = 3;
    let ticks = 3;
    let reward = 1;
    let epoch_consensus = EpochConsensus::new(Some(slots), Some(epochs), Some(ticks), Some(reward));

    //proof's number of rows
    let k: u32 = 11;
    let path = args.path.clone();
    let id = Timestamp::current_time().0;

    let mut stakeholder =
        Stakeholder::new(epoch_consensus, p2p.clone(), settings.to_owned(), &path, id, Some(k))
            .await?;

    stakeholder.background(Some(100)).await;

    p2p.stop().await;

    Ok(())
}
