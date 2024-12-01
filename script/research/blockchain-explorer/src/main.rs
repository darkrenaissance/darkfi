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

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use log::{debug, error, info};
use sled_overlay::sled;
use smol::{io::Cursor, lock::Mutex, stream::StreamExt};
use structopt_toml::{serde::Deserialize, structopt::StructOpt, StructOptToml};
use url::Url;

use darkfi::{
    async_daemonize,
    blockchain::{Blockchain, BlockchainOverlay},
    cli_desc,
    error::TxVerifyFailed,
    rpc::{
        client::RpcClient,
        server::{listen_and_serve, RequestHandler},
    },
    runtime::vm_runtime::Runtime,
    system::{StoppableTask, StoppableTaskPtr},
    tx::Transaction,
    util::path::expand_path,
    validator::{
        fees::{circuit_gas_use, GasData, PALLAS_SCHNORR_SIGNATURE_FEE},
        utils::deploy_native_contracts,
    },
    zk::VerifyingKey,
    Error, Result,
};
use darkfi_sdk::{
    crypto::{ContractId, PublicKey},
    deploy::DeployParamsV1,
    pasta::pallas,
};
use darkfi_serial::{deserialize_async, serialize_async, AsyncDecodable, AsyncEncodable};

use crate::metrics_store::{GasMetrics, GasMetricsKey, MetricsStore};

/// Crate errors
mod error;

/// JSON-RPC requests handler and methods
mod rpc;
mod rpc_blocks;
use rpc_blocks::subscribe_blocks;
mod rpc_statistics;
mod rpc_transactions;

/// Database functionality related to blocks
mod blocks;

/// Database functionality related to transactions
mod transactions;

/// Database functionality related to statistics
mod statistics;

/// Test utilities used for unit and integration testing
mod test_utils;

/// Database store functionality related to metrics
mod metrics_store;

const CONFIG_FILE: &str = "blockchain_explorer_config.toml";
const CONFIG_FILE_CONTENTS: &str = include_str!("../blockchain_explorer_config.toml");

#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[serde(default)]
#[structopt(name = "blockchain-explorer", about = cli_desc!())]
struct Args {
    #[structopt(short, long)]
    /// Configuration file to use
    config: Option<String>,

    #[structopt(short, long, default_value = "tcp://127.0.0.1:14567")]
    /// JSON-RPC listen URL
    rpc_listen: Url,

    #[structopt(long, default_value = "~/.local/share/darkfi/blockchain-explorer/daemon.db")]
    /// Path to daemon database
    db_path: String,

    #[structopt(long)]
    /// Reset the database and start syncing from first block
    reset: bool,

    #[structopt(short, long, default_value = "tcp://127.0.0.1:8340")]
    /// darkfid JSON-RPC endpoint
    endpoint: Url,

    #[structopt(short, long)]
    /// Set log file to output into
    log: Option<String>,

    #[structopt(short, parse(from_occurrences))]
    /// Increase verbosity (-vvv supported)
    verbose: u8,
}

/// Structure represents the explorer database backed by a sled DB connection.
pub struct ExplorerDb {
    /// Main pointer to the sled db connection
    pub sled_db: sled::Db,
    /// Explorer darkfid blockchain copy
    pub blockchain: Blockchain,
    /// Metrics store instance
    pub metrics_store: MetricsStore,
}

impl ExplorerDb {
    /// Creates a new `BlockExplorerDb` instance
    pub fn new(db_path: String) -> Result<ExplorerDb> {
        let db_path = expand_path(db_path.as_str())?;
        let sled_db = sled::open(&db_path)?;
        let blockchain = Blockchain::new(&sled_db)?;
        let metrics_store = MetricsStore::new(&sled_db)?;
        info!(target: "blockchain-explorer", "Initialized explorer database {}, block count: {}", db_path.display(), blockchain.len());
        Ok(ExplorerDb { sled_db, blockchain, metrics_store })
    }

    /// Calculates the fee data for a given transaction, returning a [`GasData`] object detailing various aspects of the gas usage.
    pub async fn calculate_tx_gas_data(
        &self,
        tx: &Transaction,
        verify_fee: bool,
    ) -> Result<GasData> {
        let tx_hash = tx.hash();

        let overlay = BlockchainOverlay::new(&self.blockchain)?;

        // Gas accumulators
        let mut total_gas_used = 0;
        let mut zk_circuit_gas_used = 0;
        let mut wasm_gas_used = 0;
        let mut deploy_gas_used = 0;
        let mut gas_paid = 0;

        // Table of public inputs used for ZK proof verification
        let mut zkp_table = vec![];
        // Table of public keys used for signature verification
        let mut sig_table = vec![];

        // Index of the Fee-paying call
        let fee_call_idx = 0;

        // Map of ZK proof verifying keys for the transaction
        let mut verifying_keys: HashMap<[u8; 32], HashMap<String, VerifyingKey>> = HashMap::new();
        for call in &tx.calls {
            verifying_keys.insert(call.data.contract_id.to_bytes(), HashMap::new());
        }

        let block_target = self.blockchain.blocks.get_last()?.0 + 1;

        // We'll also take note of all the circuits in a Vec so we can calculate their verification cost.
        let mut circuits_to_verify = vec![];

        // Iterate over all calls to get the metadata
        for (idx, call) in tx.calls.iter().enumerate() {
            // Transaction must not contain a Money::PoWReward(0x02) call
            if call.data.is_money_pow_reward() {
                error!(target: "block_explorer::calculate_tx_gas_data", "Reward transaction detected");
                return Err(TxVerifyFailed::ErroneousTxs(vec![tx.clone()]).into())
            }

            // Write the actual payload data
            let mut payload = vec![];
            tx.calls.encode_async(&mut payload).await?;

            let wasm = overlay.lock().unwrap().contracts.get(call.data.contract_id)?;

            let mut runtime = Runtime::new(
                &wasm,
                overlay.clone(),
                call.data.contract_id,
                block_target,
                block_target,
                tx_hash,
                idx as u8,
            )?;

            let metadata = runtime.metadata(&payload)?;

            // Decode the metadata retrieved from the execution
            let mut decoder = Cursor::new(&metadata);

            // The tuple is (zkas_ns, public_inputs)
            let zkp_pub: Vec<(String, Vec<pallas::Base>)> =
                AsyncDecodable::decode_async(&mut decoder).await?;
            let sig_pub: Vec<PublicKey> = AsyncDecodable::decode_async(&mut decoder).await?;

            if decoder.position() != metadata.len() as u64 {
                error!(
                    target: "block_explorer::calculate_tx_gas_data",
                    "[BLOCK_EXPLORER] Failed decoding entire metadata buffer for {}:{}", tx_hash, idx,
                );
                return Err(TxVerifyFailed::ErroneousTxs(vec![tx.clone()]).into())
            }

            // Here we'll look up verifying keys and insert them into the per-contract map.
            for (zkas_ns, _) in &zkp_pub {
                let inner_vk_map =
                    verifying_keys.get_mut(&call.data.contract_id.to_bytes()).unwrap();

                // TODO: This will be a problem in case of ::deploy, unless we force a different
                // namespace and disable updating existing circuit. Might be a smart idea to do
                // so in order to have to care less about being able to verify historical txs.
                if inner_vk_map.contains_key(zkas_ns.as_str()) {
                    continue
                }

                let (zkbin, vk) =
                    overlay.lock().unwrap().contracts.get_zkas(&call.data.contract_id, zkas_ns)?;

                inner_vk_map.insert(zkas_ns.to_string(), vk);
                circuits_to_verify.push(zkbin);
            }

            zkp_table.push(zkp_pub);
            sig_table.push(sig_pub);

            // Contracts are not included within blocks. They need to be deployed off-chain so that they can be accessed and utilized for fee data computation
            if call.data.is_deployment()
            /* DeployV1 */
            {
                // Deserialize the deployment parameters
                let deploy_params: DeployParamsV1 = deserialize_async(&call.data.data[1..]).await?;
                let deploy_cid = ContractId::derive_public(deploy_params.public_key);

                // Instantiate the new deployment runtime
                let mut deploy_runtime = Runtime::new(
                    &deploy_params.wasm_bincode,
                    overlay.clone(),
                    deploy_cid,
                    block_target,
                    block_target,
                    tx_hash,
                    idx as u8,
                )?;

                deploy_runtime.deploy(&deploy_params.ix)?;

                deploy_gas_used = deploy_runtime.gas_used();

                // Append the used deployment gas
                total_gas_used += deploy_gas_used;
            }

            // At this point we're done with the call and move on to the next one.
            // Accumulate the WASM gas used.
            wasm_gas_used = runtime.gas_used();

            // Append the used wasm gas
            total_gas_used += wasm_gas_used;
        }

        // The signature fee is tx_size + fixed_sig_fee * n_signatures
        let signature_gas_used = (PALLAS_SCHNORR_SIGNATURE_FEE * tx.signatures.len() as u64) +
            serialize_async(tx).await.len() as u64;

        // Append the used signature gas
        total_gas_used += signature_gas_used;

        // The ZK circuit fee is calculated using a function in validator/fees.rs
        for zkbin in circuits_to_verify.iter() {
            zk_circuit_gas_used = circuit_gas_use(zkbin);

            // Append the used zk circuit gas
            total_gas_used += zk_circuit_gas_used;
        }

        if verify_fee {
            // Deserialize the fee call to find the paid fee
            let fee: u64 = match deserialize_async(&tx.calls[fee_call_idx].data.data[1..9]).await {
                Ok(v) => v,
                Err(e) => {
                    error!(
                        target: "block_explorer::calculate_tx_gas_data",
                        "[VALIDATOR] Failed deserializing tx {} fee call: {}", tx_hash, e,
                    );
                    return Err(TxVerifyFailed::InvalidFee.into())
                }
            };

            // TODO: This counts 1 gas as 1 token unit. Pricing should be better specified.
            // Check that enough fee has been paid for the used gas in this transaction.
            if total_gas_used > fee {
                error!(
                    target: "block_explorer::calculate_tx_gas_data",
                    "[VALIDATOR] Transaction {} has insufficient fee. Required: {}, Paid: {}",
                    tx_hash, total_gas_used, fee,
                );
                return Err(TxVerifyFailed::InsufficientFee.into())
            }
            debug!(target: "block_explorer::calculate_tx_gas_data", "The gas paid for transaction {}: {}", tx_hash, gas_paid);

            // Store paid fee
            gas_paid = fee;
        }

        // Commit changes made to the overlay
        overlay.lock().unwrap().overlay.lock().unwrap().apply()?;

        let fee_data = GasData {
            paid: gas_paid,
            wasm: wasm_gas_used,
            zk_circuits: zk_circuit_gas_used,
            signatures: signature_gas_used,
            deployments: deploy_gas_used,
        };

        debug!(target: "block_explorer::calculate_tx_gas_data", "The total gas usage for transaction {}: {:?}", tx_hash, fee_data);

        Ok(fee_data)
    }
}

/// Daemon structure
pub struct Explorerd {
    /// Explorer database instance
    pub db: ExplorerDb,
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
        info!(target: "blockchain-explorer", "Created rpc client: {:?}", endpoint);

        // Initialize explorer database
        let explorer_db = ExplorerDb::new(db_path)?;

        // Deploy native contracts need to calculated transaction gas data and commit changes
        let overlay = BlockchainOverlay::new(&explorer_db.blockchain)?;
        deploy_native_contracts(&overlay, 10).await?;
        overlay.lock().unwrap().overlay.lock().unwrap().apply()?;

        Ok(Self { rpc_connections: Mutex::new(HashSet::new()), rpc_client, db: explorer_db })
    }

    /// Fetches the most current metrics from the [`MetricsStore`], returning an `Option` containing
    /// a pair of [`GasMetricsKey`] and [`GasMetrics`] upon success, or `None` if no metrics are found.
    pub fn get_latest_metrics(&self) -> Result<Option<(GasMetricsKey, GasMetrics)>> {
        self.db.metrics_store.get_last()
    }
}

async_daemonize!(realmain);
async fn realmain(args: Args, ex: Arc<smol::Executor<'static>>) -> Result<()> {
    info!(target: "blockchain-explorer", "Initializing DarkFi blockchain explorer node...");
    let explorer = Explorerd::new(args.db_path, args.endpoint.clone(), ex.clone()).await?;
    let explorer = Arc::new(explorer);
    info!(target: "blockchain-explorer", "Node initialized successfully!");

    // JSON-RPC server
    info!(target: "blockchain-explorer", "Starting JSON-RPC server");
    // Here we create a task variable so we can manually close the task later.
    let rpc_task = StoppableTask::new();
    let explorer_ = explorer.clone();
    rpc_task.clone().start(
        listen_and_serve(args.rpc_listen, explorer.clone(), None, ex.clone()),
        |res| async move {
            match res {
                Ok(()) | Err(Error::RpcServerStopped) => explorer_.stop_connections().await,
                Err(e) => error!(target: "blockchain-explorer", "Failed starting sync JSON-RPC server: {}", e),
            }
        },
        Error::RpcServerStopped,
        ex.clone(),
    );

    // Sync blocks
    info!(target: "blockchain-explorer", "Syncing blocks from darkfid...");
    if let Err(e) = explorer.sync_blocks(args.reset).await {
        let error_message = format!("Error syncing blocks: {:?}", e);
        error!(target: "blockchain-explorer", "{error_message}");
        return Err(Error::DatabaseError(error_message));
    }

    // Subscribe blocks
    info!(target: "blockchain-explorer", "Subscribing to new blocks...");
    let (subscriber_task, listener_task) =
        match subscribe_blocks(explorer.clone(), args.endpoint, ex.clone()).await {
            Ok(pair) => pair,
            Err(e) => {
                let error_message = format!("Error setting up blocks subscriber: {:?}", e);
                error!(target: "blockchain-explorer", "{error_message}");
                return Err(Error::DatabaseError(error_message));
            }
        };

    // Signal handling for graceful termination.
    let (signals_handler, signals_task) = SignalHandler::new(ex)?;
    signals_handler.wait_termination(signals_task).await?;
    info!(target: "blockchain-explorer", "Caught termination signal, cleaning up and exiting...");

    info!(target: "blockchain-explorer", "Stopping JSON-RPC server...");
    rpc_task.stop().await;

    info!(target: "blockchain-explorer", "Stopping darkfid listener...");
    listener_task.stop().await;

    info!(target: "blockchain-explorer", "Stopping darkfid subscriber...");
    subscriber_task.stop().await;

    info!(target: "blockchain-explorer", "Stopping JSON-RPC client...");
    explorer.rpc_client.stop().await;

    Ok(())
}
