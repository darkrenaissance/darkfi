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

use std::collections::HashMap;

use darkfi_sdk::{
    blockchain::{block_version, expected_reward},
    crypto::{
        schnorr::SchnorrPublic, ContractId, PublicKey, CONSENSUS_CONTRACT_ID,
        DEPLOYOOOR_CONTRACT_ID, MONEY_CONTRACT_ID,
    },
    dark_tree::dark_forest_leaf_vec_integrity_check,
    deploy::DeployParamsV1,
    pasta::pallas,
};
use darkfi_serial::{
    deserialize_async, serialize_async, AsyncDecodable, AsyncEncodable, AsyncWriteExt, WriteExt,
};
use log::{debug, error, warn};
use smol::io::Cursor;

use crate::{
    blockchain::{BlockInfo, BlockchainOverlayPtr},
    error::TxVerifyFailed,
    runtime::vm_runtime::Runtime,
    tx::{Transaction, MAX_TX_CALLS, MIN_TX_CALLS},
    util::time::TimeKeeper,
    validator::{
        consensus::{Consensus, Fork, Proposal, TXS_CAP},
        fees::PALLAS_SCHNORR_SIGNATURE_FEE,
        pow::PoWModule,
        validation::validate_block,
    },
    zk::VerifyingKey,
    Error, Result,
};

/// Verify given genesis [`BlockInfo`], and apply it to the provided overlay
pub async fn verify_genesis_block(
    overlay: &BlockchainOverlayPtr,
    time_keeper: &TimeKeeper,
    block: &BlockInfo,
    genesis_txs_total: u64,
) -> Result<()> {
    let block_hash = block.hash()?.to_string();
    debug!(target: "validator::verification::verify_genesis_block", "Validating genesis block {}", block_hash);

    // Check if block already exists
    if overlay.lock().unwrap().has_block(block)? {
        return Err(Error::BlockAlreadyExists(block_hash))
    }

    // Block height must be 0
    if block.header.height != 0 {
        return Err(Error::BlockIsInvalid(block_hash))
    }

    // Block height must be the same as the time keeper verifying slot
    if block.header.height != time_keeper.verifying_slot {
        return Err(Error::VerifyingSlotMissmatch())
    }

    // Check genesis slot exist
    if block.slots.len() != 1 {
        return Err(Error::BlockIsInvalid(block_hash))
    }

    // Retrieve genesis slot
    let genesis_slot = block.slots.last().unwrap();

    // Genesis block slot total token must correspond to the total
    // of all genesis transactions public inputs (genesis distribution).
    if genesis_slot.total_tokens != genesis_txs_total {
        return Err(Error::SlotIsInvalid(genesis_slot.id))
    }

    // Verify there is no reward
    if genesis_slot.reward != 0 {
        return Err(Error::SlotIsInvalid(genesis_slot.id))
    }

    // Verify transactions vector contains at least one(producers) transaction
    if block.txs.is_empty() {
        return Err(Error::BlockContainsNoTransactions(block_hash))
    }

    // Insert genesis slot so transactions can be validated against.
    // Since an overlay is used, original database is not affected.
    overlay.lock().unwrap().slots.insert(&[genesis_slot.clone()])?;

    // Genesis transaction must be the Transaction::default() one(empty)
    if block.txs[0] != Transaction::default() {
        error!(target: "validator::verification::verify_genesis_block", "Genesis proposal transaction is not default one");
        return Err(TxVerifyFailed::ErroneousTxs(vec![block.txs[0].clone()]).into())
    }

    // Verify transactions, exluding producer(first) one
    let txs = &block.txs[1..];
    let erroneous_txs = verify_transactions(overlay, time_keeper, txs, false).await?;
    if !erroneous_txs.is_empty() {
        warn!(target: "validator::verification::verify_genesis_block", "Erroneous transactions found in set");
        overlay.lock().unwrap().overlay.lock().unwrap().purge_new_trees()?;
        return Err(TxVerifyFailed::ErroneousTxs(erroneous_txs).into())
    }

    // Insert block
    overlay.lock().unwrap().add_block(block)?;

    debug!(target: "validator::verification::verify_genesis_block", "Genesis block {} verified successfully", block_hash);
    Ok(())
}

/// Verify given [`BlockInfo`], and apply it to the provided overlay
pub async fn verify_block(
    overlay: &BlockchainOverlayPtr,
    time_keeper: &TimeKeeper,
    module: &PoWModule,
    block: &BlockInfo,
    previous: &BlockInfo,
    expected_reward: u64,
    pos_testing_mode: bool,
) -> Result<()> {
    let block_hash = block.hash()?.to_string();
    debug!(target: "validator::verification::verify_block", "Validating block {}", block_hash);

    // Check if block already exists
    if overlay.lock().unwrap().has_block(block)? {
        return Err(Error::BlockAlreadyExists(block_hash))
    }

    // Block height must be the same as the time keeper verifying slot
    if block.header.height != time_keeper.verifying_slot {
        return Err(Error::VerifyingSlotMissmatch())
    }

    // Block epoch must be the correct one, calculated by the time keeper configuration
    if block.header.epoch != time_keeper.slot_epoch(block.header.height) {
        return Err(Error::VerifyingSlotMissmatch())
    }

    // Validate block, using its previous
    validate_block(block, previous, expected_reward, module)?;

    // Verify transactions vector contains at least one(producers) transaction
    if block.txs.is_empty() {
        return Err(Error::BlockContainsNoTransactions(block_hash))
    }

    // Insert last block slot so transactions can be validated against.
    // Rest (empty) slots will be inserted along with the block.
    // Since an overlay is used, original database is not affected.
    overlay.lock().unwrap().slots.insert(&[block.slots.last().unwrap().clone()])?;

    // Verify proposal transaction.
    // For PoS blocks(version 2) verify if not in PoS testing mode.
    if block.header.version != 2 || !pos_testing_mode {
        let public_key =
            verify_producer_transaction(overlay, time_keeper, &block.txs[0], block.header.version)
                .await?;
        verify_producer_signature(block, &public_key)?;
    }

    // Verify transactions, exluding producer(first) one
    let txs = &block.txs[1..];
    let erroneous_txs = verify_transactions(overlay, time_keeper, txs, false).await?;
    if !erroneous_txs.is_empty() {
        warn!(target: "validator::verification::verify_block", "Erroneous transactions found in set");
        overlay.lock().unwrap().overlay.lock().unwrap().purge_new_trees()?;
        return Err(TxVerifyFailed::ErroneousTxs(erroneous_txs).into())
    }

    // Insert block
    overlay.lock().unwrap().add_block(block)?;

    debug!(target: "validator::verification::verify_block", "Block {} verified successfully", block_hash);
    Ok(())
}

/// Verify block proposer signature, using the proposal transaction signature as signing key
/// over blocks header hash.
pub fn verify_producer_signature(block: &BlockInfo, public_key: &PublicKey) -> Result<()> {
    if !public_key.verify(&block.header.hash()?.as_bytes()[..], &block.signature) {
        warn!(target: "validator::verification::verify_producer_signature", "Proposer {} signature could not be verified", public_key);
        return Err(Error::InvalidSignature)
    }

    Ok(())
}

/// Verify WASM execution, signatures, and ZK proofs for a given producer [`Transaction`],
/// and apply it to the provided overlay. Returns transaction signature public key.
pub async fn verify_producer_transaction(
    overlay: &BlockchainOverlayPtr,
    time_keeper: &TimeKeeper,
    tx: &Transaction,
    block_version: u8,
) -> Result<PublicKey> {
    let tx_hash = tx.hash()?;
    debug!(target: "validator::verification::verify_producer_transaction", "Validating proposal transaction {}", tx_hash);

    // Producer transactions must contain a single, non-empty call
    if tx.calls.len() != 1 || tx.calls[0].data.data.is_empty() {
        return Err(TxVerifyFailed::ErroneousTxs(vec![tx.clone()]).into())
    }

    // Verify call based on version
    let call = &tx.calls[0];
    match block_version {
        1 => {
            // Version 1 blocks must contain a Money::PoWReward(0x08) call
            if call.data.contract_id != *MONEY_CONTRACT_ID || call.data.data[0] != 0x08 {
                return Err(TxVerifyFailed::ErroneousTxs(vec![tx.clone()]).into())
            }
        }
        2 => {
            // Version 2 blocks must contain a Consensus::Proposal(0x02) call
            if call.data.contract_id != *CONSENSUS_CONTRACT_ID || call.data.data[0] != 0x02 {
                return Err(TxVerifyFailed::ErroneousTxs(vec![tx.clone()]).into())
            }
        }
        _ => return Err(Error::BlockVersionIsInvalid(block_version)),
    }

    // Map of ZK proof verifying keys for the current transaction
    let mut verifying_keys: HashMap<[u8; 32], HashMap<String, VerifyingKey>> = HashMap::new();

    // Initialize the map
    verifying_keys.insert(call.data.contract_id.to_bytes(), HashMap::new());

    // Table of public inputs used for ZK proof verification
    let mut zkp_table = vec![];
    // Table of public keys used for signature verification
    let mut sig_table = vec![];

    debug!(target: "validator::verification::verify_producer_transaction", "Executing contract call");

    // Write the actual payload data
    let mut payload = vec![];
    payload.write_u32_async(0).await?; // Call index
    tx.calls.encode_async(&mut payload).await?; // Actual call data

    debug!(target: "validator::verification::verify_producer_transaction", "Instantiating WASM runtime");
    let wasm = overlay.lock().unwrap().wasm_bincode.get(call.data.contract_id)?;

    let mut runtime =
        Runtime::new(&wasm, overlay.clone(), call.data.contract_id, time_keeper.clone())?;

    debug!(target: "validator::verification::verify_producer_transaction", "Executing \"metadata\" call");
    let metadata = runtime.metadata(&payload)?;

    // Decode the metadata retrieved from the execution
    let mut decoder = Cursor::new(&metadata);

    // The tuple is (zkas_ns, public_inputs)
    let zkp_pub: Vec<(String, Vec<pallas::Base>)> =
        AsyncDecodable::decode_async(&mut decoder).await?;
    let sig_pub: Vec<PublicKey> = AsyncDecodable::decode_async(&mut decoder).await?;

    // Check that only one ZK proof and signature public key exist
    if zkp_pub.len() != 1 || sig_pub.len() != 1 {
        error!(target: "validator::verification::verify_producer_transaction", "Proposal contains multiple ZK proofs or signature public keys");
        return Err(TxVerifyFailed::ErroneousTxs(vec![tx.clone()]).into())
    }

    // TODO: Make sure we've read all the bytes above.
    debug!(target: "validator::verification::verify_producer_transaction", "Successfully executed \"metadata\" call");

    // Here we'll look up verifying keys and insert them into the map.
    debug!(target: "validator::verification::verify_producer_transaction", "Performing VerifyingKey lookups from the sled db");
    for (zkas_ns, _) in &zkp_pub {
        // TODO: verify this is correct behavior
        let inner_vk_map = verifying_keys.get_mut(&call.data.contract_id.to_bytes()).unwrap();
        if inner_vk_map.contains_key(zkas_ns.as_str()) {
            continue
        }
        let (_, vk) =
            overlay.lock().unwrap().contracts.get_zkas(&call.data.contract_id, zkas_ns)?;
        inner_vk_map.insert(zkas_ns.to_string(), vk);
    }

    zkp_table.push(zkp_pub);
    let signature_public_key = *sig_pub.last().unwrap();
    sig_table.push(sig_pub);

    // After getting the metadata, we run the "exec" function with the same runtime
    // and the same payload.
    debug!(target: "validator::verification::verify_producer_transaction", "Executing \"exec\" call");
    let state_update = runtime.exec(&payload)?;
    debug!(target: "validator::verification::verify_producer_transaction", "Successfully executed \"exec\" call");

    // If that was successful, we apply the state update in the ephemeral overlay.
    debug!(target: "validator::verification::verify_producer_transaction", "Executing \"apply\" call");
    runtime.apply(&state_update)?;
    debug!(target: "validator::verification::verify_producer_transaction", "Successfully executed \"apply\" call");

    // When we're done executing over the tx's contract call, we now move on with verification.
    // First we verify the signatures as that's cheaper, and then finally we verify the ZK proofs.
    debug!(target: "validator::verification::verify_producer_transaction", "Verifying signatures for transaction {}", tx_hash);
    if sig_table.len() != tx.signatures.len() {
        error!(target: "validator::verification::verify_producer_transaction", "Incorrect number of signatures in tx {}", tx_hash);
        return Err(TxVerifyFailed::MissingSignatures.into())
    }

    // TODO: Go through the ZK circuits that have to be verified and account for the opcodes.

    if let Err(e) = tx.verify_sigs(sig_table) {
        error!(target: "validator::verification::verify_producer_transaction", "Signature verification for tx {} failed: {}", tx_hash, e);
        return Err(TxVerifyFailed::InvalidSignature.into())
    }

    debug!(target: "validator::verification::verify_producer_transaction", "Signature verification successful");

    debug!(target: "validator::verification::verify_producer_transaction", "Verifying ZK proofs for transaction {}", tx_hash);
    if let Err(e) = tx.verify_zkps(&verifying_keys, zkp_table).await {
        error!(target: "validator::verification::verify_proposal_transaction", "ZK proof verification for tx {} failed: {}", tx_hash, e);
        return Err(TxVerifyFailed::InvalidZkProof.into())
    }

    debug!(target: "validator::verification::verify_producer_transaction", "ZK proof verification successful");
    debug!(target: "validator::verification::verify_producer_transaction", "Proposal transaction {} verified successfully", tx_hash);

    Ok(signature_public_key)
}

/// Verify WASM execution, signatures, and ZK proofs for a given [`Transaction`],
/// and apply it to the provided overlay.
pub async fn verify_transaction(
    overlay: &BlockchainOverlayPtr,
    time_keeper: &TimeKeeper,
    tx: &Transaction,
    verifying_keys: &mut HashMap<[u8; 32], HashMap<String, VerifyingKey>>,
    verify_fee: bool,
) -> Result<u64> {
    let tx_hash = tx.hash()?;
    debug!(target: "validator::verification::verify_transaction", "Validating transaction {}", tx_hash);

    // Gas accumulator
    let mut gas_used = 0;

    // Verify calls indexes integrity
    if verify_fee {
        dark_forest_leaf_vec_integrity_check(
            &tx.calls,
            Some(MIN_TX_CALLS + 1),
            Some(MAX_TX_CALLS),
        )?;
    } else {
        dark_forest_leaf_vec_integrity_check(&tx.calls, Some(MIN_TX_CALLS), Some(MAX_TX_CALLS))?;
    }

    // Table of public inputs used for ZK proof verification
    let mut zkp_table = vec![];
    // Table of public keys used for signature verification
    let mut sig_table = vec![];

    // Index of the Fee-paying call
    let mut fee_call_idx = 0;

    if verify_fee {
        let mut found_fee = false;
        // Verify that there is a Money::FeeV1 (0x00) call in the transaction
        for (call_idx, call) in tx.calls.iter().enumerate() {
            if call.data.contract_id == *MONEY_CONTRACT_ID && call.data.data[0] == 0x00 {
                found_fee = true;
                fee_call_idx = call_idx;
                break
            }
        }

        if !found_fee {
            error!(
                target: "validator::verification::verify_transcation",
                "[VALIDATOR] Transaction {} does not contain fee payment call", tx_hash,
            );
            return Err(TxVerifyFailed::InvalidFee.into())
        }
    }

    // Iterate over all calls to get the metadata
    for (idx, call) in tx.calls.iter().enumerate() {
        // Transaction must not contain a reward call, Money::PoWReward(0x08) or Consensus::Proposal(0x02)
        if (call.data.contract_id == *MONEY_CONTRACT_ID && call.data.data[0] == 0x08) ||
            (call.data.contract_id == *CONSENSUS_CONTRACT_ID && call.data.data[0] == 0x02)
        {
            error!(target: "validator::verification::verify_transaction", "Reward transaction detected");
            return Err(TxVerifyFailed::ErroneousTxs(vec![tx.clone()]).into())
        }

        debug!(target: "validator::verification::verify_transaction", "Executing contract call {}", idx);

        // Write the actual payload data
        let mut payload = vec![];
        payload.write_u32(idx as u32)?; // Call index
        tx.calls.encode_async(&mut payload).await?; // Actual call data

        debug!(target: "validator::verification::verify_transaction", "Instantiating WASM runtime");
        let wasm = overlay.lock().unwrap().wasm_bincode.get(call.data.contract_id)?;

        let mut runtime =
            Runtime::new(&wasm, overlay.clone(), call.data.contract_id, time_keeper.clone())?;

        debug!(target: "validator::verification::verify_transaction", "Executing \"metadata\" call");
        let metadata = runtime.metadata(&payload)?;

        // Decode the metadata retrieved from the execution
        let mut decoder = Cursor::new(&metadata);

        // The tuple is (zkas_ns, public_inputs)
        let zkp_pub: Vec<(String, Vec<pallas::Base>)> =
            AsyncDecodable::decode_async(&mut decoder).await?;
        let sig_pub: Vec<PublicKey> = AsyncDecodable::decode_async(&mut decoder).await?;

        if decoder.position() != metadata.len() as u64 {
            error!(
                target: "validator::verification::verify_transaction",
                "[VALIDATOR] Failed decoding entire metadata buffer for {}:{}", tx_hash, idx,
            );
            return Err(TxVerifyFailed::ErroneousTxs(vec![tx.clone()]).into())
        }

        debug!(target: "validator::verification::verify_transaction", "Successfully executed \"metadata\" call");

        // Here we'll look up verifying keys and insert them into the per-contract map.
        // TODO: This can potentially use a lot of RAM. Perhaps load keys on-demand at verification time?
        debug!(target: "validator::verification::verify_transaction", "Performing VerifyingKey lookups from the sled db");
        for (zkas_ns, _) in &zkp_pub {
            let inner_vk_map = verifying_keys.get_mut(&call.data.contract_id.to_bytes()).unwrap();

            // TODO: This will be a problem in case of ::deploy, unless we force a different
            // namespace and disable updating existing circuit. Might be a smart idea to do
            // so in order to have to care less about being able to verify historical txs.
            if inner_vk_map.contains_key(zkas_ns.as_str()) {
                continue
            }

            let (_, vk) =
                overlay.lock().unwrap().contracts.get_zkas(&call.data.contract_id, zkas_ns)?;

            inner_vk_map.insert(zkas_ns.to_string(), vk);
        }

        zkp_table.push(zkp_pub);
        sig_table.push(sig_pub);

        // After getting the metadata, we run the "exec" function with the same runtime
        // and the same payload.
        debug!(target: "validator::verification::verify_transaction", "Executing \"exec\" call");
        let state_update = runtime.exec(&payload)?;
        debug!(target: "validator::verification::verify_transaction", "Successfully executed \"exec\" call");

        // If that was successful, we apply the state update in the ephemeral overlay.
        debug!(target: "validator::verification::verify_transaction", "Executing \"apply\" call");
        runtime.apply(&state_update)?;
        debug!(target: "validator::verification::verify_transaction", "Successfully executed \"apply\" call");

        // If this call is supposed to deploy a new contract, we have to instantiate
        // a new `Runtime` and run its deploy function.
        if call.data.contract_id == *DEPLOYOOOR_CONTRACT_ID && call.data.data[0] == 0x00
        /* DeployV1 */
        {
            debug!(target: "validator::verification::verify_transaction", "Deploying new contract");
            // Deserialize the deployment parameters
            let deploy_params: DeployParamsV1 = deserialize_async(&call.data.data[1..]).await?;
            let deploy_cid = ContractId::derive_public(deploy_params.public_key);

            // Instantiate the new deployment runtime
            let mut deploy_runtime = Runtime::new(
                &deploy_params.wasm_bincode,
                overlay.clone(),
                deploy_cid,
                time_keeper.clone(),
            )?;

            deploy_runtime.deploy(&deploy_params.ix)?;

            // Append the used gas
            gas_used += deploy_runtime.gas_used();
        }

        // At this point we're done with the call and move on to the next one.
        // Accumulate the WASM gas used.
        gas_used += runtime.gas_used();
    }

    if verify_fee {
        // The signature fee is tx_size + fixed_sig_fee * n_signatures
        gas_used += (PALLAS_SCHNORR_SIGNATURE_FEE * tx.signatures.len() as u64) +
            serialize_async(tx).await.len() as u64;

        // TODO: This counts 1 gas as 1 token unit. Pricing should be better specified.
        // TODO: Currently this doesn't account for ZK proofs

        // Deserialize the fee call to find the paid fee
        let fee: u64 = match deserialize_async(&tx.calls[fee_call_idx].data.data[1..9]).await {
            Ok(v) => v,
            Err(e) => {
                error!(
                    target: "validator::verification::verify_transaction",
                    "[VALIDATOR] Failed deserializing tx {} fee call: {}", tx_hash, e,
                );
                return Err(TxVerifyFailed::InvalidFee.into())
            }
        };

        // Check that enough fee has been paid for the used gas in this transaction.
        if gas_used > fee {
            error!(
                target: "validator::verification::verify_transaction",
                "[VALIDATOR] Transaction {} has insufficient fee. Required: {}, Paid: {}",
                tx_hash, gas_used, fee,
            );
            return Err(TxVerifyFailed::InsufficientFee.into())
        }
    }

    // When we're done looping and executing over the tx's contract calls and
    // (optionally) made sure that enough fee was paid, we now move on with
    // verification. First we verify the transaction signatures and then we
    // verify any accompanying ZK proofs.
    debug!(target: "validator::verification::verify_transaction", "Verifying signatures for transaction {}", tx_hash);
    if sig_table.len() != tx.signatures.len() {
        error!(target: "validator::verification::verify_transaction", "Incorrect number of signatures in tx {}", tx_hash);
        return Err(TxVerifyFailed::MissingSignatures.into())
    }

    if let Err(e) = tx.verify_sigs(sig_table) {
        error!(target: "validator::verification::verify_transaction", "Signature verification for tx {} failed: {}", tx_hash, e);
        return Err(TxVerifyFailed::InvalidSignature.into())
    }

    debug!(target: "validator::verification::verify_transaction", "Signature verification successful");

    debug!(target: "validator::verification::verify_transaction", "Verifying ZK proofs for transaction {}", tx_hash);
    if let Err(e) = tx.verify_zkps(verifying_keys, zkp_table).await {
        error!(target: "validator::verification::verify_transaction", "ZK proof verification for tx {} failed: {}", tx_hash, e);
        return Err(TxVerifyFailed::InvalidZkProof.into())
    }

    debug!(target: "validator::verification::verify_transaction", "ZK proof verification successful");
    debug!(target: "validator::verification::verify_transaction", "Transaction {} verified successfully", tx_hash);

    Ok(gas_used)
}

/// Verify a set of [`Transaction`] in sequence and apply them if all are valid.
/// In case any of the transactions fail, they will be returned to the caller.
pub async fn verify_transactions(
    overlay: &BlockchainOverlayPtr,
    time_keeper: &TimeKeeper,
    txs: &[Transaction],
    verify_fees: bool,
) -> Result<Vec<Transaction>> {
    debug!(target: "validator::verification::verify_transactions", "Verifying {} transactions", txs.len());

    // Tracker for failed txs
    let mut erroneous_txs = vec![];

    // Gas accumulator
    let mut _gas_used = 0;

    // Map of ZK proof verifying keys for the current transaction batch
    let mut vks: HashMap<[u8; 32], HashMap<String, VerifyingKey>> = HashMap::new();

    // Initialize the map
    for tx in txs {
        for call in &tx.calls {
            vks.insert(call.data.contract_id.to_bytes(), HashMap::new());
        }
    }

    // Iterate over transactions and attempt to verify them
    for tx in txs {
        overlay.lock().unwrap().checkpoint();
        match verify_transaction(overlay, time_keeper, tx, &mut vks, verify_fees).await {
            Ok(gas) => _gas_used += gas,
            Err(e) => {
                warn!(target: "validator::verification::verify_transactions", "Transaction verification failed: {}", e);
                erroneous_txs.push(tx.clone());
                // TODO: verify this works as expected
                overlay.lock().unwrap().revert_to_checkpoint()?;
            }
        }

        if verify_fees {
            // Enforce that enough fee is paid.
            todo!()
        }
    }

    Ok(erroneous_txs)
}

/// Verify given [`Proposal`] against provided consensus state
pub async fn verify_proposal(
    consensus: &Consensus,
    proposal: &Proposal,
) -> Result<(Fork, Option<usize>)> {
    // TODO: verify proposal validations work as expected on versions change(cutoff)
    match block_version(proposal.block.header.height) {
        1 => verify_pow_proposal(consensus, proposal).await,
        2 => verify_pos_proposal(consensus, proposal).await,
        _ => Err(Error::BlockVersionIsInvalid(proposal.block.header.version)),
    }
}

/// Verify given PoW [`Proposal`] against provided consensus state,
/// A proposal is considered valid when the following rules apply:
///     1. Proposal hash matches the actual block one
///     2. Block transactions don't exceed set limit
///     3. If proposal extends a known fork, verify block's slot
///        correspond to the fork hot/live/next one
///     4. Block is valid
/// Additional validity rules can be applied.
pub async fn verify_pow_proposal(
    consensus: &Consensus,
    proposal: &Proposal,
) -> Result<(Fork, Option<usize>)> {
    // Check if proposal hash matches actual one (1)
    let proposal_hash = proposal.block.hash()?;
    if proposal.hash != proposal_hash {
        warn!(
            target: "validator::verification::verify_pow_proposal", "Received proposal contains mismatched hashes: {} - {}",
            proposal.hash, proposal_hash
        );
        return Err(Error::ProposalHashesMissmatchError)
    }

    // Check that proposal transactions don't exceed limit (2)
    if proposal.block.txs.len() > TXS_CAP {
        warn!(
            target: "validator::verification::verify_pow_proposal", "Received proposal transactions exceed configured cap: {} - {}",
            proposal.block.txs.len(),
            TXS_CAP
        );
        return Err(Error::ProposalTxsExceedCapError)
    }

    // Check if proposal extends any existing forks
    let (fork, index) = consensus.find_extended_fork(proposal).await?;

    // Verify block's slot correspond to the forks' hot/live/next one (3)
    if fork.slots.len() != 1 || fork.slots != proposal.block.slots {
        return Err(Error::ProposalContainsUnknownSlots)
    }

    // Insert block slot so transactions can be validated against.
    // Since this fork uses an overlay clone, original overlay is not affected.
    fork.overlay.lock().unwrap().slots.insert(&[proposal.block.slots.last().unwrap().clone()])?;

    // Grab overlay last block
    let previous = fork.overlay.lock().unwrap().last_block()?;

    // Retrieve expected reward
    let expected_reward = expected_reward(proposal.block.header.height);

    // Generate a time keeper for proposal block leight
    let mut time_keeper = consensus.time_keeper.current();
    time_keeper.verifying_slot = proposal.block.header.height;

    // Verify proposal block (4)
    if verify_block(
        &fork.overlay,
        &time_keeper,
        &fork.module,
        &proposal.block,
        &previous,
        expected_reward,
        consensus.pos_testing_mode,
    )
    .await
    .is_err()
    {
        error!(target: "validator::verification::verify_pow_proposal", "Erroneous proposal block found");
        fork.overlay.lock().unwrap().overlay.lock().unwrap().purge_new_trees()?;
        return Err(Error::BlockIsInvalid(proposal.hash.to_string()))
    };

    Ok((fork, index))
}

/// Verify given PoS [`Proposal`] against provided consensus state,
/// A proposal is considered valid when the following rules apply:
///     1. Consensus(node) has not started current slot finalization
///     2. Proposal refers to current slot
///     3. Proposal hash matches the actual block one
///     4. Block transactions don't exceed set limit
///     5. If proposal extends a known fork, verify block slots
///        correspond to the fork hot/live ones
///     6. Block is valid
/// Additional validity rules can be applied.
pub async fn verify_pos_proposal(
    consensus: &Consensus,
    proposal: &Proposal,
) -> Result<(Fork, Option<usize>)> {
    // Generate a time keeper for current slot
    let time_keeper = consensus.time_keeper.current();

    // Node have already checked for finalization in this slot (1)
    if time_keeper.verifying_slot <= *consensus.checked_finalization.read().await {
        warn!(target: "validator::verification::verify_pos_proposal", "Proposal received after finalization sync period.");
        return Err(Error::ProposalAfterFinalizationError)
    }

    // Proposal validations
    let hdr = &proposal.block.header;

    // Ignore proposal if not for current slot (2)
    if hdr.height != time_keeper.verifying_slot {
        return Err(Error::ProposalNotForCurrentSlotError)
    }

    // Check if proposal hash matches actual one (3)
    let proposal_hash = proposal.block.hash()?;
    if proposal.hash != proposal_hash {
        warn!(
            target: "validator::verification::verify_pos_proposal", "Received proposal contains mismatched hashes: {} - {}",
            proposal.hash, proposal_hash
        );
        return Err(Error::ProposalHashesMissmatchError)
    }

    // Check that proposal transactions don't exceed limit (4)
    if proposal.block.txs.len() > TXS_CAP {
        warn!(
            target: "validator::verification::verify_pos_proposal", "Received proposal transactions exceed configured cap: {} - {}",
            proposal.block.txs.len(),
            TXS_CAP
        );
        return Err(Error::ProposalTxsExceedCapError)
    }

    // Check if proposal extends any existing forks
    let (fork, index) = consensus.find_extended_fork(proposal).await?;

    // Verify block slots correspond to the forks' hot/live ones (5)
    if !fork.slots.is_empty() && fork.slots != proposal.block.slots {
        return Err(Error::ProposalContainsUnknownSlots)
    }

    // Insert last block slot so transactions can be validated against.
    // Rest (empty) slots will be inserted along with the block.
    // Since this fork uses an overlay clone, original overlay is not affected.
    fork.overlay.lock().unwrap().slots.insert(&[proposal.block.slots.last().unwrap().clone()])?;

    // Grab overlay last block
    let previous = fork.overlay.lock().unwrap().last_block()?;

    // Retrieve expected reward
    let expected_reward = expected_reward(time_keeper.verifying_slot);

    // Verify proposal block (6)
    if verify_block(
        &fork.overlay,
        &time_keeper,
        &fork.module,
        &proposal.block,
        &previous,
        expected_reward,
        consensus.pos_testing_mode,
    )
    .await
    .is_err()
    {
        error!(target: "validator::verification::verify_pos_proposal", "Erroneous proposal block found");
        fork.overlay.lock().unwrap().overlay.lock().unwrap().purge_new_trees()?;
        return Err(Error::BlockIsInvalid(proposal.hash.to_string()))
    };

    Ok((fork, index))
}
