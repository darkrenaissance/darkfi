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

use darkfi::{
    blockchain::BlockInfo,
    rpc::{jsonrpc::JsonNotification, util::JsonValue},
    system::Subscription,
    tx::{ContractCallLeaf, Transaction, TransactionBuilder},
    util::encoding::base64,
    validator::{
        consensus::{Fork, Proposal},
        utils::best_forks_indexes,
    },
    zk::{empty_witnesses, ProvingKey, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use darkfi_money_contract::{
    client::pow_reward_v1::PoWRewardCallBuilder, MoneyFunction, MONEY_CONTRACT_ZKAS_MINT_NS_V1,
};
use darkfi_sdk::{
    crypto::{poseidon_hash, PublicKey, SecretKey, MONEY_CONTRACT_ID},
    pasta::pallas,
    ContractCall,
};
use darkfi_serial::{serialize, Encodable};
use log::info;
use num_bigint::BigUint;
use rand::rngs::OsRng;

use crate::{proto::ProposalMessage, Darkfid};

// TODO: handle all ? so the task don't stop on errors

/// async task used for participating in the PoW block production.
/// Miner initializes their setup and waits for next finalization,
/// by listenning for new proposals from the network, for optimal
/// conditions. After finalization occurs, they start the actual
/// miner loop, where they first grab the best ranking fork to extend,
/// and start mining procedure for its next block. Additionally, they
/// listen to the network for new proposals, and check if these
/// proposals produce a new best ranking fork. If they do, the stop
/// mining. These two tasks run in parallel, and after one of them
/// finishes, node triggers finallization check.
pub async fn miner_task(node: &Darkfid, recipient: &PublicKey, skip_sync: bool) -> Result<()> {
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
        loop {
            subscription.receive().await;

            // Check if we can finalize anything and broadcast them
            let finalized = node.validator.finalization().await?;
            if !finalized.is_empty() {
                let mut notif_blocks = Vec::with_capacity(finalized.len());
                for block in finalized {
                    notif_blocks.push(JsonValue::String(base64::encode(&serialize(&block))));
                }
                block_sub.notify(JsonValue::Array(notif_blocks)).await;
                break;
            }
        }
    }

    // Start miner loop
    loop {
        // Grab best current fork index
        let fork_index = best_forks_indexes(&node.validator.consensus.forks.read().await)?[0];

        // Start listenning for network proposals and mining next block for best fork.
        smol::future::or(
            listen_to_network(node, fork_index, &subscription),
            mine_next_block(node, fork_index, &mut secret, recipient, &zkbin, &pk),
        )
        .await?;

        // Check if we can finalize anything and broadcast them
        let finalized = node.validator.finalization().await?;
        if !finalized.is_empty() {
            let mut notif_blocks = Vec::with_capacity(finalized.len());
            for block in finalized {
                notif_blocks.push(JsonValue::String(base64::encode(&serialize(&block))));
            }
            block_sub.notify(JsonValue::Array(notif_blocks)).await;
        }
    }
}

/// Auxiliary function to listen for incoming proposals and check if the best fork has changed
async fn listen_to_network(
    node: &Darkfid,
    fork_index: usize,
    subscription: &Subscription<JsonNotification>,
) -> Result<()> {
    loop {
        // Wait until a new proposal has been received
        subscription.receive().await;

        // Grab best current fork indexes
        let fork_indexes = best_forks_indexes(&node.validator.consensus.forks.read().await)?;

        if !fork_indexes.contains(&fork_index) {
            return Ok(())
        }
    }
}

/// Auxiliary function to generate and mine provided fork index next block
async fn mine_next_block(
    node: &Darkfid,
    fork_index: usize,
    secret: &mut SecretKey,
    recipient: &PublicKey,
    zkbin: &ZkBinary,
    pk: &ProvingKey,
) -> Result<()> {
    // Grab next target and block
    let (next_target, mut next_block) =
        generate_next_block(node, fork_index, secret, recipient, zkbin, pk).await?;

    // Execute request to minerd and parse response
    let target = JsonValue::String(next_target.to_string());
    let block = JsonValue::String(base64::encode(&serialize(&next_block)));
    let response = node.miner_daemon_request("mine", JsonValue::Array(vec![target, block])).await?;
    next_block.header.nonce = *response.get::<f64>().unwrap() as u64;

    // Sign the mined block
    next_block.sign(secret)?;

    // Verify it
    node.validator.consensus.module.read().await.verify_current_block(&next_block)?;

    // Append the mined block as a proposal
    let proposal = Proposal::new(next_block)?;
    node.validator.consensus.append_proposal(&proposal).await?;

    // Broadcast proposal to the network
    let message = ProposalMessage(proposal);
    node.miners_p2p.as_ref().unwrap().broadcast(&message).await;
    node.sync_p2p.broadcast(&message).await;

    Ok(())
}

/// Auxiliary function to generate next block in an atomic manner
async fn generate_next_block(
    node: &Darkfid,
    fork_index: usize,
    secret: &mut SecretKey,
    recipient: &PublicKey,
    zkbin: &ZkBinary,
    pk: &ProvingKey,
) -> Result<(BigUint, BlockInfo)> {
    // Grab a lock over nodes' current forks
    let forks = node.validator.consensus.forks.read().await;

    // Grab best current fork
    let fork = &forks[fork_index];

    // Generate new signing key for next block
    let next_block_height = fork.get_next_block_height()?;
    // We are deriving the next secret key for optimization.
    // Next secret is the poseidon hash of:
    //  [prefix, current(previous) secret, signing(block) height].
    let prefix = pallas::Base::from_raw([4, 0, 0, 0]);
    let next_secret = poseidon_hash([prefix, secret.inner(), next_block_height.into()]);
    *secret = SecretKey::from(next_secret);

    // Generate reward transaction
    let tx = generate_transaction(fork, secret, recipient, zkbin, pk, next_block_height)?;

    // Generate next block proposal
    let target = fork.module.next_mine_target()?;
    let next_block = node.validator.consensus.generate_unsigned_block(fork, tx).await?;

    // Drop forks lock
    drop(forks);

    Ok((target, next_block))
}

/// Auxiliary function to generate a Money::PoWReward transaction
fn generate_transaction(
    fork: &Fork,
    secret: &SecretKey,
    recipient: &PublicKey,
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    block_height: u64,
) -> Result<Transaction> {
    // Grab extended proposal info
    let last_proposal = fork.last_proposal()?;
    let last_nonce = last_proposal.block.header.nonce;
    let fork_previous_hash = last_proposal.block.header.previous;

    // We're just going to be using a zero spend-hook and user-data
    let spend_hook = pallas::Base::zero().into();
    let user_data = pallas::Base::zero();

    // Build the transaction debris
    let debris = PoWRewardCallBuilder {
        secret: *secret,
        recipient: *recipient,
        block_height,
        last_nonce,
        fork_previous_hash,
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
