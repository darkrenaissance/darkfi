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

use darkfi::{
    blockchain::{BlockInfo, Header, HeaderHash},
    rpc::{jsonrpc::JsonNotification, util::JsonValue},
    system::{ExecutorPtr, StoppableTask, Subscription},
    tx::{ContractCallLeaf, Transaction, TransactionBuilder},
    util::{encoding::base64, time::Timestamp},
    validator::{
        consensus::{Fork, Proposal},
        utils::best_fork_index,
        verification::apply_producer_transaction,
    },
    zk::{empty_witnesses, ProvingKey, ZkCircuit},
    zkas::ZkBinary,
    Error, Result,
};
use darkfi_money_contract::{
    client::pow_reward_v1::PoWRewardCallBuilder, MoneyFunction, MONEY_CONTRACT_ZKAS_MINT_NS_V1,
};
use darkfi_sdk::{
    crypto::{poseidon_hash, FuncId, MerkleTree, PublicKey, SecretKey, MONEY_CONTRACT_ID},
    pasta::pallas,
    ContractCall,
};
use darkfi_serial::{serialize_async, Encodable};
use log::{error, info};
use num_bigint::BigUint;
use rand::rngs::OsRng;
use smol::channel::{Receiver, Sender};

use crate::{proto::ProposalMessage, task::garbage_collect_task, DarkfiNodePtr};

/// Auxiliary structure representing node miner rewards recipient configuration
pub struct MinerRewardsRecipientConfig {
    pub recipient: PublicKey,
    pub spend_hook: Option<FuncId>,
    pub user_data: Option<pallas::Base>,
}

/// Async task used for participating in the PoW block production.
///
/// Miner initializes their setup and waits for next confirmation,
/// by listenning for new proposals from the network, for optimal
/// conditions. After confirmation occurs, they start the actual
/// miner loop, where they first grab the best ranking fork to extend,
/// and start mining procedure for its next block. Additionally, they
/// listen to the network for new proposals, and check if these
/// proposals produce a new best ranking fork. If they do, the stop
/// mining. These two tasks run in parallel, and after one of them
/// finishes, node triggers confirmation check.
pub async fn miner_task(
    node: &DarkfiNodePtr,
    recipient_config: &MinerRewardsRecipientConfig,
    skip_sync: bool,
    ex: &ExecutorPtr,
) -> Result<()> {
    // Initialize miner configuration
    info!(target: "darkfid::task::miner_task", "Starting miner task...");

    // Grab zkas proving keys and bin for PoWReward transaction
    info!(target: "darkfid::task::miner_task", "Generating zkas bin and proving keys...");
    let (zkbin, _) = node.validator.blockchain.contracts.get_zkas(
        &node.validator.blockchain.sled_db,
        &MONEY_CONTRACT_ID,
        MONEY_CONTRACT_ZKAS_MINT_NS_V1,
    )?;
    let circuit = ZkCircuit::new(empty_witnesses(&zkbin)?, &zkbin);
    let pk = ProvingKey::build(zkbin.k, &circuit);

    // Generate a random master secret key, to derive all signing keys from.
    // This enables us to deanonimize proposals from reward recipient(miner).
    // TODO: maybe miner wants to keep this master secret so they can
    //       verify their signature in the future?
    info!(target: "darkfid::task::miner_task", "Generating signing key...");
    let mut secret = SecretKey::random(&mut OsRng);

    // Grab blocks subscriber
    let block_sub = node.subscribers.get("blocks").unwrap();

    // Grab proposals subscriber and subscribe to it
    let proposals_sub = node.subscribers.get("proposals").unwrap();
    let subscription = proposals_sub.publisher.clone().subscribe().await;

    // Listen for blocks until next confirmation, for optimal conditions
    if !skip_sync {
        info!(target: "darkfid::task::miner_task", "Waiting for next confirmation...");
        loop {
            subscription.receive().await;

            // Check if we can confirmation anything and broadcast them
            let confirmed = node.validator.confirmation().await?;

            if confirmed.is_empty() {
                continue
            }

            let mut notif_blocks = Vec::with_capacity(confirmed.len());
            for block in confirmed {
                notif_blocks
                    .push(JsonValue::String(base64::encode(&serialize_async(&block).await)));
            }
            block_sub.notify(JsonValue::Array(notif_blocks)).await;
            break;
        }
    }

    // Create channels so threads can signal each other
    let (sender, stop_signal) = smol::channel::bounded(1);

    // Create the garbage collection task using a dummy task
    let gc_task = StoppableTask::new();
    gc_task.clone().start(
        async { Ok(()) },
        |_| async { /* Do nothing */ },
        Error::GarbageCollectionTaskStopped,
        ex.clone(),
    );

    info!(target: "darkfid::task::miner_task", "Miner initialized successfully!");

    // Start miner loop
    loop {
        // Grab best current fork
        let forks = node.validator.consensus.forks.read().await;
        let index = match best_fork_index(&forks) {
            Ok(i) => i,
            Err(e) => {
                error!(
                    target: "darkfid::task::miner_task",
                    "Finding best fork index failed: {e}"
                );
                continue
            }
        };
        let extended_fork = match forks[index].full_clone() {
            Ok(f) => f,
            Err(e) => {
                error!(
                    target: "darkfid::task::miner_task",
                    "Fork full clone creation failed: {e}"
                );
                continue
            }
        };
        drop(forks);

        // Grab extended fork last proposal hash
        let last_proposal_hash = extended_fork.last_proposal()?.hash;

        // Start listenning for network proposals and mining next block for best fork.
        match smol::future::or(
            listen_to_network(node, last_proposal_hash, &subscription, &sender),
            mine(
                node,
                extended_fork,
                &mut secret,
                recipient_config,
                &zkbin,
                &pk,
                &stop_signal,
                skip_sync,
            ),
        )
        .await
        {
            Ok(_) => { /* Do nothing */ }
            Err(Error::NetworkNotConnected) => {
                error!(target: "darkfid::task::miner_task", "Node disconnected from the network");
                subscription.unsubscribe().await;
                return Err(Error::NetworkNotConnected)
            }
            Err(e) => {
                error!(
                    target: "darkfid::task::miner_task",
                    "Error during listen_to_network() or mine(): {e}"
                );
                continue
            }
        }

        // Check if we can confirm anything and broadcast them
        let confirmed = match node.validator.confirmation().await {
            Ok(f) => f,
            Err(e) => {
                error!(
                    target: "darkfid::task::miner_task",
                    "Confirmation failed: {e}"
                );
                continue
            }
        };

        if confirmed.is_empty() {
            continue
        }

        let mut notif_blocks = Vec::with_capacity(confirmed.len());
        for block in confirmed {
            notif_blocks.push(JsonValue::String(base64::encode(&serialize_async(&block).await)));
        }
        block_sub.notify(JsonValue::Array(notif_blocks)).await;

        // Invoke the detached garbage collection task
        gc_task.clone().stop().await;
        gc_task.clone().start(
            garbage_collect_task(node.clone()),
            |res| async {
                match res {
                    Ok(()) | Err(Error::GarbageCollectionTaskStopped) => { /* Do nothing */ }
                    Err(e) => {
                        error!(target: "darkfid", "Failed starting garbage collection task: {}", e)
                    }
                }
            },
            Error::GarbageCollectionTaskStopped,
            ex.clone(),
        );
    }
}

/// Async task to listen for incoming proposals and check if the best fork has changed.
async fn listen_to_network(
    node: &DarkfiNodePtr,
    last_proposal_hash: HeaderHash,
    subscription: &Subscription<JsonNotification>,
    sender: &Sender<()>,
) -> Result<()> {
    loop {
        // Wait until a new proposal has been received
        subscription.receive().await;

        // Grab a lock over node forks
        let forks = node.validator.consensus.forks.read().await;

        // Grab best current fork index
        let index = best_fork_index(&forks)?;

        // Verify if proposals sequence has changed
        if forks[index].last_proposal()?.hash != last_proposal_hash {
            drop(forks);
            break
        }

        drop(forks);
    }

    // Signal miner to abort mining
    sender.send(()).await?;
    if let Err(e) = node.miner_daemon_request("abort", &JsonValue::Array(vec![])).await {
        error!(target: "darkfid::task::miner::listen_to_network", "Failed to execute miner daemon abort request: {}", e);
    }

    Ok(())
}

/// Async task to generate and mine provided fork index next block,
/// while listening for a stop signal.
#[allow(clippy::too_many_arguments)]
async fn mine(
    node: &DarkfiNodePtr,
    extended_fork: Fork,
    secret: &mut SecretKey,
    recipient_config: &MinerRewardsRecipientConfig,
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    stop_signal: &Receiver<()>,
    skip_sync: bool,
) -> Result<()> {
    smol::future::or(
        wait_stop_signal(stop_signal),
        mine_next_block(node, extended_fork, secret, recipient_config, zkbin, pk, skip_sync),
    )
    .await
}

/// Async task to wait for listener's stop signal.
pub async fn wait_stop_signal(stop_signal: &Receiver<()>) -> Result<()> {
    // Clean stop signal channel
    if stop_signal.is_full() {
        stop_signal.recv().await?;
    }

    // Wait for listener signal
    stop_signal.recv().await?;

    Ok(())
}

/// Async task to generate and mine provided fork index next block.
async fn mine_next_block(
    node: &DarkfiNodePtr,
    mut extended_fork: Fork,
    secret: &mut SecretKey,
    recipient_config: &MinerRewardsRecipientConfig,
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    skip_sync: bool,
) -> Result<()> {
    // Grab next target and block
    let (next_target, mut next_block) = generate_next_block(
        &mut extended_fork,
        secret,
        recipient_config,
        zkbin,
        pk,
        node.validator.consensus.module.read().await.target,
        node.validator.verify_fees,
    )
    .await?;

    // Execute request to minerd and parse response
    let target = JsonValue::String(next_target.to_string());
    let block = JsonValue::String(base64::encode(&serialize_async(&next_block).await));
    let response =
        node.miner_daemon_request_with_retry("mine", &JsonValue::Array(vec![target, block])).await;
    next_block.header.nonce = *response.get::<f64>().unwrap() as u64;

    // Sign the mined block
    next_block.sign(secret);

    // Verify it
    extended_fork.module.verify_current_block(&next_block)?;

    // Check if we are connected to the network
    if !skip_sync && !node.p2p_handler.p2p.is_connected() {
        return Err(Error::NetworkNotConnected)
    }

    // Append the mined block as a proposal
    let proposal = Proposal::new(next_block);
    node.validator.append_proposal(&proposal).await?;

    // Broadcast proposal to the network
    let message = ProposalMessage(proposal);
    node.p2p_handler.p2p.broadcast(&message).await;

    Ok(())
}

/// Auxiliary function to generate next block in an atomic manner.
async fn generate_next_block(
    extended_fork: &mut Fork,
    secret: &mut SecretKey,
    recipient_config: &MinerRewardsRecipientConfig,
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    block_target: u32,
    verify_fees: bool,
) -> Result<(BigUint, BlockInfo)> {
    // Grab forks' last block proposal(previous)
    let last_proposal = extended_fork.last_proposal()?;

    // Grab forks' next block height
    let next_block_height = last_proposal.block.header.height + 1;

    // Grab forks' unproposed transactions
    let (mut txs, _, fees, overlay) = extended_fork
        .unproposed_txs(&extended_fork.blockchain, next_block_height, block_target, verify_fees)
        .await?;

    // We are deriving the next secret key for optimization.
    // Next secret is the poseidon hash of:
    //  [prefix, current(previous) secret, signing(block) height].
    let prefix = pallas::Base::from_raw([4, 0, 0, 0]);
    let next_secret = poseidon_hash([prefix, secret.inner(), (next_block_height as u64).into()]);
    *secret = SecretKey::from(next_secret);

    // Generate reward transaction
    let tx = generate_transaction(next_block_height, fees, secret, recipient_config, zkbin, pk)?;

    // Apply producer transaction in the overlay
    let _ = apply_producer_transaction(
        &overlay,
        next_block_height,
        block_target,
        &tx,
        &mut MerkleTree::new(1),
    )
    .await?;
    txs.push(tx);

    // Grab the updated contracts states root
    overlay.lock().unwrap().contracts.update_state_monotree(&mut extended_fork.state_monotree)?;
    let Some(state_root) = extended_fork.state_monotree.get_headroot()? else {
        return Err(Error::ContractsStatesRootNotFoundError);
    };

    // Drop new trees opened by the unproposed transactions overlay
    overlay.lock().unwrap().overlay.lock().unwrap().purge_new_trees()?;

    // Generate the new header
    let mut header =
        Header::new(last_proposal.hash, next_block_height, Timestamp::current_time(), 0);
    header.state_root = state_root;

    // Generate the block
    let mut next_block = BlockInfo::new_empty(header);

    // Add transactions to the block
    next_block.append_txs(txs);

    // Grab the next mine target
    let target = extended_fork.module.next_mine_target()?;

    Ok((target, next_block))
}

/// Auxiliary function to generate a Money::PoWReward transaction.
fn generate_transaction(
    block_height: u32,
    fees: u64,
    secret: &SecretKey,
    recipient_config: &MinerRewardsRecipientConfig,
    zkbin: &ZkBinary,
    pk: &ProvingKey,
) -> Result<Transaction> {
    // Build the transaction debris
    let debris = PoWRewardCallBuilder {
        signature_public: PublicKey::from_secret(*secret),
        block_height,
        fees,
        recipient: Some(recipient_config.recipient),
        spend_hook: recipient_config.spend_hook,
        user_data: recipient_config.user_data,
        mint_zkbin: zkbin.clone(),
        mint_pk: pk.clone(),
    }
    .build()?;

    // Generate and sign the actual transaction
    let mut data = vec![MoneyFunction::PoWRewardV1 as u8];
    debris.params.encode(&mut data)?;
    let call = ContractCall { contract_id: *MONEY_CONTRACT_ID, data };
    let mut tx_builder =
        TransactionBuilder::new(ContractCallLeaf { call, proofs: debris.proofs }, vec![])?;
    let mut tx = tx_builder.build()?;
    let sigs = tx.create_sigs(&[*secret])?;
    tx.signatures = vec![sigs];

    Ok(tx)
}
