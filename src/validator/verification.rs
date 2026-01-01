/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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
        schnorr::{SchnorrPublic, Signature},
        ContractId, MerkleTree, PublicKey,
    },
    dark_tree::dark_forest_leaf_vec_integrity_check,
    deploy::DeployParamsV1,
    monotree::{self, Monotree},
    pasta::pallas,
};
use darkfi_serial::{deserialize_async, serialize_async, AsyncDecodable, AsyncEncodable};
use num_bigint::BigUint;
use sled_overlay::SledDbOverlayStateDiff;
use smol::io::Cursor;
use tracing::{debug, error, warn};

use crate::{
    blockchain::{
        block_store::append_tx_to_merkle_tree, header_store::PowData::DarkFi, BlockInfo,
        Blockchain, BlockchainOverlayPtr, HeaderHash,
    },
    error::TxVerifyFailed,
    runtime::vm_runtime::Runtime,
    tx::{Transaction, MAX_TX_CALLS, MIN_TX_CALLS},
    validator::{
        consensus::{Consensus, Fork, Proposal, BLOCK_GAS_LIMIT},
        fees::{circuit_gas_use, compute_fee, GasData, PALLAS_SCHNORR_SIGNATURE_FEE},
        pow::PoWModule,
    },
    zk::VerifyingKey,
    Error, Result,
};

/// Verify given genesis [`BlockInfo`], and apply it to the provided overlay.
pub async fn verify_genesis_block(
    overlay: &BlockchainOverlayPtr,
    block: &BlockInfo,
    block_target: u32,
) -> Result<()> {
    let block_hash = block.hash().as_string();
    debug!(target: "validator::verification::verify_genesis_block", "Validating genesis block {block_hash}");

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

    // Block must use Darkfi native Proof of Work data
    match block.header.pow_data {
        DarkFi => { /* do nothing */ }
        _ => return Err(Error::BlockIsInvalid(block_hash)),
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

    // Verify transactions, exluding producer(last) one/
    // Genesis block doesn't check for fees
    let mut tree = MerkleTree::new(1);
    let txs = &block.txs[..block.txs.len() - 1];
    if let Err(e) =
        verify_transactions(overlay, block.header.height, block_target, txs, &mut tree, false).await
    {
        warn!(
            target: "validator::verification::verify_genesis_block",
            "[VALIDATOR] Erroneous transactions found in set",
        );
        overlay.lock().unwrap().overlay.lock().unwrap().purge_new_trees()?;
        return Err(e)
    }

    // Append producer transaction to the tree and check tree matches header one
    append_tx_to_merkle_tree(&mut tree, producer_tx);
    if tree.root(0).unwrap() != block.header.transactions_root {
        error!(target: "validator::verification::verify_genesis_block", "Genesis Merkle tree is invalid");
        return Err(Error::BlockIsInvalid(block_hash))
    }

    // Verify header contracts states root
    let state_monotree = overlay.lock().unwrap().contracts.get_state_monotree()?;
    let Some(state_root) = state_monotree.get_headroot()? else {
        return Err(Error::ContractsStatesRootNotFoundError);
    };
    if state_root != block.header.state_root {
        return Err(Error::ContractsStatesRootError(
            blake3::Hash::from_bytes(state_root).to_string(),
            blake3::Hash::from_bytes(block.header.state_root).to_string(),
        ));
    }

    // Genesis producer signature must be the Signature::dummy() one(empty)
    if block.signature != Signature::dummy() {
        error!(target: "validator::verification::verify_genesis_block", "Genesis producer signature is not dummy one");
        return Err(Error::InvalidSignature)
    }

    // Insert block
    overlay.lock().unwrap().add_block(block)?;

    debug!(target: "validator::verification::verify_genesis_block", "Genesis block {block_hash} verified successfully");
    Ok(())
}

/// Validate provided block according to set rules.
///
/// A block is considered valid when the following rules apply:
///     1. Block version is correct for its height
///     2. Previous hash is equal to the hash of the provided previous block
///     3. Block height increments previous block height by 1
///     4. Timestamp is valid based on PoWModule validation
///     5. Block header Proof of Work data are valid
///     6. Block hash is valid based on PoWModule validation
/// Additional validity rules can be applied.
pub fn validate_block(block: &BlockInfo, previous: &BlockInfo, module: &PoWModule) -> Result<()> {
    // Check block version (1)
    if block.header.version != block_version(block.header.height) {
        return Err(Error::BlockIsInvalid(block.hash().as_string()))
    }

    // Check previous hash (2)
    if block.header.previous != previous.hash() {
        return Err(Error::BlockIsInvalid(block.hash().as_string()))
    }

    // Check heights are incremental (3)
    if block.header.height != previous.header.height + 1 {
        return Err(Error::BlockIsInvalid(block.hash().as_string()))
    }

    // Check timestamp validity (4)
    if !module.verify_timestamp_by_median(block.header.timestamp) {
        return Err(Error::BlockIsInvalid(block.hash().as_string()))
    }

    // Check PoW data validty (5)
    if !block.header.validate_powdata() {
        return Err(Error::BlockIsInvalid(block.hash().as_string()))
    }

    // Check block hash corresponds to next  mine target (6)
    module.verify_block_hash(&block.header)?;

    Ok(())
}

/// A blockchain is considered valid, when every block is valid,
/// based on validate_block checks.
/// Be careful as this will try to load everything in memory.
pub fn validate_blockchain(
    blockchain: &Blockchain,
    pow_target: u32,
    pow_fixed_difficulty: Option<BigUint>,
) -> Result<()> {
    // Generate a PoW module
    let mut module = PoWModule::new(blockchain.clone(), pow_target, pow_fixed_difficulty, Some(0))?;

    // We use block order store here so we have all blocks in order
    let blocks = blockchain.blocks.get_all_order()?;
    for (index, block) in blocks[1..].iter().enumerate() {
        let full_blocks = blockchain.get_blocks_by_hash(&[blocks[index].1, block.1])?;
        let full_block = &full_blocks[1];
        validate_block(full_block, &full_blocks[0], &module)?;
        // Update PoW module
        module.append(&full_block.header, &module.next_difficulty()?)?;
    }

    Ok(())
}

/// Verify given [`BlockInfo`], and apply it to the provided overlay.
pub async fn verify_block(
    overlay: &BlockchainOverlayPtr,
    diffs: &[SledDbOverlayStateDiff],
    module: &PoWModule,
    state_monotree: &mut Monotree<monotree::MemoryDb>,
    block: &BlockInfo,
    previous: &BlockInfo,
    verify_fees: bool,
) -> Result<()> {
    let block_hash = block.hash();
    debug!(target: "validator::verification::verify_block", "Validating block {block_hash}");

    // Check if block already exists
    if overlay.lock().unwrap().has_block(block)? {
        return Err(Error::BlockAlreadyExists(block_hash.as_string()))
    }

    // Validate block, using its previous
    validate_block(block, previous, module)?;

    // Verify transactions vector contains at least one(producers) transaction
    if block.txs.is_empty() {
        return Err(Error::BlockContainsNoTransactions(block_hash.as_string()))
    }

    // Verify transactions, exluding producer(last) one
    let mut tree = MerkleTree::new(1);
    let txs = &block.txs[..block.txs.len() - 1];
    let e = verify_transactions(
        overlay,
        block.header.height,
        module.target,
        txs,
        &mut tree,
        verify_fees,
    )
    .await;
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
        module.target,
        block.txs.last().unwrap(),
        &mut tree,
    )
    .await?;

    // Verify transactions merkle tree root matches header one
    if tree.root(0).unwrap() != block.header.transactions_root {
        error!(target: "validator::verification::verify_block", "Block Merkle tree root is invalid");
        return Err(Error::BlockIsInvalid(block_hash.as_string()))
    }

    // Update the provided contracts states monotree and verify header contracts states root
    let diff = overlay.lock().unwrap().overlay.lock().unwrap().diff(diffs)?;
    overlay.lock().unwrap().contracts.update_state_monotree(&diff, state_monotree)?;
    let Some(state_root) = state_monotree.get_headroot()? else {
        return Err(Error::ContractsStatesRootNotFoundError);
    };
    if state_root != block.header.state_root {
        return Err(Error::ContractsStatesRootError(
            blake3::Hash::from_bytes(state_root).to_string(),
            blake3::Hash::from_bytes(block.header.state_root).to_string(),
        ));
    }

    // Verify producer signature
    verify_producer_signature(block, &public_key)?;

    // Insert block
    overlay.lock().unwrap().add_block(block)?;

    debug!(target: "validator::verification::verify_block", "Block {block_hash} verified successfully");
    Ok(())
}

/// Verify given checkpoint [`BlockInfo`], and apply it to the provided overlay.
pub async fn verify_checkpoint_block(
    overlay: &BlockchainOverlayPtr,
    diffs: &[SledDbOverlayStateDiff],
    state_monotree: &mut Monotree<monotree::MemoryDb>,
    block: &BlockInfo,
    header: &HeaderHash,
    block_target: u32,
) -> Result<()> {
    let block_hash = block.hash();
    debug!(target: "validator::verification::verify_checkpoint_block", "Validating block {block_hash}");

    // Check if block already exists
    if overlay.lock().unwrap().has_block(block)? {
        return Err(Error::BlockAlreadyExists(block_hash.as_string()))
    }

    // Check if block hash matches the expected(provided) one
    if block_hash != *header {
        error!(target: "validator::verification::verify_checkpoint_block", "Block hash doesn't match the expected one");
        return Err(Error::BlockIsInvalid(block_hash.as_string()))
    }

    // Verify transactions vector contains at least one(producers) transaction
    if block.txs.is_empty() {
        return Err(Error::BlockContainsNoTransactions(block_hash.as_string()))
    }

    // Apply transactions, excluding producer(last) one
    let mut tree = MerkleTree::new(1);
    let txs = &block.txs[..block.txs.len() - 1];
    let e = apply_transactions(overlay, block.header.height, block_target, txs, &mut tree).await;
    if let Err(e) = e {
        warn!(
            target: "validator::verification::verify_checkpoint_block",
            "[VALIDATOR] Erroneous transactions found in set",
        );
        overlay.lock().unwrap().overlay.lock().unwrap().purge_new_trees()?;
        return Err(e)
    }

    // Apply producer transaction
    let public_key = apply_producer_transaction(
        overlay,
        block.header.height,
        block_target,
        block.txs.last().unwrap(),
        &mut tree,
    )
    .await?;

    // Verify transactions merkle tree root matches header one
    if tree.root(0).unwrap() != block.header.transactions_root {
        error!(target: "validator::verification::verify_checkpoint_block", "Block Merkle tree root is invalid");
        return Err(Error::BlockIsInvalid(block_hash.as_string()))
    }

    // Update the provided contracts states monotree and verify header contracts states root
    let diff = overlay.lock().unwrap().overlay.lock().unwrap().diff(diffs)?;
    overlay.lock().unwrap().contracts.update_state_monotree(&diff, state_monotree)?;
    let Some(state_root) = state_monotree.get_headroot()? else {
        return Err(Error::ContractsStatesRootNotFoundError);
    };
    if state_root != block.header.state_root {
        return Err(Error::ContractsStatesRootError(
            blake3::Hash::from_bytes(state_root).to_string(),
            blake3::Hash::from_bytes(block.header.state_root).to_string(),
        ));
    }

    // Verify producer signature
    verify_producer_signature(block, &public_key)?;

    // Insert block
    overlay.lock().unwrap().add_block(block)?;

    debug!(target: "validator::verification::verify_checkpoint_block", "Block {block_hash} verified successfully");
    Ok(())
}

/// Verify block proposer signature, using the producer transaction signature as signing key
/// over blocks header hash.
pub fn verify_producer_signature(block: &BlockInfo, public_key: &PublicKey) -> Result<()> {
    if !public_key.verify(block.header.hash().inner(), &block.signature) {
        warn!(target: "validator::verification::verify_producer_signature", "Proposer {public_key} signature could not be verified");
        return Err(Error::InvalidSignature)
    }

    Ok(())
}

/// Verify provided producer [`Transaction`].
///
/// Verify WASM execution, signatures, and ZK proofs and apply it to the provided,
/// provided overlay. Returns transaction signature public key. Additionally,
/// append its hash to the provided Merkle tree.
pub async fn verify_producer_transaction(
    overlay: &BlockchainOverlayPtr,
    verifying_block_height: u32,
    block_target: u32,
    tx: &Transaction,
    tree: &mut MerkleTree,
) -> Result<PublicKey> {
    let tx_hash = tx.hash();
    debug!(target: "validator::verification::verify_producer_transaction", "Validating producer transaction {tx_hash}");

    // Transaction must be a PoW reward one
    if !tx.is_pow_reward() {
        return Err(TxVerifyFailed::ErroneousTxs(vec![tx.clone()]).into())
    }

    // Retrieve first call from the transaction for further processing
    let call = &tx.calls[0];

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
    tx.calls.encode_async(&mut payload).await?; // Actual call data

    debug!(target: "validator::verification::verify_producer_transaction", "Instantiating WASM runtime");
    let wasm = overlay.lock().unwrap().contracts.get(call.data.contract_id)?;

    let mut runtime = Runtime::new(
        &wasm,
        overlay.clone(),
        call.data.contract_id,
        verifying_block_height,
        block_target,
        tx_hash,
        // Call index in producer tx is 0
        0,
    )?;

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
        error!(target: "validator::verification::verify_producer_transaction", "Producer transaction contains multiple ZK proofs or signature public keys");
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
    // and the same payload. We keep the returned state update in a buffer, prefixed
    // by the call function ID, enforcing the state update function in the contract.
    debug!(target: "validator::verification::verify_producer_transaction", "Executing \"exec\" call");
    let mut state_update = vec![call.data.data[0]];
    state_update.append(&mut runtime.exec(&payload)?);
    debug!(target: "validator::verification::verify_producer_transaction", "Successfully executed \"exec\" call");

    // If that was successful, we apply the state update in the ephemeral overlay.
    debug!(target: "validator::verification::verify_producer_transaction", "Executing \"apply\" call");
    runtime.apply(&state_update)?;
    debug!(target: "validator::verification::verify_producer_transaction", "Successfully executed \"apply\" call");

    // When we're done executing over the tx's contract call, we now move on with verification.
    // First we verify the signatures as that's cheaper, and then finally we verify the ZK proofs.
    debug!(target: "validator::verification::verify_producer_transaction", "Verifying signatures for transaction {tx_hash}");
    if sig_table.len() != tx.signatures.len() {
        error!(target: "validator::verification::verify_producer_transaction", "Incorrect number of signatures in tx {tx_hash}");
        return Err(TxVerifyFailed::MissingSignatures.into())
    }

    // TODO: Go through the ZK circuits that have to be verified and account for the opcodes.

    if let Err(e) = tx.verify_sigs(sig_table) {
        error!(target: "validator::verification::verify_producer_transaction", "Signature verification for tx {tx_hash} failed: {e}");
        return Err(TxVerifyFailed::InvalidSignature.into())
    }

    debug!(target: "validator::verification::verify_producer_transaction", "Signature verification successful");

    debug!(target: "validator::verification::verify_producer_transaction", "Verifying ZK proofs for transaction {tx_hash}");
    if let Err(e) = tx.verify_zkps(&verifying_keys, zkp_table).await {
        error!(target: "validator::verification::verify_producer_transaction", "ZK proof verification for tx {tx_hash} failed: {e}");
        return Err(TxVerifyFailed::InvalidZkProof.into())
    }
    debug!(target: "validator::verification::verify_producer_transaction", "ZK proof verification successful");

    // Append hash to merkle tree
    append_tx_to_merkle_tree(tree, tx);

    debug!(target: "validator::verification::verify_producer_transaction", "Producer transaction {tx_hash} verified successfully");

    Ok(signature_public_key)
}

/// Apply given producer [`Transaction`] to the provided overlay, without formal verification.
/// Returns transaction signature public key. Additionally, append its hash to the provided Merkle tree.
pub async fn apply_producer_transaction(
    overlay: &BlockchainOverlayPtr,
    verifying_block_height: u32,
    block_target: u32,
    tx: &Transaction,
    tree: &mut MerkleTree,
) -> Result<PublicKey> {
    let tx_hash = tx.hash();
    debug!(target: "validator::verification::apply_producer_transaction", "Applying producer transaction {tx_hash}");

    // Producer transactions must contain a single, non-empty call
    if !tx.is_single_call() {
        return Err(TxVerifyFailed::ErroneousTxs(vec![tx.clone()]).into())
    }

    debug!(target: "validator::verification::apply_producer_transaction", "Executing contract call");

    // Write the actual payload data
    let mut payload = vec![];
    tx.calls.encode_async(&mut payload).await?; // Actual call data

    debug!(target: "validator::verification::apply_producer_transaction", "Instantiating WASM runtime");
    let call = &tx.calls[0];
    let wasm = overlay.lock().unwrap().contracts.get(call.data.contract_id)?;

    let mut runtime = Runtime::new(
        &wasm,
        overlay.clone(),
        call.data.contract_id,
        verifying_block_height,
        block_target,
        tx_hash,
        // Call index in producer tx is 0
        0,
    )?;

    debug!(target: "validator::verification::apply_producer_transaction", "Executing \"metadata\" call");
    let metadata = runtime.metadata(&payload)?;

    // Decode the metadata retrieved from the execution
    let mut decoder = Cursor::new(&metadata);

    // The tuple is (zkas_ns, public_inputs)
    let _: Vec<(String, Vec<pallas::Base>)> = AsyncDecodable::decode_async(&mut decoder).await?;
    let sig_pub: Vec<PublicKey> = AsyncDecodable::decode_async(&mut decoder).await?;

    // Check that only one ZK proof and signature public key exist
    if sig_pub.len() != 1 {
        error!(target: "validator::verification::apply_producer_transaction", "Producer transaction contains multiple ZK proofs or signature public keys");
        return Err(TxVerifyFailed::ErroneousTxs(vec![tx.clone()]).into())
    }

    let signature_public_key = *sig_pub.last().unwrap();

    // After getting the metadata, we run the "exec" function with the same runtime
    // and the same payload. We keep the returned state update in a buffer, prefixed
    // by the call function ID, enforcing the state update function in the contract.
    debug!(target: "validator::verification::apply_producer_transaction", "Executing \"exec\" call");
    let mut state_update = vec![call.data.data[0]];
    state_update.append(&mut runtime.exec(&payload)?);
    debug!(target: "validator::verification::apply_producer_transaction", "Successfully executed \"exec\" call");

    // If that was successful, we apply the state update in the ephemeral overlay.
    debug!(target: "validator::verification::apply_producer_transaction", "Executing \"apply\" call");
    runtime.apply(&state_update)?;
    debug!(target: "validator::verification::apply_producer_transaction", "Successfully executed \"apply\" call");

    // Append hash to merkle tree
    append_tx_to_merkle_tree(tree, tx);

    debug!(target: "validator::verification::apply_producer_transaction", "Producer transaction {tx_hash} executed successfully");

    Ok(signature_public_key)
}

/// Verify WASM execution, signatures, and ZK proofs for a given [`Transaction`],
/// and apply it to the provided overlay. Additionally, append its hash to the
/// provided Merkle tree.
pub async fn verify_transaction(
    overlay: &BlockchainOverlayPtr,
    verifying_block_height: u32,
    block_target: u32,
    tx: &Transaction,
    tree: &mut MerkleTree,
    verifying_keys: &mut HashMap<[u8; 32], HashMap<String, VerifyingKey>>,
    verify_fee: bool,
) -> Result<GasData> {
    let tx_hash = tx.hash();
    debug!(target: "validator::verification::verify_transaction", "Validating transaction {tx_hash}");

    // Create a FeeData instance to hold the calculated fee data
    let mut gas_data = GasData::default();

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
        // Verify that there is a single money fee call in the transaction
        let mut found_fee = false;
        for (call_idx, call) in tx.calls.iter().enumerate() {
            if !call.data.is_money_fee() {
                continue
            }

            if found_fee {
                error!(
                    target: "validator::verification::verify_transcation",
                    "[VALIDATOR] Transaction {tx_hash} contains multiple fee payment calls"
                );
                return Err(TxVerifyFailed::InvalidFee.into())
            }

            found_fee = true;
            fee_call_idx = call_idx;
        }

        if !found_fee {
            error!(
                target: "validator::verification::verify_transcation",
                "[VALIDATOR] Transaction {tx_hash} does not contain fee payment call"
            );
            return Err(TxVerifyFailed::InvalidFee.into())
        }
    }

    // Write the transaction calls payload data
    let mut payload = vec![];
    tx.calls.encode_async(&mut payload).await?;

    // Define a buffer in case we want to use a different payload in a specific call
    let mut _call_payload = vec![];

    // We'll also take note of all the circuits in a Vec so we can calculate their verification cost.
    let mut circuits_to_verify = vec![];

    // Iterate over all calls to get the metadata
    for (idx, call) in tx.calls.iter().enumerate() {
        debug!(target: "validator::verification::verify_transaction", "Executing contract call {idx}");

        // Transaction must not contain a Pow reward call
        if call.data.is_money_pow_reward() {
            error!(target: "validator::verification::verify_transaction", "Reward transaction detected");
            return Err(TxVerifyFailed::ErroneousTxs(vec![tx.clone()]).into())
        }

        // Check if its the fee call so we only pass its payload
        let (call_idx, call_payload) = if call.data.is_money_fee() {
            _call_payload = vec![];
            vec![call.clone()].encode_async(&mut _call_payload).await?;
            (0_u8, &_call_payload)
        } else {
            (idx as u8, &payload)
        };

        debug!(target: "validator::verification::verify_transaction", "Instantiating WASM runtime");
        let wasm = overlay.lock().unwrap().contracts.get(call.data.contract_id)?;
        let mut runtime = Runtime::new(
            &wasm,
            overlay.clone(),
            call.data.contract_id,
            verifying_block_height,
            block_target,
            tx_hash,
            call_idx,
        )?;

        debug!(target: "validator::verification::verify_transaction", "Executing \"metadata\" call");
        let metadata = runtime.metadata(call_payload)?;

        // Decode the metadata retrieved from the execution
        let mut decoder = Cursor::new(&metadata);

        // The tuple is (zkas_ns, public_inputs)
        let zkp_pub: Vec<(String, Vec<pallas::Base>)> =
            AsyncDecodable::decode_async(&mut decoder).await?;
        let sig_pub: Vec<PublicKey> = AsyncDecodable::decode_async(&mut decoder).await?;

        if decoder.position() != metadata.len() as u64 {
            error!(
                target: "validator::verification::verify_transaction",
                "[VALIDATOR] Failed decoding entire metadata buffer for {tx_hash}:{idx}"
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
        // and the same payload. We keep the returned state update in a buffer, prefixed
        // by the call function ID, enforcing the state update function in the contract.
        debug!(target: "validator::verification::verify_transaction", "Executing \"exec\" call");
        let mut state_update = vec![call.data.data[0]];
        state_update.append(&mut runtime.exec(call_payload)?);
        debug!(target: "validator::verification::verify_transaction", "Successfully executed \"exec\" call");

        // If that was successful, we apply the state update in the ephemeral overlay.
        debug!(target: "validator::verification::verify_transaction", "Executing \"apply\" call");
        runtime.apply(&state_update)?;
        debug!(target: "validator::verification::verify_transaction", "Successfully executed \"apply\" call");

        // If this call is supposed to deploy a new contract, we have to instantiate
        // a new `Runtime` and run its deploy function.
        if call.data.is_deployment()
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
                block_target,
                tx_hash,
                call_idx,
            )?;

            deploy_runtime.deploy(&deploy_params.ix)?;

            let deploy_gas_used = deploy_runtime.gas_used();
            debug!(target: "validator::verification::verify_transaction", "The gas used for deployment call {call:?} of transaction {tx_hash}: {deploy_gas_used}");
            gas_data.deployments += deploy_gas_used;
        }

        // At this point we're done with the call and move on to the next one.
        // Accumulate the WASM gas used.
        let wasm_gas_used = runtime.gas_used();
        debug!(target: "validator::verification::verify_transaction", "The gas used for WASM call {call:?} of transaction {tx_hash}: {wasm_gas_used}");

        // Append the used wasm gas
        gas_data.wasm += wasm_gas_used;
    }

    // The signature fee is tx_size + fixed_sig_fee * n_signatures
    gas_data.signatures = (PALLAS_SCHNORR_SIGNATURE_FEE * tx.signatures.len() as u64) +
        serialize_async(tx).await.len() as u64;
    debug!(target: "validator::verification::verify_transaction", "The gas used for signature of transaction {tx_hash}: {}", gas_data.signatures);

    // The ZK circuit fee is calculated using a function in validator/fees.rs
    for zkbin in circuits_to_verify.iter() {
        let zk_circuit_gas_used = circuit_gas_use(zkbin);
        debug!(target: "validator::verification::verify_transaction", "The gas used for ZK circuit in namespace {} of transaction {tx_hash}: {zk_circuit_gas_used}", zkbin.namespace);

        // Append the used zk circuit gas
        gas_data.zk_circuits += zk_circuit_gas_used;
    }

    // Store the calculated total gas used to avoid recalculating it for subsequent uses
    let total_gas_used = gas_data.total_gas_used();

    if verify_fee {
        // Deserialize the fee call to find the paid fee
        let fee: u64 = match deserialize_async(&tx.calls[fee_call_idx].data.data[1..9]).await {
            Ok(v) => v,
            Err(e) => {
                error!(
                    target: "validator::verification::verify_transaction",
                    "[VALIDATOR] Failed deserializing tx {tx_hash} fee call: {e}"
                );
                return Err(TxVerifyFailed::InvalidFee.into())
            }
        };

        // Compute the required fee for this transaction
        let required_fee = compute_fee(&total_gas_used);

        // Check that enough fee has been paid for the used gas in this transaction
        if required_fee > fee {
            error!(
                target: "validator::verification::verify_transaction",
                "[VALIDATOR] Transaction {tx_hash} has insufficient fee. Required: {required_fee}, Paid: {fee}"
            );
            return Err(TxVerifyFailed::InsufficientFee.into())
        }
        debug!(target: "validator::verification::verify_transaction", "The gas paid for transaction {tx_hash}: {}", gas_data.paid);

        // Store paid fee
        gas_data.paid = fee;
    }

    // When we're done looping and executing over the tx's contract calls and
    // (optionally) made sure that enough fee was paid, we now move on with
    // verification. First we verify the transaction signatures and then we
    // verify any accompanying ZK proofs.
    debug!(target: "validator::verification::verify_transaction", "Verifying signatures for transaction {tx_hash}");
    if sig_table.len() != tx.signatures.len() {
        error!(
            target: "validator::verification::verify_transaction",
            "[VALIDATOR] Incorrect number of signatures in tx {tx_hash}"
        );
        return Err(TxVerifyFailed::MissingSignatures.into())
    }

    if let Err(e) = tx.verify_sigs(sig_table) {
        error!(
            target: "validator::verification::verify_transaction",
            "[VALIDATOR] Signature verification for tx {tx_hash} failed: {e}"
        );
        return Err(TxVerifyFailed::InvalidSignature.into())
    }
    debug!(target: "validator::verification::verify_transaction", "Signature verification successful");

    debug!(target: "validator::verification::verify_transaction", "Verifying ZK proofs for transaction {tx_hash}");
    if let Err(e) = tx.verify_zkps(verifying_keys, zkp_table).await {
        error!(
            target: "validator::verification::verify_transaction",
            "[VALIDATOR] ZK proof verification for tx {tx_hash} failed: {e}"
        );
        return Err(TxVerifyFailed::InvalidZkProof.into())
    }
    debug!(target: "validator::verification::verify_transaction", "ZK proof verification successful");

    // Append hash to merkle tree
    append_tx_to_merkle_tree(tree, tx);

    debug!(target: "validator::verification::verify_transaction", "The total gas used for transaction {tx_hash}: {total_gas_used}");
    debug!(target: "validator::verification::verify_transaction", "Transaction {tx_hash} verified successfully");
    Ok(gas_data)
}

/// Apply given [`Transaction`] to the provided overlay.
/// Additionally, append its hash to the provided Merkle tree.
pub async fn apply_transaction(
    overlay: &BlockchainOverlayPtr,
    verifying_block_height: u32,
    block_target: u32,
    tx: &Transaction,
    tree: &mut MerkleTree,
) -> Result<()> {
    let tx_hash = tx.hash();
    debug!(target: "validator::verification::apply_transaction", "Applying transaction {tx_hash}");

    // Write the transaction calls payload data
    let mut payload = vec![];
    tx.calls.encode_async(&mut payload).await?;

    // Iterate over all calls to get the metadata
    for (idx, call) in tx.calls.iter().enumerate() {
        debug!(target: "validator::verification::apply_transaction", "Executing contract call {idx}");

        debug!(target: "validator::verification::apply_transaction", "Instantiating WASM runtime");
        let wasm = overlay.lock().unwrap().contracts.get(call.data.contract_id)?;
        let mut runtime = Runtime::new(
            &wasm,
            overlay.clone(),
            call.data.contract_id,
            verifying_block_height,
            block_target,
            tx_hash,
            idx as u8,
        )?;

        // Run the "exec" function. We keep the returned state update in a buffer, prefixed
        // by the call function ID, enforcing the state update function in the contract.
        debug!(target: "validator::verification::apply_transaction", "Executing \"exec\" call");
        let mut state_update = vec![call.data.data[0]];
        state_update.append(&mut runtime.exec(&payload)?);
        debug!(target: "validator::verification::apply_transaction", "Successfully executed \"exec\" call");

        // If that was successful, we apply the state update in the ephemeral overlay
        debug!(target: "validator::verification::apply_transaction", "Executing \"apply\" call");
        runtime.apply(&state_update)?;
        debug!(target: "validator::verification::apply_transaction", "Successfully executed \"apply\" call");

        // If this call is supposed to deploy a new contract, we have to instantiate
        // a new `Runtime` and run its deploy function.
        if call.data.is_deployment()
        /* DeployV1 */
        {
            debug!(target: "validator::verification::apply_transaction", "Deploying new contract");
            // Deserialize the deployment parameters
            let deploy_params: DeployParamsV1 = deserialize_async(&call.data.data[1..]).await?;
            let deploy_cid = ContractId::derive_public(deploy_params.public_key);

            // Instantiate the new deployment runtime
            let mut deploy_runtime = Runtime::new(
                &deploy_params.wasm_bincode,
                overlay.clone(),
                deploy_cid,
                verifying_block_height,
                block_target,
                tx_hash,
                idx as u8,
            )?;

            deploy_runtime.deploy(&deploy_params.ix)?;
        }
    }

    // Append hash to merkle tree
    append_tx_to_merkle_tree(tree, tx);

    debug!(target: "validator::verification::apply_transaction", "Transaction {tx_hash} applied successfully");
    Ok(())
}

/// Verify a set of [`Transaction`] in sequence and apply them if all are valid.
///
/// In case any of the transactions fail, they will be returned to the caller as an error.
/// If all transactions are valid, the function will return the total gas used and total
/// paid fees from all the transactions. Additionally, their hash is appended to the provided
/// Merkle tree.
pub async fn verify_transactions(
    overlay: &BlockchainOverlayPtr,
    verifying_block_height: u32,
    block_target: u32,
    txs: &[Transaction],
    tree: &mut MerkleTree,
    verify_fees: bool,
) -> Result<(u64, u64)> {
    debug!(target: "validator::verification::verify_transactions", "Verifying {} transactions", txs.len());
    if txs.is_empty() {
        return Ok((0, 0))
    }

    // Tracker for failed txs
    let mut erroneous_txs = vec![];

    // Total gas accumulators
    let mut total_gas_used = 0;
    let mut total_gas_paid = 0;

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
        let gas_data = match verify_transaction(
            overlay,
            verifying_block_height,
            block_target,
            tx,
            tree,
            &mut vks,
            verify_fees,
        )
        .await
        {
            Ok(gas_values) => gas_values,
            Err(e) => {
                warn!(target: "validator::verification::verify_transactions", "Transaction verification failed: {e}");
                erroneous_txs.push(tx.clone());
                overlay.lock().unwrap().overlay.lock().unwrap().purge_new_trees()?;
                overlay.lock().unwrap().revert_to_checkpoint()?;
                continue
            }
        };

        // Store the gas used by the verified transaction
        let tx_gas_used = gas_data.total_gas_used();

        // Calculate current accumulated gas usage
        let accumulated_gas_usage = total_gas_used + tx_gas_used;

        // Check gas limit - if accumulated gas used exceeds it, break out of loop
        if accumulated_gas_usage > BLOCK_GAS_LIMIT {
            warn!(
                target: "validator::verification::verify_transactions",
                "Transaction {} exceeds configured transaction gas limit: {accumulated_gas_usage} - {BLOCK_GAS_LIMIT}",
                tx.hash()
            );
            erroneous_txs.push(tx.clone());
            overlay.lock().unwrap().overlay.lock().unwrap().purge_new_trees()?;
            overlay.lock().unwrap().revert_to_checkpoint()?;
            break
        }

        // Update accumulated total gas
        total_gas_used += tx_gas_used;
        total_gas_paid += gas_data.paid;
    }

    if !erroneous_txs.is_empty() {
        return Err(TxVerifyFailed::ErroneousTxs(erroneous_txs).into())
    }

    Ok((total_gas_used, total_gas_paid))
}

/// Apply given set of [`Transaction`] in sequence, without formal verification.
/// In case any of the transactions fail, they will be returned to the caller as an error.
/// Additionally, their hash is appended to the provided Merkle tree.
async fn apply_transactions(
    overlay: &BlockchainOverlayPtr,
    verifying_block_height: u32,
    block_target: u32,
    txs: &[Transaction],
    tree: &mut MerkleTree,
) -> Result<()> {
    debug!(target: "validator::verification::apply_transactions", "Applying {} transactions", txs.len());
    if txs.is_empty() {
        return Ok(())
    }

    // Tracker for failed txs
    let mut erroneous_txs = vec![];

    // Iterate over transactions and attempt to apply them
    for tx in txs {
        overlay.lock().unwrap().checkpoint();
        if let Err(e) =
            apply_transaction(overlay, verifying_block_height, block_target, tx, tree).await
        {
            warn!(target: "validator::verification::apply_transactions", "Transaction apply failed: {e}");
            erroneous_txs.push(tx.clone());
            overlay.lock().unwrap().revert_to_checkpoint()?;
        };
    }

    if !erroneous_txs.is_empty() {
        return Err(TxVerifyFailed::ErroneousTxs(erroneous_txs).into())
    }

    Ok(())
}

/// Verify given [`Proposal`] against provided consensus state.
///
/// A proposal is considered valid when the following rules apply:
///     1. Proposal hash matches the actual block one
///     2. Block is valid
/// Additional validity rules can be applied.
pub async fn verify_proposal(
    consensus: &Consensus,
    proposal: &Proposal,
    verify_fees: bool,
) -> Result<(Fork, Option<usize>)> {
    // Check if proposal hash matches actual one (1)
    let proposal_hash = proposal.block.hash();
    if proposal.hash != proposal_hash {
        warn!(
            target: "validator::verification::verify_proposal", "Received proposal contains mismatched hashes: {} - {proposal_hash}",
            proposal.hash
        );
        return Err(Error::ProposalHashesMissmatchError)
    }

    // Check if proposal extends any existing forks
    let (mut fork, index) = consensus.find_extended_fork(proposal).await?;

    // Grab overlay last block
    let previous = fork.overlay.lock().unwrap().last_block()?;

    // Verify proposal block (2)
    if let Err(e) = verify_block(
        &fork.overlay,
        &fork.diffs,
        &fork.module,
        &mut fork.state_monotree,
        &proposal.block,
        &previous,
        verify_fees,
    )
    .await
    {
        error!(target: "validator::verification::verify_proposal", "Erroneous proposal block found: {e}");
        fork.overlay.lock().unwrap().overlay.lock().unwrap().purge_new_trees()?;
        return Err(Error::BlockIsInvalid(proposal.hash.as_string()))
    };

    Ok((fork, index))
}

/// Verify given [`Proposal`] against provided fork state.
///
/// A proposal is considered valid when the following rules apply:
///     1. Proposal hash matches the actual block one
///     2. Block is valid
/// Additional validity rules can be applied.
pub async fn verify_fork_proposal(
    fork: &mut Fork,
    proposal: &Proposal,
    verify_fees: bool,
) -> Result<()> {
    // Check if proposal hash matches actual one (1)
    let proposal_hash = proposal.block.hash();
    if proposal.hash != proposal_hash {
        warn!(
            target: "validator::verification::verify_fork_proposal", "Received proposal contains mismatched hashes: {} - {proposal_hash}",
            proposal.hash
        );
        return Err(Error::ProposalHashesMissmatchError)
    }

    // Grab overlay last block
    let previous = fork.overlay.lock().unwrap().last_block()?;

    // Verify proposal block (2)
    if let Err(e) = verify_block(
        &fork.overlay,
        &fork.diffs,
        &fork.module,
        &mut fork.state_monotree,
        &proposal.block,
        &previous,
        verify_fees,
    )
    .await
    {
        error!(target: "validator::verification::verify_fork_proposal", "Erroneous proposal block found: {e}");
        fork.overlay.lock().unwrap().overlay.lock().unwrap().purge_new_trees()?;
        return Err(Error::BlockIsInvalid(proposal.hash.as_string()))
    };

    Ok(())
}
