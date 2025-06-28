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

use std::sync::Arc;

use log::{debug, error, info};
use smol::{fs::read_to_string, stream::StreamExt};
use structopt_toml::{serde::Deserialize, structopt::StructOpt, StructOptToml};
use url::Url;

use darkfi::{
    async_daemonize,
    blockchain::BlockInfo,
    cli_desc,
    net::settings::SettingsOpt,
    rpc::settings::RpcSettingsOpt,
    util::{
        encoding::base64,
        path::{expand_path, get_config_path},
    },
    validator::{Validator, ValidatorConfig},
    Error, Result,
};
use darkfi_serial::deserialize_async;

use darkfid::{task::consensus::ConsensusInitTaskConfig, Darkfid};

const CONFIG_FILE: &str = "darkfid_config.toml";
const CONFIG_FILE_CONTENTS: &str = include_str!("../darkfid_config.toml");
/// Note:
/// If you change these don't forget to remove their corresponding database folder,
/// since if it already has a genesis block, provided one is ignored.
const GENESIS_BLOCK_LOCALNET: &str = include_str!("../genesis_block_localnet");
const GENESIS_BLOCK_TESTNET: &str = include_str!("../genesis_block_testnet");
const GENESIS_BLOCK_MAINNET: &str = include_str!("../genesis_block_mainnet");

#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[serde(default)]
#[structopt(name = "darkfid", about = cli_desc!())]
struct Args {
    #[structopt(short, long)]
    /// Configuration file to use
    config: Option<String>,

    #[structopt(short, long, default_value = "testnet")]
    /// Blockchain network to use
    network: String,

    #[structopt(short, long)]
    /// Reset validator state to given block height
    reset: Option<u32>,

    #[structopt(short, long)]
    /// Purge pending sync headers
    purge_sync: bool,

    #[structopt(long)]
    /// Fully validates existing blockchain state
    validate: bool,

    #[structopt(long)]
    /// Fully rebuild the difficulties database based on existing blockchain state
    rebuild_difficulties: bool,

    #[structopt(short, long)]
    /// Set log file to ouput into
    log: Option<String>,

    #[structopt(short, parse(from_occurrences))]
    /// Increase verbosity (-vvv supported)
    verbose: u8,
}

#[derive(Clone, Debug, serde::Deserialize, structopt::StructOpt, structopt_toml::StructOptToml)]
#[structopt()]
/// Defines a blockchain network configuration.
/// Default values correspond to a local network.
pub struct BlockchainNetwork {
    #[structopt(long, default_value = "~/.local/share/darkfi/darkfid/localnet")]
    /// Path to blockchain database
    database: String,

    #[structopt(long, default_value = "3")]
    /// Confirmation threshold, denominated by number of blocks
    threshold: usize,

    #[structopt(long)]
    /// minerd JSON-RPC endpoint
    minerd_endpoint: Option<Url>,

    #[structopt(skip)]
    /// Optional JSON-RPC settings for p2pool merge mining requests
    mm_rpc: Option<RpcSettingsOpt>,

    #[structopt(long, default_value = "120")]
    /// PoW block production target, in seconds
    pow_target: u32,

    #[structopt(long)]
    /// Optional fixed PoW difficulty, used for testing
    pow_fixed_difficulty: Option<usize>,

    #[structopt(long)]
    /// Wallet address to receive mining rewards
    recipient: Option<String>,

    #[structopt(long)]
    /// Optional contract spend hook to use in the mining reward
    spend_hook: Option<String>,

    #[structopt(long)]
    /// Optional contract user data to use in the mining reward.
    /// This is not arbitrary data.
    user_data: Option<String>,

    #[structopt(long)]
    /// Skip syncing process and start node right away
    skip_sync: bool,

    #[structopt(long)]
    /// Disable transaction's fee verification, used for testing
    skip_fees: bool,

    #[structopt(long)]
    /// Optional sync checkpoint height
    checkpoint_height: Option<u32>,

    #[structopt(long)]
    /// Optional sync checkpoint hash
    checkpoint: Option<String>,

    #[structopt(long)]
    /// Optional bootstrap timestamp
    bootstrap: Option<u64>,

    #[structopt(long)]
    /// Garbage collection task transactions batch size
    txs_batch_size: Option<usize>,

    #[structopt(flatten)]
    /// P2P network settings
    net: SettingsOpt,

    #[structopt(flatten)]
    /// JSON-RPC settings
    rpc: RpcSettingsOpt,
}

async_daemonize!(realmain);
async fn realmain(args: Args, ex: Arc<smol::Executor<'static>>) -> Result<()> {
    info!(target: "darkfid", "Initializing DarkFi node...");

    // Grab blockchain network configuration
    let (blockchain_config, genesis_block) = match args.network.as_str() {
        "localnet" => {
            (parse_blockchain_config(args.config, "localnet").await?, GENESIS_BLOCK_LOCALNET)
        }
        "testnet" => {
            (parse_blockchain_config(args.config, "testnet").await?, GENESIS_BLOCK_TESTNET)
        }
        "mainnet" => {
            (parse_blockchain_config(args.config, "mainnet").await?, GENESIS_BLOCK_MAINNET)
        }
        _ => {
            error!("Unsupported chain `{}`", args.network);
            return Err(Error::UnsupportedChain)
        }
    };

    // Parse the genesis block
    let bytes = base64::decode(genesis_block.trim()).unwrap();
    let genesis_block: BlockInfo = deserialize_async(&bytes).await?;

    // Compute the bootstrap timestamp
    let bootstrap = match blockchain_config.bootstrap {
        Some(b) => b,
        None => genesis_block.header.timestamp.inner(),
    };

    // Initialize or open sled database
    let db_path = expand_path(&blockchain_config.database)?;
    let sled_db = sled_overlay::sled::open(&db_path)?;

    // Initialize validator configuration
    let pow_fixed_difficulty = if let Some(diff) = blockchain_config.pow_fixed_difficulty {
        info!(target: "darkfid", "Node is configured to run with fixed PoW difficulty: {diff}");
        Some(diff.into())
    } else {
        None
    };

    let config = ValidatorConfig {
        confirmation_threshold: blockchain_config.threshold,
        pow_target: blockchain_config.pow_target,
        pow_fixed_difficulty,
        genesis_block,
        verify_fees: !blockchain_config.skip_fees,
    };

    // Check if reset was requested
    if let Some(height) = args.reset {
        info!(target: "darkfid", "Node will reset validator state to height: {height}");
        let validator = Validator::new(&sled_db, &config).await?;
        validator.reset_to_height(height).await?;
        info!(target: "darkfid", "Validator state reset successfully!");
        return Ok(())
    }

    // Check if sync headers purge was requested
    if args.purge_sync {
        info!(target: "darkfid", "Node will purge all pending sync headers.");
        let validator = Validator::new(&sled_db, &config).await?;
        validator.blockchain.headers.remove_all_sync()?;
        info!(target: "darkfid", "Validator pending sync headers purged successfully!");
        return Ok(())
    }

    // Check if validate was requested
    if args.validate {
        info!(target: "darkfid", "Node will validate existing blockchain state.");
        let validator = Validator::new(&sled_db, &config).await?;
        validator.validate_blockchain(config.pow_target, config.pow_fixed_difficulty).await?;
        info!(target: "darkfid", "Validator blockchain state validated successfully!");
        return Ok(())
    }

    // Check if rebuild difficulties was requested
    if args.rebuild_difficulties {
        info!(target: "darkfid", "Node will rebuild difficulties of existing blockchain state.");
        let validator = Validator::new(&sled_db, &config).await?;
        validator
            .rebuild_block_difficulties(config.pow_target, config.pow_fixed_difficulty)
            .await?;
        info!(target: "darkfid", "Validator difficulties rebuilt successfully!");
        return Ok(())
    }

    // Generate the daemon
    let daemon = Darkfid::init(
        &sled_db,
        &config,
        &blockchain_config.net.into(),
        &blockchain_config.minerd_endpoint,
        &blockchain_config.txs_batch_size,
        &ex,
    )
    .await?;

    // Start the daemon
    let config = ConsensusInitTaskConfig {
        skip_sync: blockchain_config.skip_sync,
        checkpoint_height: blockchain_config.checkpoint_height,
        checkpoint: blockchain_config.checkpoint,
        miner: blockchain_config.minerd_endpoint.is_some(),
        recipient: blockchain_config.recipient,
        spend_hook: blockchain_config.spend_hook,
        user_data: blockchain_config.user_data,
        bootstrap,
    };
    daemon
        .start(
            &ex,
            &blockchain_config.rpc.into(),
            &blockchain_config.mm_rpc.map(|mm_rpc_opts| mm_rpc_opts.into()),
            &config,
        )
        .await?;

    // Signal handling for graceful termination.
    let (signals_handler, signals_task) = SignalHandler::new(ex)?;
    signals_handler.wait_termination(signals_task).await?;
    info!(target: "darkfid", "Caught termination signal, cleaning up and exiting...");

    daemon.stop().await?;

    info!(target: "darkfid", "Shut down successfully");

    Ok(())
}

/// Auxiliary function to parse darkfid configuration file and extract requested
/// blockchain network config.
pub async fn parse_blockchain_config(
    config: Option<String>,
    network: &str,
) -> Result<BlockchainNetwork> {
    // Grab config path
    let config_path = get_config_path(config, CONFIG_FILE)?;
    debug!(target: "darkfid", "Parsing configuration file: {config_path:?}");

    // Parse TOML file contents
    let contents = read_to_string(&config_path).await?;
    let contents: toml::Value = match toml::from_str(&contents) {
        Ok(v) => v,
        Err(e) => {
            error!(target: "darkfid", "Failed parsing TOML config: {e}");
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
                error!(target: "darkfid", "Failed parsing requested network configuration: {e}");
                return Err(Error::ParseFailed("Failed parsing requested network configuration"))
            }
        };
    debug!(target: "darkfid", "Parsed network configuration: {network_config:?}");

    Ok(network_config)
}
