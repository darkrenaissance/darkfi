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
    blockchain::block_version,
    crypto::{
        schnorr::SchnorrPublic, ContractId, MerkleTree, PublicKey, DEPLOYOOOR_CONTRACT_ID,
        MONEY_CONTRACT_ID,
    },
    dark_tree::dark_forest_leaf_vec_integrity_check,
    deploy::DeployParamsV1,
    pasta::pallas,
};
use darkfi_serial::{
    deserialize_async, serialize_async, AsyncDecodable, AsyncEncodable, AsyncWriteExt, WriteExt,
};
use log::{debug, error, warn};
use num_bigint::BigUint;
use smol::io::Cursor;

use crate::{
    blockchain::{
        block_store::append_tx_to_merkle_tree, BlockInfo, Blockchain, BlockchainOverlayPtr,
    },
    error::TxVerifyFailed,
    runtime::vm_runtime::Runtime,
    tx::{Transaction, MAX_TX_CALLS, MIN_TX_CALLS},
    validator::{
        consensus::{Consensus, Fork, Proposal, TXS_CAP},
        fees::{circuit_gas_use, PALLAS_SCHNORR_SIGNATURE_FEE},
        pow::PoWModule,
    },
    zk::VerifyingKey,
    Error, Result,
};

/// Verify given genesis [`BlockInfo`], and apply it to the provided overlay
pub async fn verify_genesis_block(overlay: &BlockchainOverlayPtr, block: &BlockInfo) -> Result<()> {
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

    // Block version must be correct
    if block.header.version != block_version(block.header.height) {
        return Err(Error::BlockIsInvalid(block_hash))
    }

    // Verify transactions vector contains at least one(producers) transaction
    if block.txs.is_empty() {
        return Err(Error::BlockContainsNoTransactions(block_hash))
    }

    // Genesis producer transaction must be the Transaction::default() one(empty)
    let producer_tx = block.txs.last().unwrap();
    if producer_tx != &Transaction::default() {
        error!(target: "validator::verification::verify_genesis_block", "Genesis producer transaction is not default one");
        return Err(TxVerifyFailed::ErroneousTxs(vec![producer_tx.clone()]).into())
    }

    // Verify transactions, exluding producer(last) one
    let mut tree = MerkleTree::new(1);
    let txs = &block.txs[..block.txs.len() - 1];
    if let Err(e) = verify_transactions(overlay, block.header.height, txs, &mut tree, false).await {
        warn!(
            target: "validator::verification::verify_genesis_block",
            "[VALIDATOR] Erroneous transactions found in set",
        );
        overlay.lock().unwrap().overlay.lock().unwrap().purge_new_trees()?;
        return Err(e)
    }

    // Append producer transaction to the tree and check tree matches header one
    append_tx_to_merkle_tree(&mut tree, producer_tx);
    if tree != block.header.tree {
        error!(target: "validator::verification::verify_genesis_block", "Genesis Merkle tree is invalid");
        return Err(Error::BlockIsInvalid(block_hash))
    }

    // Insert block
    overlay.lock().unwrap().add_block(block)?;

    debug!(target: "validator::verification::verify_genesis_block", "Genesis block {} verified successfully", block_hash);
    Ok(())
}

/// A block is considered valid when the following rules apply:
///     1. Block version is correct for its height
///     2. Parent hash is equal to the hash of the previous block
///     3. Block height increments previous block height by 1
///     4. Timestamp is valid based on PoWModule validation
///     5. Block hash is valid based on PoWModule validation
/// Additional validity rules can be applied.
pub fn validate_block(block: &BlockInfo, previous: &BlockInfo, module: &PoWModule) -> Result<()> {
    // Check block version (1)
    if block.header.version != block_version(block.header.height) {
        return Err(Error::BlockIsInvalid(block.hash()?.to_string()))
    }

    // Check previous hash (2)
    let previous_hash = previous.hash()?;
    if block.header.previous != previous_hash {
        return Err(Error::BlockIsInvalid(block.hash()?.to_string()))
    }

    // Check heights are incremental (3)
    if block.header.height != previous.header.height + 1 {
        return Err(Error::BlockIsInvalid(block.hash()?.to_string()))
    }

    // Check timestamp validity (4)
    if !module.verify_timestamp_by_median(block.header.timestamp) {
        return Err(Error::BlockIsInvalid(block.hash()?.to_string()))
    }

    // Check block hash corresponds to next one (5)
    module.verify_block_hash(block)?;

    Ok(())
}

/// A blockchain is considered valid, when every block is valid,
/// based on validate_block checks.
/// Be careful as this will try to load everything in memory.
pub fn validate_blockchain(
    blockchain: &Blockchain,
    pow_target: usize,
    pow_fixed_difficulty: Option<BigUint>,
) -> Result<()> {
    // Generate a PoW module
    let mut module = PoWModule::new(blockchain.clone(), pow_target, pow_fixed_difficulty)?;
    // We use block order store here so we have all blocks in order
    let blocks = blockchain.order.get_all()?;
    for (index, block) in blocks[1..].iter().enumerate() {
        let full_blocks = blockchain.get_blocks_by_hash(&[blocks[index].1, block.1])?;
        let full_block = &full_blocks[1];
        validate_block(full_block, &full_blocks[0], &module)?;
        // Update PoW module
        module.append(full_block.header.timestamp, &module.next_difficulty()?);
    }

    Ok(())
}

/// Verify given [`BlockInfo`], and apply it to the provided overlay
pub async fn verify_block(
    overlay: &BlockchainOverlayPtr,
    module: &PoWModule,
    block: &BlockInfo,
    previous: &BlockInfo,
) -> Result<()> {
    let block_hash = block.hash()?.to_string();
    debug!(target: "validator::verification::verify_block", "Validating block {}", block_hash);

    // Check if block already exists
    if overlay.lock().unwrap().has_block(block)? {
        return Err(Error::BlockAlreadyExists(block_hash))
    }

    // Validate block, using its previous
    validate_block(block, previous, module)?;

    // Verify transactions vector contains at least one(producers) transaction
    if block.txs.is_empty() {
        return Err(Error::BlockContainsNoTransactions(block_hash))
    }

    // Verify transactions, exluding producer(last) one
    let mut tree = MerkleTree::new(1);
    let txs = &block.txs[..block.txs.len() - 1];
    let e = verify_transactions(overlay, block.header.height, txs, &mut tree, false).await;
    if let Err(e) = e {
        warn!(
            target: "validator::verification::verify_block",
            "[VALIDATOR] Erroneous transactions found in set",
        );
        overlay.lock().unwrap().overlay.lock().unwrap().purge_new_trees()?;
        return Err(e)
    }

    // Verify producer transaction
    let public_key = verify_producer_transaction(
        overlay,
        block.header.height,
        block.txs.last().unwrap(),
        &mut tree,
    )
    .await?;
    verify_producer_signature(block, &public_key)?;

    // Verify tree matches header one
    if tree != block.header.tree {
        error!(target: "validator::verification::verify_block", "Block Merkle tree is invalid");
        return Err(Error::BlockIsInvalid(block_hash))
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
/// Additionally, append its hash to the provided Merkle tree.
pub async fn verify_producer_transaction(
    overlay: &BlockchainOverlayPtr,
    verifying_block_height: u64,
    tx: &Transaction,
    tree: &mut MerkleTree,
) -> Result<PublicKey> {
    let tx_hash = tx.hash();
    debug!(target: "validator::verification::verify_producer_transaction", "Validating proposal transaction {}", tx_hash);

    // Producer transactions must contain a single, non-empty call
    if tx.calls.len() != 1 || tx.calls[0].data.data.is_empty() {
        return Err(TxVerifyFailed::ErroneousTxs(vec![tx.clone()]).into())
    }

    // Verify call based on version
    let call = &tx.calls[0];
    // Block must contain a Money::PoWReward(0x06) call
    if call.data.contract_id != *MONEY_CONTRACT_ID || call.data.data[0] != 0x06 {
        return Err(TxVerifyFailed::ErroneousTxs(vec![tx.clone()]).into())
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
        Runtime::new(&wasm, overlay.clone(), call.data.contract_id, verifying_block_height)?;

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

        let (_zkbin, vk) =
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

    // Append hash to merkle tree
    append_tx_to_merkle_tree(tree, tx);

    debug!(target: "validator::verification::verify_producer_transaction", "Proposal transaction {} verified successfully", tx_hash);

    Ok(signature_public_key)
}

/// Verify WASM execution, signatures, and ZK proofs for a given [`Transaction`],
/// and apply it to the provided overlay. Additionally, append its hash to the
/// provided Merkle tree.
pub async fn verify_transaction(
    overlay: &BlockchainOverlayPtr,
    verifying_block_height: u64,
    tx: &Transaction,
    tree: &mut MerkleTree,
    verifying_keys: &mut HashMap<[u8; 32], HashMap<String, VerifyingKey>>,
    verify_fee: bool,
) -> Result<u64> {
    let tx_hash = tx.hash();
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

    // We'll also take note of all the circuits in a Vec so we can calculate their verification cost.
    let mut circuits_to_verify = vec![];

    // Iterate over all calls to get the metadata
    for (idx, call) in tx.calls.iter().enumerate() {
        // Transaction must not contain a Money::PoWReward(0x06) call
        if call.data.contract_id == *MONEY_CONTRACT_ID && call.data.data[0] == 0x06 {
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
            Runtime::new(&wasm, overlay.clone(), call.data.contract_id, verifying_block_height)?;

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
        // TODO: This vk map can potentially use a lot of RAM. Perhaps load keys on-demand at verification time?
        debug!(target: "validator::verification::verify_transaction", "Performing VerifyingKey lookups from the sled db");
        for (zkas_ns, _) in &zkp_pub {
            let inner_vk_map = verifying_keys.get_mut(&call.data.contract_id.to_bytes()).unwrap();

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
                verifying_block_height,
                tx_hash.clone(),
            )?;

            deploy_runtime.deploy(&deploy_params.ix)?;

            // Append the used gas
            gas_used += deploy_runtime.gas_used();
        }

        // At this point we're done with the call and move on to the next one.
        // Accumulate the WASM gas used.
        gas_used += runtime.gas_used();
    }

    // The signature fee is tx_size + fixed_sig_fee * n_signatures
    gas_used += (PALLAS_SCHNORR_SIGNATURE_FEE * tx.signatures.len() as u64) +
        serialize_async(tx).await.len() as u64;

    // The ZK circuit fee is calculated using a function in validator/fees.rs
    for zkbin in circuits_to_verify.iter() {
        gas_used += circuit_gas_use(zkbin);
    }

    if verify_fee {
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

        // TODO: This counts 1 gas as 1 token unit. Pricing should be better specified.
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
        error!(
            target: "validator::verification::verify_transaction",
            "[VALIDATOR] Incorrect number of signatures in tx {}", tx_hash,
        );
        return Err(TxVerifyFailed::MissingSignatures.into())
    }

    if let Err(e) = tx.verify_sigs(sig_table) {
        error!(
            target: "validator::verification::verify_transaction",
            "[VALIDATOR] Signature verification for tx {} failed: {}", tx_hash, e,
        );
        return Err(TxVerifyFailed::InvalidSignature.into())
    }
    debug!(target: "validator::verification::verify_transaction", "Signature verification successful");

    debug!(target: "validator::verification::verify_transaction", "Verifying ZK proofs for transaction {}", tx_hash);
    if let Err(e) = tx.verify_zkps(verifying_keys, zkp_table).await {
        error!(
            target: "validator::verification::verify_transaction",
            "[VALIDATOR] ZK proof verification for tx {} failed: {}", tx_hash, e,
        );
        return Err(TxVerifyFailed::InvalidZkProof.into())
    }
    debug!(target: "validator::verification::verify_transaction", "ZK proof verification successful");

    // Append hash to merkle tree
    append_tx_to_merkle_tree(tree, tx);

    debug!(target: "validator::verification::verify_transaction", "Transaction {} verified successfully", tx_hash);
    Ok(gas_used)
}

/// Verify a set of [`Transaction`] in sequence and apply them if all are valid.
/// In case any of the transactions fail, they will be returned to the caller as an error.
/// If all transactions are valid, the function will return the accumulated gas used from
/// all the transactions. Additionally, their hash is appended to the provided Merkle tree.
pub async fn verify_transactions(
    overlay: &BlockchainOverlayPtr,
    verifying_block_height: u64,
    txs: &[Transaction],
    tree: &mut MerkleTree,
    verify_fees: bool,
) -> Result<u64> {
    debug!(target: "validator::verification::verify_transactions", "Verifying {} transactions", txs.len());
    if txs.is_empty() {
        return Ok(0)
    }

    // Tracker for failed txs
    let mut erroneous_txs = vec![];

    // Gas accumulator
    let mut gas_used = 0;

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
        match verify_transaction(overlay, verifying_block_height, tx, tree, &mut vks, verify_fees)
            .await
        {
            Ok(gas) => gas_used += gas,
            Err(e) => {
                warn!(target: "validator::verification::verify_transactions", "Transaction verification failed: {}", e);
                erroneous_txs.push(tx.clone());
                overlay.lock().unwrap().revert_to_checkpoint()?;
            }
        }
    }

    if erroneous_txs.is_empty() {
        Ok(gas_used)
    } else {
        Err(TxVerifyFailed::ErroneousTxs(erroneous_txs).into())
    }
}

/// Verify given [`Proposal`] against provided consensus state,
/// A proposal is considered valid when the following rules apply:
///     1. Proposal hash matches the actual block one
///     2. Block transactions don't exceed set limit
///     3. Block is valid
/// Additional validity rules can be applied.
pub async fn verify_proposal(
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
    if proposal.block.txs.len() > TXS_CAP + 1 {
        warn!(
            target: "validator::verification::verify_pow_proposal", "Received proposal transactions exceed configured cap: {} - {}",
            proposal.block.txs.len(),
            TXS_CAP
        );
        return Err(Error::ProposalTxsExceedCapError)
    }

    // Check if proposal extends any existing forks
    let (fork, index) = consensus.find_extended_fork(proposal).await?;

    // Grab overlay last block
    let previous = fork.overlay.lock().unwrap().last_block()?;

    // Verify proposal block (3)
    if verify_block(&fork.overlay, &fork.module, &proposal.block, &previous).await.is_err() {
        error!(target: "validator::verification::verify_pow_proposal", "Erroneous proposal block found");
        fork.overlay.lock().unwrap().overlay.lock().unwrap().purge_new_trees()?;
        return Err(Error::BlockIsInvalid(proposal.hash.to_string()))
    };

    Ok((fork, index))
}
