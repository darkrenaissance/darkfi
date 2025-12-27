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

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use smol::lock::{Mutex, RwLock};
use tinyjson::JsonValue;
use tracing::{error, info};

use darkfi::{
    blockchain::BlockInfo,
    rpc::{
        jsonrpc::JsonSubscriber,
        server::{listen_and_serve, RequestHandler},
        settings::RpcSettings,
    },
    system::{ExecutorPtr, StoppableTask, StoppableTaskPtr},
    util::encoding::base64,
    validator::{consensus::Proposal, ValidatorPtr},
    Error, Result,
};
use darkfi_sdk::crypto::{keypair::Network, pasta_prelude::PrimeField};
use darkfi_serial::serialize_async;

use crate::{
    proto::{DarkfidP2pHandlerPtr, ProposalMessage},
    rpc::{rpc_stratum::StratumRpcHandler, rpc_xmr::MmRpcHandler},
    DarkfiNode, DarkfiNodePtr,
};

/// Block related structures
pub mod model;
use model::{
    generate_next_block_template, BlockTemplate, MinerClient, MinerRewardsRecipientConfig,
    PowRewardV1Zk,
};

/// Atomic pointer to the DarkFi node miners registry.
pub type DarkfiMinersRegistryPtr = Arc<DarkfiMinersRegistry>;

/// DarkFi node miners registry.
pub struct DarkfiMinersRegistry {
    /// Blockchain network
    pub network: Network,
    /// PowRewardV1 ZK data
    pub powrewardv1_zk: PowRewardV1Zk,
    /// Mining block templates of each wallet config
    pub block_templates: RwLock<HashMap<String, BlockTemplate>>,
    /// Active mining jobs mapped to the wallet template they
    /// represent. For native jobs the key(job id) is the hex
    /// encoded header hash, while for merge mining jobs it's
    /// the header template hash.
    pub jobs: RwLock<HashMap<String, String>>,
    /// Active native clients mapped to their information.
    pub clients: RwLock<HashMap<String, MinerClient>>,
    /// Submission lock so we can queue up submissions process
    pub submit_lock: RwLock<()>,
    /// Stratum JSON-RPC background task
    stratum_rpc_task: StoppableTaskPtr,
    /// Stratum JSON-RPC connection tracker
    pub stratum_rpc_connections: Mutex<HashSet<StoppableTaskPtr>>,
    /// HTTP JSON-RPC background task
    mm_rpc_task: StoppableTaskPtr,
    /// HTTP JSON-RPC connection tracker
    pub mm_rpc_connections: Mutex<HashSet<StoppableTaskPtr>>,
}

impl DarkfiMinersRegistry {
    /// Initialize a DarkFi node miners registry.
    pub fn init(network: Network, validator: &ValidatorPtr) -> Result<DarkfiMinersRegistryPtr> {
        info!(
            target: "darkfid::registry::mod::DarkfiMinersRegistry::init",
            "Initializing a new DarkFi node miners registry..."
        );

        // Generate the PowRewardV1 ZK data
        let powrewardv1_zk = PowRewardV1Zk::new(validator)?;

        // Generate the stratum JSON-RPC background task and its
        // connections tracker.
        let stratum_rpc_task = StoppableTask::new();
        let stratum_rpc_connections = Mutex::new(HashSet::new());

        // Generate the HTTP JSON-RPC background task and its
        // connections tracker.
        let mm_rpc_task = StoppableTask::new();
        let mm_rpc_connections = Mutex::new(HashSet::new());

        info!(
            target: "darkfid::registry::mod::DarkfiMinersRegistry::init",
            "DarkFi node miners registry generated successfully!"
        );

        Ok(Arc::new(Self {
            network,
            powrewardv1_zk,
            block_templates: RwLock::new(HashMap::new()),
            jobs: RwLock::new(HashMap::new()),
            clients: RwLock::new(HashMap::new()),
            submit_lock: RwLock::new(()),
            stratum_rpc_task,
            stratum_rpc_connections,
            mm_rpc_task,
            mm_rpc_connections,
        }))
    }

    /// Start the DarkFi node miners registry for provided DarkFi node
    /// instance.
    pub fn start(
        &self,
        executor: &ExecutorPtr,
        node: &DarkfiNodePtr,
        stratum_rpc_settings: &Option<RpcSettings>,
        mm_rpc_settings: &Option<RpcSettings>,
    ) -> Result<()> {
        info!(
            target: "darkfid::registry::mod::DarkfiMinersRegistry::start",
            "Starting the DarkFi node miners registry..."
        );

        // Start the stratum server JSON-RPC task
        if let Some(stratum_rpc) = stratum_rpc_settings {
            info!(target: "darkfid::registry::mod::DarkfiMinersRegistry::start", "Starting Stratum JSON-RPC server");
            let node_ = node.clone();
            self.stratum_rpc_task.clone().start(
                listen_and_serve::<StratumRpcHandler>(stratum_rpc.clone(), node.clone(), None, executor.clone()),
                |res| async move {
                    match res {
                        Ok(()) | Err(Error::RpcServerStopped) => <DarkfiNode as RequestHandler<StratumRpcHandler>>::stop_connections(&node_).await,
                        Err(e) => error!(target: "darkfid::registry::mod::DarkfiMinersRegistry::start", "Failed starting Stratum JSON-RPC server: {e}"),
                    }
                },
                Error::RpcServerStopped,
                executor.clone(),
            );
        } else {
            // Create a dummy task
            self.stratum_rpc_task.clone().start(
                async { Ok(()) },
                |_| async { /* Do nothing */ },
                Error::RpcServerStopped,
                executor.clone(),
            );
        }

        // Start the merge mining JSON-RPC task
        if let Some(mm_rpc) = mm_rpc_settings {
            info!(target: "darkfid::registry::mod::DarkfiMinersRegistry::start", "Starting merge mining JSON-RPC server");
            let node_ = node.clone();
            self.mm_rpc_task.clone().start(
                listen_and_serve::<MmRpcHandler>(mm_rpc.clone(), node.clone(), None, executor.clone()),
                |res| async move {
                    match res {
                        Ok(()) | Err(Error::RpcServerStopped) => <DarkfiNode as RequestHandler<MmRpcHandler>>::stop_connections(&node_).await,
                        Err(e) => error!(target: "darkfid::registry::mod::DarkfiMinersRegistry::start", "Failed starting merge mining JSON-RPC server: {e}"),
                    }
                },
                Error::RpcServerStopped,
                executor.clone(),
            );
        } else {
            // Create a dummy task
            self.mm_rpc_task.clone().start(
                async { Ok(()) },
                |_| async { /* Do nothing */ },
                Error::RpcServerStopped,
                executor.clone(),
            );
        }

        info!(
            target: "darkfid::registry::mod::DarkfiMinersRegistry::start",
            "DarkFi node miners registry started successfully!"
        );

        Ok(())
    }

    /// Stop the DarkFi node miners registry.
    pub async fn stop(&self) {
        info!(target: "darkfid::registry::mod::DarkfiMinersRegistry::stop", "Terminating DarkFi node miners registry...");

        // Stop the Stratum JSON-RPC task
        info!(target: "darkfid::registry::mod::DarkfiMinersRegistry::stop", "Stopping Stratum JSON-RPC server...");
        self.stratum_rpc_task.stop().await;

        // Stop the merge mining JSON-RPC task
        info!(target: "darkfid::registry::mod::DarkfiMinersRegistry::stop", "Stopping merge mining JSON-RPC server...");
        self.mm_rpc_task.stop().await;

        info!(target: "darkfid::registry::mod::DarkfiMinersRegistry::stop", "DarkFi node miners registry terminated successfully!");
    }

    /// Create a registry record for provided wallet config. If the
    /// record already exists return its template, otherwise create its
    /// current template based on provided validator state.
    async fn create_template(
        &self,
        validator: &ValidatorPtr,
        wallet: &String,
        config: &MinerRewardsRecipientConfig,
    ) -> Result<BlockTemplate> {
        // Grab a lock over current templates
        let mut block_templates = self.block_templates.write().await;

        // Check if a template already exists for this wallet
        if let Some(block_template) = block_templates.get(wallet) {
            return Ok(block_template.clone())
        }

        // Grab validator best current fork
        let mut extended_fork = validator.best_current_fork().await?;

        // Generate the next block template
        let result = generate_next_block_template(
            &mut extended_fork,
            config,
            &self.powrewardv1_zk.zkbin,
            &self.powrewardv1_zk.provingkey,
            validator.verify_fees,
        )
        .await;

        // Drop new trees opened by the forks' overlay
        extended_fork.overlay.lock().unwrap().overlay.lock().unwrap().purge_new_trees()?;

        // Check result
        let block_template = result?;

        // Create the new registry record
        block_templates.insert(wallet.clone(), block_template.clone());

        // Print the new template wallet information
        let recipient_str = format!("{}", config.recipient);
        let spend_hook_str = match config.spend_hook {
            Some(spend_hook) => format!("{spend_hook}"),
            None => String::from("-"),
        };
        let user_data_str = match config.user_data {
            Some(user_data) => bs58::encode(user_data.to_repr()).into_string(),
            None => String::from("-"),
        };
        info!(target: "darkfid::registry::mod::DarkfiMinersRegistry::create_template",
            "Created new block template for wallet: address={recipient_str}, spend_hook={spend_hook_str}, user_data={user_data_str}",
        );

        Ok(block_template)
    }

    /// Register a new miner and create its job.
    pub async fn register_miner(
        &self,
        validator: &ValidatorPtr,
        wallet: &String,
        config: &MinerRewardsRecipientConfig,
    ) -> Result<(String, BlockTemplate, JsonSubscriber)> {
        // Grab a lock over current jobs and clients
        let mut jobs = self.jobs.write().await;
        let mut clients = self.clients.write().await;

        // Create wallet template
        let block_template = self.create_template(validator, wallet, config).await?;

        // Grab the hex encoded block hash and create the job record
        let block_hash = hex::encode(block_template.block.header.hash().inner()).to_string();
        jobs.insert(block_hash.clone(), wallet.clone());

        // Create the client record
        let (client_id, client) = MinerClient::new(wallet, config, &block_hash);
        let publisher = client.publisher.clone();
        clients.insert(client_id.clone(), client);

        Ok((client_id, block_template, publisher))
    }

    /// Register a new merge miner and create its job.
    pub async fn register_merge_miner(
        &self,
        validator: &ValidatorPtr,
        wallet: &String,
        config: &MinerRewardsRecipientConfig,
    ) -> Result<(String, f64)> {
        // Grab a lock over current jobs
        let mut jobs = self.jobs.write().await;

        // Create wallet template
        let block_template = self.create_template(validator, wallet, config).await?;

        // Grab the block template hash and its difficulty, and then
        // create the job record.
        let block_template_hash = block_template.block.header.template_hash().as_string();
        let difficulty = block_template.difficulty;
        jobs.insert(block_template_hash.clone(), wallet.clone());

        Ok((block_template_hash, difficulty))
    }

    /// Submit provided block to the provided node.
    pub async fn submit(
        &self,
        validator: &ValidatorPtr,
        subscribers: &HashMap<&'static str, JsonSubscriber>,
        p2p_handler: &DarkfidP2pHandlerPtr,
        block: BlockInfo,
    ) -> Result<()> {
        info!(
            target: "darkfid::registry::mod::DarkfiMinersRegistry::submit",
            "Proposing new block to network",
        );
        let proposal = Proposal::new(block);
        validator.append_proposal(&proposal).await?;

        let proposals_sub = subscribers.get("proposals").unwrap();
        let enc_prop = JsonValue::String(base64::encode(&serialize_async(&proposal).await));
        proposals_sub.notify(vec![enc_prop].into()).await;

        info!(
            target: "darkfid::registry::mod::DarkfiMinersRegistry::submit",
            "Broadcasting new block to network",
        );
        let message = ProposalMessage(proposal);
        p2p_handler.p2p.broadcast(&message).await;

        Ok(())
    }

    /// Refresh outdated jobs in the registry based on provided
    /// validator state.
    pub async fn refresh(&self, validator: &ValidatorPtr) -> Result<()> {
        // Grab locks
        let submit_lock = self.submit_lock.write().await;
        let mut clients = self.clients.write().await;
        let mut jobs = self.jobs.write().await;
        let mut block_templates = self.block_templates.write().await;

        // Find inactive clients
        let mut dropped_clients = vec![];
        let mut active_clients_jobs = vec![];
        for (client_id, client) in clients.iter() {
            if client.publisher.publisher.clear_inactive().await {
                dropped_clients.push(client_id.clone());
                continue
            }
            active_clients_jobs.push(client.job.clone());
        }

        // Drop inactive clients and their jobs
        for client_id in dropped_clients {
            // Its safe to unwrap here since the client key is from the
            // previous loop.
            let client = clients.remove(&client_id).unwrap();
            let wallet = jobs.remove(&client.job).unwrap();
            block_templates.remove(&wallet);
        }

        // Return if no clients exists. Merge miners will create a new
        // template and job on next poll.
        if clients.is_empty() {
            *jobs = HashMap::new();
            *block_templates = HashMap::new();
            return Ok(())
        }

        // Find inactive jobs (not referenced by clients)
        let mut dropped_jobs = vec![];
        let mut active_wallets = vec![];
        for (job, wallet) in jobs.iter() {
            if !active_clients_jobs.contains(job) {
                dropped_jobs.push(job.clone());
                continue
            }
            active_wallets.push(wallet.clone());
        }

        // Drop inactive jobs
        for job in dropped_jobs {
            jobs.remove(&job);
        }

        // Return if no jobs exists. Merge miners will create a new
        // template and job on next poll.
        if jobs.is_empty() {
            *block_templates = HashMap::new();
            return Ok(())
        }

        // Find inactive wallets templates
        let mut dropped_wallets = vec![];
        for wallet in block_templates.keys() {
            if !active_wallets.contains(wallet) {
                dropped_wallets.push(wallet.clone());
            }
        }

        // Drop inactive wallets templates
        for wallet in dropped_wallets {
            block_templates.remove(&wallet);
        }

        // Return if no wallets templates exists. Merge miners will
        // create a new template and job on next poll.
        if block_templates.is_empty() {
            return Ok(())
        }

        // Grab validator best current fork
        let extended_fork = validator.best_current_fork().await?;

        // Iterate over active clients to refresh their jobs
        for (_, client) in clients.iter_mut() {
            // Clone the fork so each client generates over a new one
            let mut extended_fork = extended_fork.full_clone()?;

            // Drop its current job. Its safe to unwrap here since we
            // know the job exists.
            let wallet = jobs.remove(&client.job).unwrap();
            // Drop its current template. Its safe to unwrap here since
            // we know the template exists.
            block_templates.remove(&wallet);

            // Generate the next block template
            let result = generate_next_block_template(
                &mut extended_fork,
                &client.config,
                &self.powrewardv1_zk.zkbin,
                &self.powrewardv1_zk.provingkey,
                validator.verify_fees,
            )
            .await;

            // Drop new trees opened by the forks' overlay
            extended_fork.overlay.lock().unwrap().overlay.lock().unwrap().purge_new_trees()?;

            // Check result
            let block_template = result?;

            // Print the updated template wallet information
            let recipient_str = format!("{}", client.config.recipient);
            let spend_hook_str = match client.config.spend_hook {
                Some(spend_hook) => format!("{spend_hook}"),
                None => String::from("-"),
            };
            let user_data_str = match client.config.user_data {
                Some(user_data) => bs58::encode(user_data.to_repr()).into_string(),
                None => String::from("-"),
            };
            info!(target: "darkfid::registry::mod::DarkfiMinersRegistry::create_template",
                "Updated block template for wallet: address={recipient_str}, spend_hook={spend_hook_str}, user_data={user_data_str}",
            );

            // Create the new job notification
            let (job, notification) = block_template.job_notification();

            // Create the new registry records
            block_templates.insert(wallet.clone(), block_template);
            jobs.insert(job.clone(), wallet);

            // Update the client record
            client.job = job;

            // Push job notification to subscriber
            client.publisher.notify(notification).await;
        }

        // Release all locks
        drop(block_templates);
        drop(jobs);
        drop(submit_lock);

        Ok(())
    }
}
