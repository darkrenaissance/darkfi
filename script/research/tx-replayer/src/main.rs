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

use clap::Parser;
use darkfi::{
    blockchain::{
        Blockchain, BlockchainOverlay, BlockchainOverlayPtr, block_store::append_tx_to_merkle_tree,
    },
    cli_desc,
    error::TxVerifyFailed,
    runtime::vm_runtime::Runtime,
    tx::{MAX_TX_CALLS, MIN_TX_CALLS, Transaction},
    util::path::expand_path,
    validator::{
        fees::{GasData, PALLAS_SCHNORR_SIGNATURE_FEE, circuit_gas_use, compute_fee},
        verification::verify_transaction,
    },
    zk::VerifyingKey,
};
use darkfi_sdk::{
    crypto::{ContractId, MerkleTree, PublicKey},
    dark_tree::dark_forest_leaf_vec_integrity_check,
    deploy::DeployParamsV1,
    pasta::pallas,
    tx::TransactionHash,
};
use darkfi_serial::{AsyncDecodable, AsyncEncodable, deserialize_async, serialize_async};
use smol::io::Cursor;

#[derive(Parser)]
#[command(about = cli_desc!())]
struct Args {
    #[arg(short, long)]
    database_path: String,
    #[arg(short, long)]
    tx_hash: String,
    #[arg(long, conflicts_with_all = ["zkp", "sig"])]
    wasm: bool,
    #[arg(long, conflicts_with_all = ["wasm", "sig"])]
    zkp: bool,
    #[arg(long, conflicts_with_all = ["wasm", "zkp"])]
    sig: bool,
}

fn main() {
    smol::block_on(async {
        let args = Args::parse();
        replay_tx(args).await;
    });
}

async fn replay_tx(args: Args) {
    let db_path = expand_path(&args.database_path).unwrap();
    let sled_db = sled_overlay::sled::open(&db_path).unwrap();

    let blockchain = Blockchain::new(&sled_db).unwrap();
    let txh: TransactionHash = args.tx_hash.parse().unwrap();

    let (tx_height, _) =
        blockchain.transactions.get_location(&[txh], true).unwrap().first().unwrap().unwrap();
    let block_header_hash =
        blockchain.blocks.get_order(&[tx_height], true).unwrap().first().unwrap().unwrap();
    // Get all the transactions in the block of our target tx
    let block = blockchain
        .blocks
        .get(&[block_header_hash], true)
        .unwrap()
        .first()
        .unwrap()
        .clone()
        .unwrap();
    let txs: Vec<Transaction> = blockchain
        .transactions
        .get(&block.txs, true)
        .unwrap()
        .into_iter()
        .map(|t| t.unwrap())
        .collect();

    let (overlay, new_height) = rollback_database(&blockchain, txh).await;

    // Apply all transactions upto and including our target tx
    let mut tree = MerkleTree::new(1);
    for tx in txs {
        perform_tx_verification(&tx, new_height, &overlay, &mut tree, &args).await;
        // We have applied our target tx so let's bail out
        if tx.hash() == txh {
            break;
        }
    }
}

async fn perform_tx_verification(
    tx: &Transaction,
    new_height: u32,
    overlay: &BlockchainOverlayPtr,
    tree: &mut MerkleTree,
    args: &Args,
) {
    let mut vks: HashMap<[u8; 32], HashMap<String, VerifyingKey>> = HashMap::new();
    for call in &tx.calls {
        vks.insert(call.data.contract_id.to_bytes(), HashMap::new());
    }

    let result = if args.wasm {
        verify_transaction_wasm(overlay, new_height, 2, tx, tree, &mut vks, true).await.unwrap()
    } else if args.zkp {
        verify_transaction_zkps(overlay, new_height, 2, tx, tree, &mut vks, true).await.unwrap()
    } else if args.sig {
        verify_transaction_signatures(overlay, new_height, 2, tx, tree, &mut vks, true)
            .await
            .unwrap()
    } else {
        verify_transaction(overlay, new_height, 2, tx, tree, &mut vks, true).await.unwrap()
    };

    println!("Verify Transaction Result: {:?}", result);
}

/// Resets the blockchain in memory to a height before the transaction.
async fn rollback_database(
    blockchain: &Blockchain,
    txh: TransactionHash,
) -> (BlockchainOverlayPtr, u32) {
    let (tx_height, _) =
        blockchain.transactions.get_location(&[txh], true).unwrap().first().unwrap().unwrap();

    let new_height = tx_height - 1;
    println!("Rolling back database to Height: {new_height}");

    let (last, _) = blockchain.last().unwrap();
    let heights: Vec<u32> = (new_height + 1..=last).rev().collect();
    let inverse_diffs = blockchain.blocks.get_state_inverse_diff(&heights, true).unwrap();

    let overlay = BlockchainOverlay::new(blockchain).unwrap();

    let overlay_lock = overlay.lock().unwrap();
    let mut lock = overlay_lock.overlay.lock().unwrap();
    for inverse_diff in inverse_diffs {
        let inverse_diff = inverse_diff.unwrap();
        lock.add_diff(&inverse_diff).unwrap();
    }
    drop(lock);
    drop(overlay_lock);

    (overlay, new_height)
}

async fn verify_transaction_wasm(
    overlay: &BlockchainOverlayPtr,
    verifying_block_height: u32,
    block_target: u32,
    tx: &Transaction,
    tree: &mut MerkleTree,
    _verifying_keys: &mut HashMap<[u8; 32], HashMap<String, VerifyingKey>>,
    verify_fee: bool,
) -> darkfi::Result<GasData> {
    let tx_hash = tx.hash();

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
                return Err(TxVerifyFailed::InvalidFee.into())
            }

            found_fee = true;
            fee_call_idx = call_idx;
        }

        if !found_fee {
            return Err(TxVerifyFailed::InvalidFee.into())
        }
    }

    // Write the transaction calls payload data
    let mut payload = vec![];
    tx.calls.encode_async(&mut payload).await?;

    // Define a buffer in case we want to use a different payload in a specific call
    let mut _call_payload = vec![];

    // Iterate over all calls to get the metadata
    for (idx, call) in tx.calls.iter().enumerate() {
        // Transaction must not contain a Pow reward call
        if call.data.is_money_pow_reward() {
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

        // After getting the metadata, we run the "exec" function with the same runtime
        // and the same payload. We keep the returned state update in a buffer, prefixed
        // by the call function ID, enforcing the state update function in the contract.
        let mut state_update = vec![call.data.data[0]];
        state_update.append(&mut runtime.exec(call_payload)?);

        // If that was successful, we apply the state update in the ephemeral overlay.
        runtime.apply(&state_update)?;

        // If this call is supposed to deploy a new contract, we have to instantiate
        // a new `Runtime` and run its deploy function.
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
                verifying_block_height,
                block_target,
                tx_hash,
                call_idx,
            )?;

            deploy_runtime.deploy(&deploy_params.ix)?;

            let deploy_gas_used = deploy_runtime.gas_used();
            gas_data.deployments += deploy_gas_used;
        }

        // At this point we're done with the call and move on to the next one.
        // Accumulate the WASM gas used.
        let wasm_gas_used = runtime.gas_used();

        // Append the used wasm gas
        gas_data.wasm += wasm_gas_used;
    }

    // Store the calculated total gas used to avoid recalculating it for subsequent uses
    let total_gas_used = gas_data.total_gas_used();

    if verify_fee {
        // Deserialize the fee call to find the paid fee
        let fee: u64 = match deserialize_async(&tx.calls[fee_call_idx].data.data[1..9]).await {
            Ok(v) => v,
            Err(_) => return Err(TxVerifyFailed::InvalidFee.into()),
        };

        // Compute the required fee for this transaction
        let required_fee = compute_fee(&total_gas_used);

        // Check that enough fee has been paid for the used gas in this transaction
        if required_fee > fee {
            return Err(TxVerifyFailed::InsufficientFee.into())
        }

        // Store paid fee
        gas_data.paid = fee;
    }

    // Append hash to merkle tree
    append_tx_to_merkle_tree(tree, tx);

    Ok(gas_data)
}

async fn verify_transaction_zkps(
    overlay: &BlockchainOverlayPtr,
    verifying_block_height: u32,
    block_target: u32,
    tx: &Transaction,
    tree: &mut MerkleTree,
    verifying_keys: &mut HashMap<[u8; 32], HashMap<String, VerifyingKey>>,
    verify_fee: bool,
) -> darkfi::Result<GasData> {
    let tx_hash = tx.hash();

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
                return Err(TxVerifyFailed::InvalidFee.into())
            }

            found_fee = true;
            fee_call_idx = call_idx;
        }

        if !found_fee {
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
        // Transaction must not contain a Pow reward call
        if call.data.is_money_pow_reward() {
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

        let metadata = runtime.metadata(call_payload)?;

        // Decode the metadata retrieved from the execution
        let mut decoder = Cursor::new(&metadata);

        // The tuple is (zkas_ns, public_inputs)
        let zkp_pub: Vec<(String, Vec<pallas::Base>)> =
            AsyncDecodable::decode_async(&mut decoder).await?;
        let sig_pub: Vec<PublicKey> = AsyncDecodable::decode_async(&mut decoder).await?;

        if decoder.position() != metadata.len() as u64 {
            return Err(TxVerifyFailed::ErroneousTxs(vec![tx.clone()]).into())
        }

        // Here we'll look up verifying keys and insert them into the per-contract map.
        // TODO: This vk map can potentially use a lot of RAM. Perhaps load keys on-demand at verification time?
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

        // At this point we're done with the call and move on to the next one.
        // Accumulate the WASM gas used.
        let wasm_gas_used = runtime.gas_used();

        // Append the used wasm gas
        gas_data.wasm += wasm_gas_used;
    }

    // The ZK circuit fee is calculated using a function in validator/fees.rs
    for zkbin in circuits_to_verify.iter() {
        let zk_circuit_gas_used = circuit_gas_use(zkbin);
        // Append the used zk circuit gas
        gas_data.zk_circuits += zk_circuit_gas_used;
    }

    // Store the calculated total gas used to avoid recalculating it for subsequent uses
    let total_gas_used = gas_data.total_gas_used();

    if verify_fee {
        // Deserialize the fee call to find the paid fee
        let fee: u64 = match deserialize_async(&tx.calls[fee_call_idx].data.data[1..9]).await {
            Ok(v) => v,
            Err(_) => return Err(TxVerifyFailed::InvalidFee.into()),
        };

        // Compute the required fee for this transaction
        let required_fee = compute_fee(&total_gas_used);

        // Check that enough fee has been paid for the used gas in this transaction
        if required_fee > fee {
            return Err(TxVerifyFailed::InsufficientFee.into())
        }

        // Store paid fee
        gas_data.paid = fee;
    }

    if tx.verify_zkps(verifying_keys, zkp_table).await.is_err() {
        return Err(TxVerifyFailed::InvalidZkProof.into())
    }

    // Append hash to merkle tree
    append_tx_to_merkle_tree(tree, tx);

    Ok(gas_data)
}

async fn verify_transaction_signatures(
    overlay: &BlockchainOverlayPtr,
    verifying_block_height: u32,
    block_target: u32,
    tx: &Transaction,
    tree: &mut MerkleTree,
    _verifying_keys: &mut HashMap<[u8; 32], HashMap<String, VerifyingKey>>,
    verify_fee: bool,
) -> darkfi::Result<GasData> {
    let tx_hash = tx.hash();

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
                return Err(TxVerifyFailed::InvalidFee.into())
            }

            found_fee = true;
            fee_call_idx = call_idx;
        }

        if !found_fee {
            return Err(TxVerifyFailed::InvalidFee.into())
        }
    }

    // Write the transaction calls payload data
    let mut payload = vec![];
    tx.calls.encode_async(&mut payload).await?;

    // Define a buffer in case we want to use a different payload in a specific call
    let mut _call_payload = vec![];

    // Iterate over all calls to get the metadata
    for (idx, call) in tx.calls.iter().enumerate() {
        // Transaction must not contain a Pow reward call
        if call.data.is_money_pow_reward() {
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

        let metadata = runtime.metadata(call_payload)?;

        // Decode the metadata retrieved from the execution
        let mut decoder = Cursor::new(&metadata);

        // The tuple is (zkas_ns, public_inputs)
        let zkp_pub: Vec<(String, Vec<pallas::Base>)> =
            AsyncDecodable::decode_async(&mut decoder).await?;
        let sig_pub: Vec<PublicKey> = AsyncDecodable::decode_async(&mut decoder).await?;

        if decoder.position() != metadata.len() as u64 {
            return Err(TxVerifyFailed::ErroneousTxs(vec![tx.clone()]).into())
        }

        zkp_table.push(zkp_pub);
        sig_table.push(sig_pub);

        // At this point we're done with the call and move on to the next one.
        // Accumulate the WASM gas used.
        let wasm_gas_used = runtime.gas_used();

        // Append the used wasm gas
        gas_data.wasm += wasm_gas_used;
    }

    // The signature fee is tx_size + fixed_sig_fee * n_signatures
    gas_data.signatures = (PALLAS_SCHNORR_SIGNATURE_FEE * tx.signatures.len() as u64) +
        serialize_async(tx).await.len() as u64;

    // Store the calculated total gas used to avoid recalculating it for subsequent uses
    let total_gas_used = gas_data.total_gas_used();

    if verify_fee {
        // Deserialize the fee call to find the paid fee
        let fee: u64 = match deserialize_async(&tx.calls[fee_call_idx].data.data[1..9]).await {
            Ok(v) => v,
            Err(_) => return Err(TxVerifyFailed::InvalidFee.into()),
        };

        // Compute the required fee for this transaction
        let required_fee = compute_fee(&total_gas_used);

        // Check that enough fee has been paid for the used gas in this transaction
        if required_fee > fee {
            return Err(TxVerifyFailed::InsufficientFee.into())
        }

        // Store paid fee
        gas_data.paid = fee;
    }

    // When we're done looping and executing over the tx's contract calls and
    // (optionally) made sure that enough fee was paid, we now move on with
    // verification. First we verify the transaction signatures and then we
    // verify any accompanying ZK proofs.
    if sig_table.len() != tx.signatures.len() {
        return Err(TxVerifyFailed::MissingSignatures.into())
    }

    if tx.verify_sigs(sig_table).is_err() {
        return Err(TxVerifyFailed::InvalidSignature.into())
    }

    // Append hash to merkle tree
    append_tx_to_merkle_tree(tree, tx);

    Ok(gas_data)
}
