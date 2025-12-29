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
    /// Active native clients mapped to their job information.
    /// This client information includes their wallet template key,
    /// recipient configuration, current mining job key(job id) and
    /// its connection publisher. For native jobs the job key is the
    /// hex encoded header hash.
    pub jobs: RwLock<HashMap<String, MinerClient>>,
    /// Active merge mining jobs mapped to the wallet template they
    /// represent. The key(job id) is the the header template hash.
    pub mm_jobs: RwLock<HashMap<String, String>>,
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
            mm_jobs: RwLock::new(HashMap::new()),
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
    ) -> Result<(String, String, JsonValue, JsonSubscriber)> {
        // Grab a lock over current native jobs
        let mut jobs = self.jobs.write().await;

        // Create wallet template
        let block_template = self.create_template(validator, wallet, config).await?;

        // Grab the hex encoded block hash and create the client job record
        let (job_id, job) = block_template.job_notification();
        let (client_id, client) = MinerClient::new(wallet, config, &job_id);
        let publisher = client.publisher.clone();
        jobs.insert(client_id.clone(), client);

        Ok((client_id, job_id, job, publisher))
    }

    /// Register a new merge miner and create its job.
    pub async fn register_merge_miner(
        &self,
        validator: &ValidatorPtr,
        wallet: &String,
        config: &MinerRewardsRecipientConfig,
    ) -> Result<(String, f64)> {
        // Grab a lock over current mm jobs
        let mut jobs = self.mm_jobs.write().await;

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
        let proposal = Proposal::new(block);
        validator.append_proposal(&proposal).await?;

        info!(
            target: "darkfid::registry::mod::DarkfiMinersRegistry::submit",
            "Proposing new block to network",
        );

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

    /// Refresh outdated jobs in the provided registry maps based on
    /// provided validator state.
    pub async fn refresh_jobs(
        &self,
        block_templates: &mut HashMap<String, BlockTemplate>,
        jobs: &mut HashMap<String, MinerClient>,
        mm_jobs: &mut HashMap<String, String>,
        validator: &ValidatorPtr,
    ) -> Result<()> {
        // Find inactive native jobs and drop them
        let mut dropped_jobs = vec![];
        let mut active_templates = HashSet::new();
        for (client_id, client) in jobs.iter() {
            // Clear inactive client publisher subscribers. If none
            // exists afterwards, the client is considered inactive so
            // we mark it for drop.
            if client.publisher.publisher.clear_inactive().await {
                dropped_jobs.push(client_id.clone());
                continue
            }

            // Mark client block template as active
            active_templates.insert(client.wallet.clone());
        }
        jobs.retain(|client_id, _| !dropped_jobs.contains(client_id));

        // Grab validator best current fork and its last proposal for
        // checks.
        let extended_fork = validator.best_current_fork().await?;
        let last_proposal = extended_fork.last_proposal()?.hash;

        // Find mm jobs not extending the best current fork and drop
        // them.
        let mut dropped_mm_jobs = vec![];
        for (job_id, wallet) in mm_jobs.iter() {
            // Grab its wallet template. Its safe to unwrap here since
            // we know the job exists.
            let block_template = block_templates.get(wallet).unwrap();

            // Check if it extends current best fork
            if block_template.block.header.previous == last_proposal {
                active_templates.insert(wallet.clone());
                continue
            }

            // This mm job doesn't extend current best fork so we mark
            // it for drop.
            dropped_mm_jobs.push(job_id.clone());
        }
        mm_jobs.retain(|job_id, _| !dropped_mm_jobs.contains(job_id));

        // Drop inactive templates. Merge miners will create a new
        // template and job on next poll.
        block_templates.retain(|wallet, _| active_templates.contains(wallet));

        // Return if no wallets templates exists.
        if block_templates.is_empty() {
            return Ok(())
        }

        // Iterate over active clients to refresh their jobs, if needed
        for (_, client) in jobs.iter_mut() {
            // Grab its wallet template. Its safe to unwrap here since
            // we know the job exists.
            let block_template = block_templates.get_mut(&client.wallet).unwrap();

            // Check if it extends current best fork
            if block_template.block.header.previous == last_proposal {
                continue
            }

            // Clone the fork so each client generates over a new one
            let mut extended_fork = extended_fork.full_clone()?;

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
            *block_template = result?;

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

            // Update the client record
            client.job = job;

            // Push job notification to subscriber
            client.publisher.notify(notification).await;
        }

        Ok(())
    }

    /// Refresh outdated jobs in the registry based on provided
    /// validator state.
    pub async fn refresh(&self, validator: &ValidatorPtr) -> Result<()> {
        // Grab registry locks
        let submit_lock = self.submit_lock.write().await;
        let mut block_templates = self.block_templates.write().await;
        let mut jobs = self.jobs.write().await;
        let mut mm_jobs = self.mm_jobs.write().await;

        // Refresh jobs
        self.refresh_jobs(&mut block_templates, &mut jobs, &mut mm_jobs, validator).await?;

        // Release registry locks
        drop(block_templates);
        drop(jobs);
        drop(mm_jobs);
        drop(submit_lock);

        Ok(())
    }
}
