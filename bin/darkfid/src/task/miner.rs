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

use std::sync::Arc;

use darkfi::{
    blockchain::BlockInfo,
    rpc::{jsonrpc::JsonNotification, util::JsonValue},
    system::{StoppableTask, Subscription},
    tx::{ContractCallLeaf, Transaction, TransactionBuilder},
    util::encoding::base64,
    validator::{
        consensus::{Fork, Proposal},
        utils::best_fork_index,
    },
    zk::{empty_witnesses, ProvingKey, ZkCircuit},
    zkas::ZkBinary,
    Error, Result,
};
use darkfi_money_contract::{
    client::pow_reward_v1::PoWRewardCallBuilder, MoneyFunction, MONEY_CONTRACT_ZKAS_MINT_NS_V1,
};
use darkfi_sdk::{
    crypto::{poseidon_hash, PublicKey, SecretKey, MONEY_CONTRACT_ID},
    pasta::pallas,
    ContractCall,
};
use darkfi_serial::{serialize_async, Encodable};
use log::{error, info};
use num_bigint::BigUint;
use rand::rngs::OsRng;
use smol::channel::{Receiver, Sender};

use crate::{proto::ProposalMessage, task::garbage_collect_task, Darkfid};

// TODO: handle all ? so the task don't stop on errors

/// Async task used for participating in the PoW block production.
/// Miner initializes their setup and waits for next finalization,
/// by listenning for new proposals from the network, for optimal
/// conditions. After finalization occurs, they start the actual
/// miner loop, where they first grab the best ranking fork to extend,
/// and start mining procedure for its next block. Additionally, they
/// listen to the network for new proposals, and check if these
/// proposals produce a new best ranking fork. If they do, the stop
/// mining. These two tasks run in parallel, and after one of them
/// finishes, node triggers finallization check.
pub async fn miner_task(
    node: Arc<Darkfid>,
    recipient: PublicKey,
    skip_sync: bool,
    ex: Arc<smol::Executor<'static>>,
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

    // Generate a new fork to be able to extend
    info!(target: "darkfid::task::miner_task", "Generating new empty fork...");
    node.validator.consensus.generate_empty_fork().await?;

    // Grab blocks subscriber
    let block_sub = node.subscribers.get("blocks").unwrap();

    // Grab proposals subscriber and subscribe to it
    let proposals_sub = node.subscribers.get("proposals").unwrap();
    let subscription = proposals_sub.sub.clone().subscribe().await;

    // Listen for blocks until next finalization, for optimal conditions
    if !skip_sync {
        info!(target: "darkfid::task::miner_task", "Waiting for next finalization...");
        loop {
            subscription.receive().await;

            // Check if we can finalize anything and broadcast them
            let finalized = node.validator.finalization().await?;
            if !finalized.is_empty() {
                let mut notif_blocks = Vec::with_capacity(finalized.len());
                for block in finalized {
                    notif_blocks
                        .push(JsonValue::String(base64::encode(&serialize_async(&block).await)));
                }
                block_sub.notify(JsonValue::Array(notif_blocks)).await;
                break;
            }
        }
    }

    // Create channels so threads can signal each other
    let (sender, stop_signal) = smol::channel::bounded(1);
    let (gc_sender, gc_stop_signal) = smol::channel::bounded(1);

    info!(target: "darkfid::task::miner_task", "Miner initialized successfully!");

    // Start miner loop
    loop {
        // Grab best current fork
        let forks = node.validator.consensus.forks.read().await;
        let extended_fork = forks[best_fork_index(&forks)?].full_clone()?;
        drop(forks);

        // Start listenning for network proposals and mining next block for best fork.
        smol::future::or(
            listen_to_network(&node, &extended_fork, &subscription, &sender),
            mine(&node, &extended_fork, &mut secret, &recipient, &zkbin, &pk, &stop_signal),
        )
        .await?;

        // Check if we can finalize anything and broadcast them
        let finalized = node.validator.finalization().await?;
        if !finalized.is_empty() {
            let mut notif_blocks = Vec::with_capacity(finalized.len());
            for block in finalized {
                notif_blocks
                    .push(JsonValue::String(base64::encode(&serialize_async(&block).await)));
            }
            block_sub.notify(JsonValue::Array(notif_blocks)).await;

            // Invoke detached garbage collection task
            gc_sender.send(()).await?;
            StoppableTask::new().start(
                garbage_collect_task(node.clone(), gc_stop_signal.clone()),
                |res| async {
                    match res {
                        Ok(()) => { /* Do nothing */ }
                        Err(e) => error!(target: "darkfid", "Failed starting garbage collection task: {}", e),
                    }
                },
                Error::MinerTaskStopped,
                ex.clone(),
            );
        }
    }
}

/// Async task to listen for incoming proposals and check if the best fork has changed.
async fn listen_to_network(
    node: &Darkfid,
    extended_fork: &Fork,
    subscription: &Subscription<JsonNotification>,
    sender: &Sender<()>,
) -> Result<()> {
    // Grab extended fork last proposal hash
    let last_proposal_hash = extended_fork.last_proposal()?.hash;
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
        error!(target: "darkfid::task::miner_task::listen_to_network", "Failed to execute miner daemon abort request: {}", e);
    }

    Ok(())
}

/// Async task to generate and mine provided fork index next block,
/// while listening for a stop signal.
async fn mine(
    node: &Darkfid,
    extended_fork: &Fork,
    secret: &mut SecretKey,
    recipient: &PublicKey,
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    stop_signal: &Receiver<()>,
) -> Result<()> {
    smol::future::or(
        wait_stop_signal(stop_signal),
        mine_next_block(node, extended_fork, secret, recipient, zkbin, pk),
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
    node: &Darkfid,
    extended_fork: &Fork,
    secret: &mut SecretKey,
    recipient: &PublicKey,
    zkbin: &ZkBinary,
    pk: &ProvingKey,
) -> Result<()> {
    // Grab next target and block
    let (next_target, mut next_block) =
        generate_next_block(extended_fork, secret, recipient, zkbin, pk).await?;

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

    // Append the mined block as a proposal
    let proposal = Proposal::new(next_block);
    node.validator.append_proposal(&proposal).await?;

    // Broadcast proposal to the network
    let message = ProposalMessage(proposal);
    node.p2p.broadcast(&message).await;

    Ok(())
}

/// Auxiliary function to generate next block in an atomic manner.
async fn generate_next_block(
    extended_fork: &Fork,
    secret: &mut SecretKey,
    recipient: &PublicKey,
    zkbin: &ZkBinary,
    pk: &ProvingKey,
) -> Result<(BigUint, BlockInfo)> {
    // Grab extended fork next block height
    let last_proposal = extended_fork.last_proposal()?;
    let next_block_height = last_proposal.block.header.height + 1;

    // We are deriving the next secret key for optimization.
    // Next secret is the poseidon hash of:
    //  [prefix, current(previous) secret, signing(block) height].
    let prefix = pallas::Base::from_raw([4, 0, 0, 0]);
    let next_secret = poseidon_hash([prefix, secret.inner(), (next_block_height as u64).into()]);
    *secret = SecretKey::from(next_secret);

    // Generate reward transaction
    let tx = generate_transaction(next_block_height, secret, recipient, zkbin, pk)?;

    // Generate next block proposal
    let target = extended_fork.module.next_mine_target()?;
    let next_block = extended_fork.generate_unsigned_block(tx).await?;

    Ok((target, next_block))
}

/// Auxiliary function to generate a Money::PoWReward transaction.
fn generate_transaction(
    block_height: u32,
    secret: &SecretKey,
    recipient: &PublicKey,
    zkbin: &ZkBinary,
    pk: &ProvingKey,
) -> Result<Transaction> {
    // We're just going to be using a zero spend-hook and user-data
    let spend_hook = pallas::Base::zero().into();
    let user_data = pallas::Base::zero();

    // Build the transaction debris
    let debris = PoWRewardCallBuilder {
        secret: *secret,
        recipient: *recipient,
        block_height,
        spend_hook,
        user_data,
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
