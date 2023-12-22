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

use darkfi::{
    blockchain::BlockInfo,
    tx::{ContractCallLeaf, Transaction, TransactionBuilder},
    validator::{
        consensus::{Fork, Proposal},
        pow::PoWModule,
        utils::best_forks_indexes,
    },
    zk::{empty_witnesses, ProvingKey, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use darkfi_consensus_contract::model::SECRET_KEY_PREFIX;
use darkfi_money_contract::{
    client::pow_reward_v1::PoWRewardCallBuilder, MoneyFunction, MONEY_CONTRACT_ZKAS_MINT_NS_V1,
};
use darkfi_sdk::{
    crypto::{poseidon_hash, PublicKey, SecretKey, MONEY_CONTRACT_ID},
    pasta::pallas,
    ContractCall,
};
use darkfi_serial::Encodable;
use log::info;
use rand::rngs::OsRng;
use smol::channel::Receiver;

use crate::{proto::BlockInfoMessage, Darkfid};

// TODO: handle all ? so the task don't stop on errors

/// async task used for participating in the PoW consensus protocol
pub async fn miner_task(
    node: &Darkfid,
    recipient: &PublicKey,
    stop_signal: &Receiver<()>,
) -> Result<()> {
    // TODO: For now we asume we have a single miner that produces block,
    //       until the PoW consensus and proper validations have been added.
    //       The miner workflow would be:
    //          First we wait for next finalization, for optimal conditions.
    //          After that we ask all our connected peers for their blocks,
    //          and append them to our consensus state, creating their forks.
    //          Then we evaluate each fork and find the best one, so we can
    //          mine its next.
    //          We start running 2 tasks, one listenning for blocks(proposals)
    //          from other miners, and one mining the best fork next block.
    //          These two tasks run in parallel. If we receive a block from
    //          another miner, we evaluate it and if it produces a higher
    //          ranking fork that the one we currectly mine, we stop, check
    //          if we can finalize any fork, and then start mining that fork
    //          next block. If we manage to mine the block next, we broadcast
    //          it and then execute the finalization check and start mining
    //          next best fork block.
    info!(target: "darkfid::task::miner_task", "Starting miner task...");

    // Start miner loop
    miner_loop(node, recipient, stop_signal).await?;

    Ok(())
}

/// Miner loop
async fn miner_loop(
    node: &Darkfid,
    recipient: &PublicKey,
    stop_signal: &Receiver<()>,
) -> Result<()> {
    // Grab zkas proving keys and bin for PoWReward transaction
    info!(target: "darkfid::task::miner_task", "Generating zkas bin and proving keys...");
    let blockchain = node.validator.blockchain.clone();
    let (zkbin, _) = blockchain.contracts.get_zkas(
        &blockchain.sled_db,
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
    node.validator.consensus.generate_pow_slot().await?;

    info!(target: "darkfid::task::miner_task", "Miner loop starts!");
    // Miner loop
    loop {
        // Grab next block
        let (mut next_block, module) =
            generate_next_block(node, &mut secret, recipient, &zkbin, &pk).await?;
        module.mine_block(&mut next_block, stop_signal)?;

        // Sign the mined block
        next_block.sign(&secret)?;

        // Verify it
        node.validator.consensus.module.read().await.verify_current_block(&next_block)?;

        // Append the mined block as a proposal
        let proposal = Proposal::new(next_block)?;
        node.validator.consensus.append_proposal(&proposal).await?;

        // Check if we can finalize anything and broadcast them
        let finalized = node.validator.finalization().await?;
        if !finalized.is_empty() {
            for block in finalized {
                let message = BlockInfoMessage::from(&block);
                node.sync_p2p.broadcast(&message).await;
            }
        }
    }
}

/// Auxiliary function to generate next block in an atomic manner
async fn generate_next_block(
    node: &Darkfid,
    secret: &mut SecretKey,
    recipient: &PublicKey,
    zkbin: &ZkBinary,
    pk: &ProvingKey,
) -> Result<(BlockInfo, PoWModule)> {
    // Grab a lock over nodes' current forks
    let forks = node.validator.consensus.forks.read().await;

    // Grab best current fork
    let fork_index = best_forks_indexes(&forks)?[0];
    let fork = &forks[fork_index];

    // Generate new signing key for next block
    let height = fork.slots.last().unwrap().id;
    // We are deriving the next secret key for optimization.
    // Next secret is the poseidon hash of:
    //  [prefix, current(previous) secret, signing(block) height].
    let next_secret = poseidon_hash([SECRET_KEY_PREFIX, secret.inner(), height.into()]);
    *secret = SecretKey::from(next_secret);

    // Generate reward transaction
    let tx = generate_pow_transaction(fork, secret, recipient, zkbin, pk)?;

    // Generate next block proposal
    let next_block = node.validator.consensus.generate_unsigned_block(fork, tx).await?;
    let module = fork.module.clone();

    // Drop forks lock
    drop(forks);

    Ok((next_block, module))
}

/// Auxiliary function to generate a Money::PoWReward transaction
fn generate_pow_transaction(
    fork: &Fork,
    secret: &SecretKey,
    recipient: &PublicKey,
    zkbin: &ZkBinary,
    pk: &ProvingKey,
) -> Result<Transaction> {
    // Grab next block height
    let block_height = fork.slots.last().unwrap().id;

    // Grab extended proposal info
    let last_proposal = fork.last_proposal()?;
    let last_nonce = last_proposal.block.header.nonce;
    let fork_hash = last_proposal.hash;
    let fork_previous_hash = last_proposal.block.header.previous;

    // We're just going to be using a zero spend-hook and user-data
    let spend_hook = pallas::Base::zero();
    let user_data = pallas::Base::zero();

    // Build the transaction debris
    let debris = PoWRewardCallBuilder {
        secret: *secret,
        recipient: *recipient,
        block_height,
        last_nonce,
        fork_hash,
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
    let sigs = tx.create_sigs(&mut OsRng, &[*secret])?;
    tx.signatures = vec![sigs];

    Ok(tx)
}
