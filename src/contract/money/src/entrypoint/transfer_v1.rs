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

use darkfi_sdk::{
    crypto::{
        pasta_prelude::*,
        smt::{
            wasmdb::{SmtWasmDbStorage, SmtWasmFp},
            PoseidonFp, EMPTY_NODES_FP,
        },
        ContractId, FuncId, FuncRef, MerkleNode, PublicKey,
    },
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg,
    pasta::pallas,
    wasm::{
        self,
        db::{
            db_contains_key, db_contains_key_local, db_lookup, db_lookup_local, db_set,
            db_set_local,
        },
    },
    ContractCall,
};
use darkfi_serial::{deserialize, serialize, Encodable};

use crate::{
    error::MoneyError,
    model::{MoneyTransferParamsV1, MoneyTransferUpdateV1},
    MONEY_CONTRACT_COINS_TREE, MONEY_CONTRACT_COIN_MERKLE_TREE, MONEY_CONTRACT_COIN_ROOTS_TREE,
    MONEY_CONTRACT_INFO_TREE, MONEY_CONTRACT_LATEST_COIN_ROOT,
    MONEY_CONTRACT_LATEST_NULLIFIER_ROOT, MONEY_CONTRACT_NULLIFIERS_TREE,
    MONEY_CONTRACT_NULLIFIER_ROOTS_TREE, MONEY_CONTRACT_ZKAS_BURN_NS_V1,
    MONEY_CONTRACT_ZKAS_MINT_NS_V1,
};

/// Helper function to find or create a group for a given token ID
/// commitment.
/// Returns a mutable reference to the accumulator.
fn get_or_insert_group(
    groups: &mut Vec<(pallas::Base, pallas::Point)>,
    tokcom: pallas::Base,
) -> &mut pallas::Point {
    let pos = groups.iter().position(|(tc, _)| *tc == tokcom).unwrap_or_else(|| {
        groups.push((tokcom, pallas::Point::identity()));
        groups.len() - 1
    });

    &mut groups[pos].1
}

/// `get_metadata` function for `Money::TransferV1`
pub(crate) fn money_transfer_get_metadata_v1(
    _cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: MoneyTransferParamsV1 = deserialize(&self_.data[1..])?;

    // Public inputs for the ZK proofs we have to verify
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    // Public keys for the transaction signatures we have to verify
    let mut signature_pubkeys: Vec<PublicKey> = vec![];

    // Calculate the spend hook
    let spend_hook = match calls[call_idx].parent_index {
        Some(parent_idx) => {
            let parent_call = &calls[parent_idx].data;
            let contract_id = parent_call.contract_id;
            let func_code = parent_call.data[0];

            FuncRef { contract_id, func_code }.to_func_id()
        }
        None => FuncId::none(),
    };

    // Grab the pedersen commitments and signature pubkeys from the
    // anonymous inputs
    for input in &params.inputs {
        let value_coords = input.value_commit.to_affine().coordinates().unwrap();
        let (sig_x, sig_y) = input.signature_public.xy();

        // It is very important that these are in the same order as the
        // `constrain_instance` calls in the zkas code.
        // Otherwise verification will fail.
        zk_public_inputs.push((
            MONEY_CONTRACT_ZKAS_BURN_NS_V1.to_string(),
            vec![
                input.nullifier.inner(),
                *value_coords.x(),
                *value_coords.y(),
                input.token_commit,
                input.merkle_root.inner(),
                input.user_data_enc,
                spend_hook.inner(),
                sig_x,
                sig_y,
            ],
        ));

        signature_pubkeys.push(input.signature_public);
    }

    // Grab the pedersen commitments from the anonymous outputs
    for output in &params.outputs {
        let value_coords = output.value_commit.to_affine().coordinates().unwrap();

        zk_public_inputs.push((
            MONEY_CONTRACT_ZKAS_MINT_NS_V1.to_string(),
            vec![output.coin.inner(), *value_coords.x(), *value_coords.y(), output.token_commit],
        ));
    }

    // Serialize everything gathered and return it
    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    signature_pubkeys.encode(&mut metadata)?;

    Ok(metadata)
}

/// `process_instruction` function for `Money::TransferV1`
pub(crate) fn money_transfer_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx];
    let params: MoneyTransferParamsV1 = deserialize(&self_.data.data[1..])?;

    if params.inputs.is_empty() {
        msg!("[TransferV1] Error: No inputs in the call");
        return Err(MoneyError::TransferMissingInputs.into())
    }

    if params.outputs.is_empty() {
        msg!("[TransferV1] Error: No outputs in the call");
        return Err(MoneyError::TransferMissingOutputs.into())
    }

    // Access the necessary databases where there is information to
    // validate this state transition.
    let coins_db = db_lookup(cid, MONEY_CONTRACT_COINS_TREE)?;
    let coins_db_local = db_lookup_local(cid, MONEY_CONTRACT_COINS_TREE)?;

    let coin_roots_db = db_lookup(cid, MONEY_CONTRACT_COIN_ROOTS_TREE)?;
    let coin_roots_db_local = db_lookup_local(cid, MONEY_CONTRACT_COIN_ROOTS_TREE)?;

    let nullifiers_db = db_lookup(cid, MONEY_CONTRACT_NULLIFIERS_TREE)?;

    // Initialize the sparse Merkle tree for nullifier storage
    let hasher = PoseidonFp::new();
    let empty_leaf = pallas::Base::ZERO;
    let smt_store = SmtWasmDbStorage::new(nullifiers_db);
    let smt = SmtWasmFp::new(smt_store, hasher, &EMPTY_NODES_FP);

    // Collect all distinct token commitments and build per-token accumulators.
    // We track (token_commit -> value_commit accumulator) pairs.
    // For each token group, inputs and outputs subtract.
    // Every group must independently sum to identity.
    let mut token_groups: Vec<(pallas::Base, pallas::Point)> = vec![];

    // ===================================
    // Perform the actual state transition
    // ===================================

    // Keep track of new introduced nullifiers
    let mut new_nullifiers = Vec::with_capacity(params.inputs.len());

    // For anonymous inputs, we must gather all the new nullifiers
    // that are introduced, and verify their token commitments.
    msg!("[TransferV1] Iterating over anonymous inputs");
    for (i, input) in params.inputs.iter().enumerate() {
        // The Merkle root is used to know whether this is a coin that
        // existed in a previous state.
        //
        // If the input was created from a tx-local output, we will check
        // it against the tx-local Merkle tree, otherwise we will check
        // it against the on-chain Merkle tree.
        let merkle_root_ser = serialize(&input.merkle_root);
        if input.tx_local {
            if !db_contains_key_local(coin_roots_db_local, &merkle_root_ser)? {
                msg!("[TransferV1] Error: Merkle root not found in tx-local state (input {})", i);
                return Err(MoneyError::TransferMerkleRootNotFound.into())
            }
        } else if !db_contains_key(coin_roots_db, &merkle_root_ser)? {
            msg!("[TransferV1] Error: Merkle root not found in on-chain state (input {})", i);
            return Err(MoneyError::TransferMerkleRootNotFound.into())
        }

        // The nullifier should not already exist. It's the double-spend protection.
        // Nullifiers are always written on-chain regardless if tx-local or not.
        if new_nullifiers.contains(&input.nullifier) ||
            smt.get_leaf(&input.nullifier.inner()) != empty_leaf
        {
            msg!("[TransferV1] Error: Duplicate nullifier found in input {}", i);
            return Err(MoneyError::DuplicateNullifier.into())
        }

        // Accumulate the value commitment
        let acc = get_or_insert_group(&mut token_groups, input.token_commit);
        *acc += input.value_commit;

        // Append this new nullifier to seen nullifiers
        new_nullifiers.push(input.nullifier);
    }

    // Newly created coins for this call are in the outputs.
    // Here we gather them, check that they haven't existed before and their
    // token commitment is valid.
    let mut new_global_coins = Vec::new();
    let mut new_local_coins = Vec::new();
    msg!("[TransferV1] Iterating over anonymous outputs");
    for (i, output) in params.outputs.iter().enumerate() {
        let coin_ser = serialize(&output.coin);
        if new_global_coins.contains(&output.coin) ||
            new_local_coins.contains(&output.coin) ||
            db_contains_key(coins_db, &coin_ser)? ||
            db_contains_key_local(coins_db_local, &coin_ser)?
        {
            msg!("[TransferV1] Error: Duplicate coin found in output {}", i);
            return Err(MoneyError::DuplicateCoin.into())
        }

        // Subtract the value commitment
        let acc = get_or_insert_group(&mut token_groups, output.token_commit);
        *acc -= output.value_commit;

        // Append this new coin to seen coins
        if output.tx_local {
            new_local_coins.push(output.coin);
        } else {
            new_global_coins.push(output.coin);
        }
    }

    // Verify all token groups balance. The accumulators should be back
    // in their initial state, otherwise it means there is a value mismatch
    // between inputs and outputs.
    for (i, (_tokcom, acc)) in token_groups.iter().enumerate() {
        if *acc != pallas::Point::identity() {
            msg!("[TransferV1] Error: Value commitments for token group {} do not balance", i);
            return Err(MoneyError::ValueMismatch.into())
        }
    }

    // At this point the state transition has passed. Create a state update.
    let update = MoneyTransferUpdateV1 {
        nullifiers: new_nullifiers,
        global_coins: new_global_coins,
        local_coins: new_local_coins,
    };

    // Return it
    Ok(serialize(&update))
}

/// `process_update` function for `Money::TransferV1`
pub(crate) fn money_transfer_process_update_v1(
    cid: ContractId,
    update: MoneyTransferUpdateV1,
) -> ContractResult {
    // Grab all necessary db handles for where we want to write
    let info_db = db_lookup(cid, MONEY_CONTRACT_INFO_TREE)?;
    let info_db_local = db_lookup_local(cid, MONEY_CONTRACT_INFO_TREE)?;

    let coins_db = db_lookup(cid, MONEY_CONTRACT_COINS_TREE)?;
    let coins_db_local = db_lookup_local(cid, MONEY_CONTRACT_COINS_TREE)?;

    let nullifiers_db = db_lookup(cid, MONEY_CONTRACT_NULLIFIERS_TREE)?;
    let nullifier_roots_db = db_lookup(cid, MONEY_CONTRACT_NULLIFIER_ROOTS_TREE)?;

    let coin_roots_db = db_lookup(cid, MONEY_CONTRACT_COIN_ROOTS_TREE)?;
    let coin_roots_db_local = db_lookup_local(cid, MONEY_CONTRACT_COIN_ROOTS_TREE)?;

    msg!("[TransferV1] Adding new nullifiers to the set");
    wasm::merkle::sparse_merkle_insert_batch(
        info_db,
        nullifiers_db,
        nullifier_roots_db,
        MONEY_CONTRACT_LATEST_NULLIFIER_ROOT,
        &update.nullifiers.iter().map(|n| n.inner()).collect::<Vec<_>>(),
    )?;

    msg!("[TransferV1] Adding new coins to the set");
    for coin in &update.global_coins {
        db_set(coins_db, &serialize(coin), &[])?;
    }

    for coin in &update.local_coins {
        db_set_local(coins_db_local, &serialize(coin), &[])?;
    }

    if !update.global_coins.is_empty() {
        msg!("[TransferV1] Adding new coins to on-chain Merkle tree");
        wasm::merkle::merkle_add(
            info_db,
            coin_roots_db,
            MONEY_CONTRACT_LATEST_COIN_ROOT,
            MONEY_CONTRACT_COIN_MERKLE_TREE,
            &update
                .global_coins
                .iter()
                .map(|c| MerkleNode::from(c.inner()))
                .collect::<Vec<MerkleNode>>(),
        )?;
    }

    if !update.local_coins.is_empty() {
        msg!("[TransferV1] Adding new coins to tx-local Merkle tree");
        wasm::merkle::merkle_add_local(
            info_db_local,
            coin_roots_db_local,
            MONEY_CONTRACT_LATEST_COIN_ROOT,
            MONEY_CONTRACT_COIN_MERKLE_TREE,
            &update
                .local_coins
                .iter()
                .map(|c| MerkleNode::from(c.inner()))
                .collect::<Vec<MerkleNode>>(),
        )?;
    }

    Ok(())
}
