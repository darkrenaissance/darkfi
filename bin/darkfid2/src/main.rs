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

use async_std::{stream::StreamExt, sync::Arc};
use log::info;
use structopt_toml::{serde::Deserialize, structopt::StructOpt, StructOptToml};
use url::Url;

use darkfi::{
    async_daemonize,
    blockchain::BlockInfo,
    cli_desc,
    net::{settings::SettingsOpt, P2p, P2pPtr, SESSION_ALL},
    rpc::server::listen_and_serve,
    util::time::TimeKeeper,
    validator::{proto::ProtocolTx, Validator, ValidatorConfig, ValidatorPtr},
    Result,
};
use darkfi_contract_test_harness::vks;

#[cfg(test)]
mod tests;

mod error;
use error::{server_error, RpcError};

/// JSON-RPC requests handler and methods
mod rpc;
mod rpc_blockchain;
mod rpc_tx;

/// Utility functions
mod utils;
use utils::genesis_txs_total;

const CONFIG_FILE: &str = "darkfid_config.toml";
const CONFIG_FILE_CONTENTS: &str = include_str!("../darkfid_config.toml");

#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[serde(default)]
#[structopt(name = "darkfid", about = cli_desc!())]
struct Args {
    #[structopt(short, long)]
    /// Configuration file to use
    config: Option<String>,

    #[structopt(long, default_value = "tcp://127.0.0.1:8340")]
    /// JSON-RPC listen URL
    rpc_listen: Url,

    #[structopt(long)]
    /// Participate in the consensus protocol
    consensus: bool,

    /// Syncing network settings
    #[structopt(flatten)]
    sync_net: SettingsOpt,

    /// Consensus network settings
    #[structopt(flatten)]
    consensus_net: SettingsOpt,

    #[structopt(long)]
    /// Enable testing mode for local testing
    testing_mode: bool,

    #[structopt(short, long)]
    /// Set log file to ouput into
    log: Option<String>,

    #[structopt(short, parse(from_occurrences))]
    /// Increase verbosity (-vvv supported)
    verbose: u8,
}

pub struct Darkfid {
    sync_p2p: P2pPtr,
    consensus_p2p: Option<P2pPtr>,
    validator: ValidatorPtr,
}

impl Darkfid {
    pub async fn new(
        sync_p2p: P2pPtr,
        consensus_p2p: Option<P2pPtr>,
        validator: ValidatorPtr,
    ) -> Self {
        Self { sync_p2p, consensus_p2p, validator }
    }
}

async_daemonize!(realmain);
async fn realmain(args: Args, ex: Arc<smol::Executor<'_>>) -> Result<()> {
    info!(target: "darkfid", "Initializing DarkFi node...");

    if args.testing_mode {
        info!(target: "darkfid", "Node is configured to run in testing mode!");
    }

    // NOTE: everything is dummy for now
    // Initialize or open sled database
    let sled_db = sled::Config::new().temporary(true).open()?;
    vks::inject(&sled_db)?;

    // Initialize validator configuration
    let genesis_block = BlockInfo::default();
    let genesis_txs_total = genesis_txs_total(&genesis_block.txs)?;
    let time_keeper = TimeKeeper::new(genesis_block.header.timestamp, 10, 90, 0);
    let config = ValidatorConfig::new(
        time_keeper,
        genesis_block,
        genesis_txs_total,
        vec![],
        args.testing_mode,
    );

    // Initialize validator
    let validator = Validator::new(&sled_db, config).await?;

    // Initialize syncing P2P network
    let sync_p2p = {
        info!("Registering sync network P2P protocols...");
        let p2p = P2p::new(args.sync_net.into()).await;
        let registry = p2p.protocol_registry();

        let _validator = validator.clone();
        registry
            .register(SESSION_ALL, move |channel, p2p| {
                let validator = _validator.clone();
                async move { ProtocolTx::init(channel, validator, p2p).await.unwrap() }
            })
            .await;

        p2p
    };

    // Initialize consensus P2P network
    let consensus_p2p = {
        if !args.consensus {
            None
        } else {
            Some(P2p::new(args.consensus_net.into()).await)
        }
    };

    // Initialize node
    let darkfid = Darkfid::new(sync_p2p, consensus_p2p, validator).await;
    let darkfid = Arc::new(darkfid);
    info!(target: "darkfid", "Node initialized successfully!");

    // JSON-RPC server
    info!(target: "darkfid", "Starting JSON-RPC server");
    let _ex = ex.clone();
    ex.spawn(listen_and_serve(args.rpc_listen, darkfid.clone(), _ex)).detach();

    // Simulate that we have synced
    darkfid.validator.write().await.synced = true;

    // Signal handling for graceful termination.
    let (signals_handler, signals_task) = SignalHandler::new()?;
    signals_handler.wait_termination(signals_task).await?;
    info!(target: "darkfid", "Caught termination signal, cleaning up and exiting...");

    info!(target: "darkfid", "Stopping syncing P2P network...");
    darkfid.sync_p2p.stop().await;

    if args.consensus {
        info!(target: "darkfid", "Stopping consensus P2P network...");
        darkfid.consensus_p2p.clone().unwrap().stop().await;
    }

    info!(target: "darkfid", "Flushing sled database...");
    let flushed_bytes = sled_db.flush_async().await?;
    info!(target: "darkfid", "Flushed {} bytes", flushed_bytes);

    Ok(())
}
