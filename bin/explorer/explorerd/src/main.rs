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

use lazy_static::lazy_static;
use log::{debug, error, info};
use sled_overlay::sled;
use smol::{lock::Mutex, stream::StreamExt};
use std::{
    collections::{HashMap, HashSet},
    path::Path,
    str::FromStr,
    sync::Arc,
};
use structopt_toml::{serde::Deserialize, structopt::StructOpt, StructOptToml};
use url::Url;

use darkfi::{
    async_daemonize,
    blockchain::{Blockchain, BlockchainOverlay},
    cli_desc,
    rpc::{
        client::RpcClient,
        server::{listen_and_serve, RequestHandler},
    },
    system::{StoppableTask, StoppableTaskPtr},
    util::path::{expand_path, get_config_path},
    validator::utils::deploy_native_contracts,
    Error, Result,
};
use darkfi_sdk::crypto::{ContractId, DAO_CONTRACT_ID, DEPLOYOOOR_CONTRACT_ID, MONEY_CONTRACT_ID};

use crate::{
    config::ExplorerNetworkConfig,
    contract_meta_store::{ContractMetaData, ContractMetaStore},
    contracts::untar_source,
    metrics_store::MetricsStore,
    rpc_blocks::subscribe_blocks,
};

/// Crate errors
mod error;

/// JSON-RPC requests handler and methods
mod rpc;
mod rpc_blocks;
mod rpc_contracts;
mod rpc_statistics;
mod rpc_transactions;

/// Service functionality related to blocks
mod blocks;

/// Service functionality related to transactions
mod transactions;

/// Service functionality related to statistics
mod statistics;

/// Service functionality related to contracts
mod contracts;

/// Test utilities used for unit and integration testing
mod test_utils;

/// Database store functionality related to metrics
mod metrics_store;

/// Database store functionality related to contract metadata
mod contract_meta_store;

/// Configuration management across multiple networks (localnet, testnet, mainnet)
mod config;

const CONFIG_FILE: &str = "explorerd_config.toml";
const CONFIG_FILE_CONTENTS: &str = include_str!("../explorerd_config.toml");

// Load the contract source archives to bootstrap them on explorer startup
lazy_static! {
    static ref NATIVE_CONTRACT_SOURCE_ARCHIVES: HashMap<String, &'static [u8]> = {
        let mut src_map = HashMap::new();
        src_map.insert(
            MONEY_CONTRACT_ID.to_string(),
            &include_bytes!("../native_contracts_src/money_contract_src.tar")[..],
        );
        src_map.insert(
            DAO_CONTRACT_ID.to_string(),
            &include_bytes!("../native_contracts_src/dao_contract_src.tar")[..],
        );
        src_map.insert(
            DEPLOYOOOR_CONTRACT_ID.to_string(),
            &include_bytes!("../native_contracts_src/deployooor_contract_src.tar")[..],
        );
        src_map
    };
}

#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[serde(default)]
#[structopt(name = "explorerd", about = cli_desc!())]
struct Args {
    #[structopt(short, long)]
    /// Configuration file to use
    config: Option<String>,

    #[structopt(short, long, default_value = "testnet")]
    /// Explorer network (localnet, testnet, mainnet)
    network: String,

    #[structopt(long)]
    /// Reset the database and start syncing from first block
    reset: bool,

    #[structopt(short, long)]
    /// Set log file to output to
    log: Option<String>,

    #[structopt(short, parse(from_occurrences))]
    /// Increase verbosity (-vvv supported)
    verbose: u8,
}

/// Represents the service layer for the Explorer application, bridging the RPC layer and the database.
/// It encapsulates explorer business logic and provides a unified interface for core functionalities,
/// providing a clear separation of concerns between RPC handling and data management layers.
///
/// Core functionalities include:
///
/// - Data Transformation: Converting database data into structured responses suitable for RPC callers.
/// - Blocks: Synchronization, retrieval, counting, and management.
/// - Contracts: Handling native and user contract data, source code, tar files, and metadata.
/// - Metrics: Providing metric-related data over the life of the chain.
/// - Transactions: Synchronization, calculating gas data, retrieval, counting, and related block information.
pub struct ExplorerService {
    /// Explorer database instance
    db: ExplorerDb,
}

impl ExplorerService {
    /// Creates a new `ExplorerService` instance.
    pub fn new(db_path: String) -> Result<Self> {
        // Initialize explorer database
        let db = ExplorerDb::new(db_path)?;

        Ok(Self { db })
    }

    /// Initializes the explorer service by deploying native contracts and loading native contract
    /// source code and metadata required for its operation.
    pub async fn init(&self) -> Result<()> {
        self.deploy_native_contracts().await?;
        self.load_native_contract_sources()?;
        self.load_native_contract_metadata()?;
        Ok(())
    }

    /// Deploys native contracts required for gas calculation and retrieval.
    pub async fn deploy_native_contracts(&self) -> Result<()> {
        let overlay = BlockchainOverlay::new(&self.db.blockchain)?;
        deploy_native_contracts(&overlay, 10).await?;
        overlay.lock().unwrap().overlay.lock().unwrap().apply()?;
        Ok(())
    }

    /// Loads native contract source code into the explorer database by extracting it from tar archives
    /// created during the explorer build process. The extracted source code is associated with
    /// the corresponding [`ContractId`] for each loaded contract and stored.
    pub fn load_native_contract_sources(&self) -> Result<()> {
        // Iterate each native contract source archive
        for (contract_id_str, archive_bytes) in NATIVE_CONTRACT_SOURCE_ARCHIVES.iter() {
            // Untar the native contract source code
            let source_code = untar_source(archive_bytes)?;

            // Parse contract id into a contract id instance
            let contract_id = &ContractId::from_str(contract_id_str)?;

            // Add source code into the `ContractMetaStore`
            self.db.contract_meta_store.insert_source(contract_id, &source_code)?;
            info!(target: "explorerd: load_native_contract_sources", "Loaded native contract source {}", contract_id_str.to_string());
        }
        Ok(())
    }

    /// Loads [`ContractMetaData`] for deployed native contracts into the explorer database by adding descriptive
    /// information (e.g., name and description) used to display contract details.
    pub fn load_native_contract_metadata(&self) -> Result<()> {
        let contract_ids = [*MONEY_CONTRACT_ID, *DAO_CONTRACT_ID, *DEPLOYOOOR_CONTRACT_ID];

        // Create pre-defined native contract metadata
        let metadatas = [
            ContractMetaData::new(
                "Money".to_string(),
                "Facilitates money transfers, atomic swaps, minting, freezing, and staking of consensus tokens".to_string(),
            ),
            ContractMetaData::new(
                "DAO".to_string(),
                "Provides functionality for Anonymous DAOs".to_string(),
            ),
            ContractMetaData::new(
                "Deployoor".to_string(),
                "Handles non-native smart contract deployments".to_string(),
            ),
        ];

        // Load contract metadata into the `ContractMetaStore`
        self.db.contract_meta_store.insert_metadata(&contract_ids, &metadatas)?;
        info!(target: "explorerd: load_native_contract_metadata", "Loaded metadata for native contracts");

        Ok(())
    }

    /// Resets the explorer state to the specified height. If a genesis block height is provided,
    /// all blocks and transactions are purged from the database. Otherwise, the state is reverted
    /// to the given height. The explorer metrics are updated to reflect the updated blocks and
    /// transactions up to the reset height, ensuring consistency. Returns a result indicating
    /// success or an error if the operation fails.
    pub fn reset_explorer_state(&self, height: u32) -> Result<()> {
        debug!(target: "explorerd::reset_explorer_state", "Resetting explorer state to height: {height}");

        // Check if a genesis block reset or to a specific height
        match height {
            // Reset for genesis height 0, purge blocks and transactions
            0 => {
                self.reset_blocks()?;
                self.reset_transactions()?;
                debug!(target: "explorerd::reset_explorer_state", "Reset explorer state to accept a new genesis block");
            }
            // Reset for all other heights
            _ => {
                self.reset_to_height(height)?;
                debug!(target: "explorerd::reset_explorer_state", "Reset blocks to height: {height}");
            }
        }

        // Reset gas metrics to the specified height to reflect the updated blockchain state
        self.db.metrics_store.reset_gas_metrics(height)?;
        debug!(target: "explorerd::reset_explorer_state", "Reset metrics store to height: {height}");

        Ok(())
    }
}

/// Represents the explorer database backed by a `sled` database connection, responsible for maintaining
/// persistent state required for blockchain exploration. It serves as the core data layer for the Explorer application,
/// storing and managing blockchain data, metrics, and contract-related information.
pub struct ExplorerDb {
    /// The main `sled` database connection used for data storage and retrieval
    pub sled_db: sled::Db,
    /// Local copy of the Darkfi blockchain used for block synchronization and exploration
    pub blockchain: Blockchain,
    /// Store for tracking chain-related metrics
    pub metrics_store: MetricsStore,
    /// Store for managing contract metadata, source code, and related data
    pub contract_meta_store: ContractMetaStore,
}

impl ExplorerDb {
    /// Creates a new `ExplorerDb` instance
    pub fn new(db_path: String) -> Result<Self> {
        let db_path = expand_path(db_path.as_str())?;
        let sled_db = sled::open(&db_path)?;
        let blockchain = Blockchain::new(&sled_db)?;
        let metrics_store = MetricsStore::new(&sled_db)?;
        let contract_meta_store = ContractMetaStore::new(&sled_db)?;
        info!(target: "explorerd", "Initialized explorer database {}: block count: {}, tx count: {}", db_path.display(), blockchain.len(), blockchain.txs_len());
        Ok(Self { sled_db, blockchain, metrics_store, contract_meta_store })
    }
}

/// Defines a daemon structure responsible for handling incoming JSON-RPC requests and delegating them
/// to the backend layer for processing. It provides a JSON-RPC interface for managing operations related to
/// blocks, transactions, contracts, and metrics.
///
/// Upon startup, the daemon initializes a background task to handle incoming JSON-RPC requests.
/// This includes processing operations related to blocks, transactions, contracts, and metrics by
/// delegating them to the backend and returning appropriate RPC responses. Additionally, the daemon
/// synchronizes blocks from the `darkfid` daemon into the explorer database and subscribes
/// to new blocks, ensuring that the local database remains updated in real-time.
pub struct Explorerd {
    /// Explorer service instance
    pub service: ExplorerService,
    /// JSON-RPC connection tracker
    pub rpc_connections: Mutex<HashSet<StoppableTaskPtr>>,
    /// JSON-RPC client to execute requests to darkfid daemon
    pub rpc_client: RpcClient,
}

impl Explorerd {
    /// Creates a new `BlockchainExplorer` instance.
    async fn new(db_path: String, endpoint: Url, ex: Arc<smol::Executor<'static>>) -> Result<Self> {
        // Initialize rpc client
        let rpc_client = RpcClient::new(endpoint.clone(), ex).await?;
        info!(target: "explorerd", "Connected to Darkfi node: {}", endpoint.to_string().trim_end_matches('/'));

        // Create explorer service
        let service = ExplorerService::new(db_path)?;

        // Initialize the explorer service
        service.init().await?;

        Ok(Self { rpc_connections: Mutex::new(HashSet::new()), rpc_client, service })
    }
}

async_daemonize!(realmain);
async fn realmain(args: Args, ex: Arc<smol::Executor<'static>>) -> Result<()> {
    info!(target: "explorerd", "Initializing DarkFi blockchain explorer node...");

    // Resolve the configuration path
    let config_path = get_config_path(args.config.clone(), CONFIG_FILE)?;

    // Get explorer network configuration
    let config: ExplorerNetworkConfig = (&config_path, &args.network).try_into()?;

    // Initialize the explorer daemon instance
    let explorer =
        Explorerd::new(config.database.clone(), config.endpoint.clone(), ex.clone()).await?;
    let explorer = Arc::new(explorer);
    info!(target: "explorerd", "Node initialized successfully!");

    // JSON-RPC server
    // Here we create a task variable so we can manually close the task later.
    let rpc_task = StoppableTask::new();
    let explorer_ = explorer.clone();
    rpc_task.clone().start(
        listen_and_serve(config.rpc.clone().into(), explorer.clone(), None, ex.clone()),
        |res| async move {
            match res {
                Ok(()) | Err(Error::RpcServerStopped) => explorer_.stop_connections().await,
                Err(e) => {
                    error!(target: "explorerd", "Failed starting sync JSON-RPC server: {}", e)
                }
            }
        },
        Error::RpcServerStopped,
        ex.clone(),
    );
    info!(target: "explorerd", "Started JSON-RPC server: {}", config.rpc.rpc_listen.to_string().trim_end_matches("/"));

    // Sync blocks
    info!(target: "explorerd", "Syncing blocks from darkfid...");
    if let Err(e) = explorer.sync_blocks(args.reset).await {
        let error_message = format!("Error syncing blocks: {:?}", e);
        error!(target: "explorerd", "{error_message}");
        return Err(Error::DatabaseError(error_message));
    }

    // Subscribe blocks
    info!(target: "explorerd", "Subscribing to new blocks...");
    let (subscriber_task, listener_task) =
        match subscribe_blocks(explorer.clone(), config.endpoint.clone(), ex.clone()).await {
            Ok(pair) => pair,
            Err(e) => {
                let error_message = format!("Error setting up blocks subscriber: {:?}", e);
                error!(target: "explorerd", "{error_message}");
                return Err(Error::DatabaseError(error_message));
            }
        };

    log_started_banner(explorer.clone(), &config, &args, &config_path);
    info!(target: "explorerd::", "All is good. Waiting for block notifications...");

    // Signal handling for graceful termination.
    let (signals_handler, signals_task) = SignalHandler::new(ex)?;
    signals_handler.wait_termination(signals_task).await?;
    info!(target: "explorerd", "Caught termination signal, cleaning up and exiting...");

    info!(target: "explorerd", "Stopping JSON-RPC server...");
    rpc_task.stop().await;

    info!(target: "explorerd", "Stopping darkfid listener...");
    listener_task.stop().await;

    info!(target: "explorerd", "Stopping darkfid subscriber...");
    subscriber_task.stop().await;

    info!(target: "explorerd", "Stopping JSON-RPC client...");
    explorer.rpc_client.stop().await;

    Ok(())
}

/// Logs a banner displaying the startup details of the DarkFi Explorer Node.
fn log_started_banner(
    explorer: Arc<Explorerd>,
    config: &ExplorerNetworkConfig,
    args: &Args,
    config_path: &Path,
) {
    info!(target: "explorerd", "========================================================================================");
    info!(target: "explorerd", "                   Started DarkFi Explorer Node                                        ");
    info!(target: "explorerd", "========================================================================================");
    info!(target: "explorerd", "  - Network: {}", args.network);
    info!(target: "explorerd", "  - JSON-RPC Endpoint: {}", config.rpc.rpc_listen.to_string().trim_end_matches("/"));
    info!(target: "explorerd", "  - Database: {}", config.database);
    info!(target: "explorerd", "  - Configuration: {}", config_path.to_str().unwrap_or("Error: configuration path not found!"));
    info!(target: "explorerd", "  - Reset Blocks: {}", if args.reset { "Yes" } else { "No" });
    info!(target: "explorerd", "~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~");
    info!(target: "explorerd", "  - Synced Blocks: {}", explorer.service.db.blockchain.len());
    info!(target: "explorerd", "  - Synced Transactions: {}", explorer.service.db.blockchain.len());
    info!(target: "explorerd", "  - Connected Darkfi Node: {}", config.endpoint.to_string().trim_end_matches("/"));
    info!(target: "explorerd", "========================================================================================");
}
