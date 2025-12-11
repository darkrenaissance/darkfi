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

use std::{collections::HashMap, str::FromStr};

use smol::{fs::read_to_string, stream::StreamExt};
use structopt_toml::{serde::Deserialize, structopt::StructOpt, StructOptToml};
use tracing::{debug, error, info};
use url::Url;

use darkfi::{
    async_daemonize, cli_desc, rpc::util::JsonValue, system::ExecutorPtr,
    util::path::get_config_path, Error, Result,
};
use darkfi_sdk::{
    crypto::{pasta_prelude::PrimeField, FuncId, PublicKey},
    pasta::pallas,
};

use minerd::{benchmark::benchmark, MinerNodeConfig, Minerd};

const CONFIG_FILE: &str = "minerd.toml";
const CONFIG_FILE_CONTENTS: &str = include_str!("../minerd.toml");

#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[serde(default)]
#[structopt(name = "minerd", about = cli_desc!())]
struct Args {
    #[structopt(short, long)]
    /// Configuration file to use
    config: Option<String>,

    #[structopt(short, long)]
    /// Number of nonces to execute in system hashrate benchmark
    bench: Option<u64>,

    #[structopt(long)]
    /// Flag indicating whether to run miner in light mode
    light_mode: bool,

    #[structopt(long)]
    /// Flag indicating whether to run miner with Large Pages
    large_pages: bool,

    #[structopt(long)]
    /// Flag indicating whether to run miner with secure access to JIT memory (if supported)
    secure: bool,

    #[structopt(short, long, default_value = "4")]
    /// PoW miner number of threads to use
    threads: usize,

    #[structopt(short, long, default_value = "2")]
    /// Polling rate to ask darkfid for mining jobs
    polling_rate: u64,

    #[structopt(long, default_value = "0")]
    /// Stop mining at given height (0 mines forever)
    stop_at_height: u32,

    #[structopt(short, long, default_value = "testnet")]
    /// Blockchain network to use
    network: String,

    #[structopt(short, long)]
    /// Set log file to ouput into
    log: Option<String>,

    #[structopt(short, parse(from_occurrences))]
    /// Increase verbosity (-vvv supported)
    verbose: u8,
}

#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[structopt()]
/// Defines a blockchain network configuration.
/// Default values correspond to a local network.
pub struct BlockchainNetwork {
    #[structopt(short, long, default_value = "tcp://127.0.0.1:8240")]
    /// darkfid JSON-RPC endpoint
    endpoint: Url,

    #[structopt(long, default_value = "")]
    /// Wallet mining address to receive mining rewards
    recipient: String,

    #[structopt(long)]
    /// Optional contract spend hook to use in the mining reward
    spend_hook: Option<String>,

    #[structopt(long)]
    /// Optional contract user data to use in the mining reward.
    /// This is not arbitrary data.
    user_data: Option<String>,
}

/// Auxiliary function to parse minerd configuration file and extract
/// requested blockchain network config.
pub async fn parse_blockchain_config(
    config: Option<String>,
    network: &str,
) -> Result<BlockchainNetwork> {
    // Grab config path
    let config_path = get_config_path(config, CONFIG_FILE)?;
    debug!(target: "minerd", "Parsing configuration file: {config_path:?}");

    // Parse TOML file contents
    let contents = read_to_string(&config_path).await?;
    let contents: toml::Value = match toml::from_str(&contents) {
        Ok(v) => v,
        Err(e) => {
            error!(target: "minerd", "Failed parsing TOML config: {e}");
            return Err(Error::ParseFailed("Failed parsing TOML config"))
        }
    };

    // Grab requested network config
    let Some(table) = contents.as_table() else { return Err(Error::ParseFailed("TOML not a map")) };
    let Some(network_configs) = table.get("network_config") else {
        return Err(Error::ParseFailed("TOML does not contain network configurations"))
    };
    let Some(network_configs) = network_configs.as_table() else {
        return Err(Error::ParseFailed("`network_config` not a map"))
    };
    let Some(network_config) = network_configs.get(network) else {
        return Err(Error::ParseFailed("TOML does not contain requested network configuration"))
    };
    let network_config = toml::to_string(&network_config).unwrap();
    let network_config =
        match BlockchainNetwork::from_iter_with_toml::<Vec<String>>(&network_config, vec![]) {
            Ok(v) => v,
            Err(e) => {
                error!(target: "minerd", "Failed parsing requested network configuration: {e}");
                return Err(Error::ParseFailed("Failed parsing requested network configuration"))
            }
        };
    debug!(target: "minerd", "Parsed network configuration: {network_config:?}");

    Ok(network_config)
}

async_daemonize!(realmain);
async fn realmain(args: Args, ex: ExecutorPtr) -> Result<()> {
    // Run system hashrate benchmark if requested
    if let Some(nonces) = args.bench {
        return benchmark(!args.light_mode, args.large_pages, args.secure, args.threads, nonces)
    }

    info!(target: "minerd", "Starting DarkFi Mining Daemon...");

    // Grab blockchain network configuration
    let blockchain_config = match args.network.as_str() {
        "localnet" => parse_blockchain_config(args.config, "localnet").await?,
        "testnet" => parse_blockchain_config(args.config, "testnet").await?,
        "mainnet" => parse_blockchain_config(args.config, "mainnet").await?,
        _ => {
            error!(target: "minerd", "Unsupported chain `{}`", args.network);
            return Err(Error::UnsupportedChain)
        }
    };
    debug!(target: "minerd", "Blockchain config: {blockchain_config:?}");

    // Parse the network wallet configuration
    if PublicKey::from_str(&blockchain_config.recipient).is_err() {
        return Err(Error::InvalidAddress)
    }
    let mut wallet_config = HashMap::from([(
        String::from("recipient"),
        JsonValue::String(blockchain_config.recipient),
    )]);

    if let Some(spend_hook) = &blockchain_config.spend_hook {
        if FuncId::from_str(spend_hook).is_err() {
            return Err(Error::ParseFailed("Invalid spend hook"))
        }
        wallet_config.insert(String::from("spend_hook"), JsonValue::String(spend_hook.to_string()));
    }

    if let Some(user_data_string) = &blockchain_config.user_data {
        let bytes: [u8; 32] = match bs58::decode(&user_data_string).into_vec()?.try_into() {
            Ok(b) => b,
            Err(_) => return Err(Error::ParseFailed("Invalid user data")),
        };
        let user_data: Option<pallas::Base> = pallas::Base::from_repr(bytes).into();
        if user_data.is_none() {
            return Err(Error::ParseFailed("Invalid user data"))
        }
        wallet_config
            .insert(String::from("user_data"), JsonValue::String(user_data_string.to_string()));
    }

    // Generate the daemon
    let miner_config = MinerNodeConfig::new(
        !args.light_mode,
        args.large_pages,
        args.secure,
        args.threads,
        args.polling_rate,
        args.stop_at_height,
        wallet_config,
    );
    let daemon = Minerd::init(miner_config, blockchain_config.endpoint, &ex).await;
    daemon.start(&ex);

    // Signal handling for graceful termination.
    let (signals_handler, signals_task) = SignalHandler::new(ex)?;
    signals_handler.wait_termination(signals_task).await?;
    info!(target: "minerd", "Caught termination signal, cleaning up and exiting");

    daemon.stop().await;

    info!(target: "minerd", "Shut down successfully");
    Ok(())
}
