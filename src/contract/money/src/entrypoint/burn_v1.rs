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
        ContractId, FuncId, FuncRef, PublicKey,
    },
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg,
    pasta::pallas,
    wasm, ContractCall,
};
use darkfi_serial::{deserialize, serialize, Encodable};

use crate::{
    error::MoneyError,
    model::{MoneyBurnParamsV1, MoneyBurnUpdateV1},
    MONEY_CONTRACT_COIN_ROOTS_TREE, MONEY_CONTRACT_INFO_TREE, MONEY_CONTRACT_LATEST_NULLIFIER_ROOT,
    MONEY_CONTRACT_NULLIFIERS_TREE, MONEY_CONTRACT_NULLIFIER_ROOTS_TREE,
    MONEY_CONTRACT_ZKAS_BURN_NS_V1,
};

/// `get_metadata` function for `Money::BurnV1`
pub(crate) fn money_burn_get_metadata_v1(
    _cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: MoneyBurnParamsV1 = deserialize(&self_.data[1..])?;

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

    // No outputs - this is a burn, value is destroyed.

    // Serialize everything gathered and return it
    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    signature_pubkeys.encode(&mut metadata)?;

    Ok(metadata)
}

/// `process_instruction` function for `Money::BurnV1`
pub(crate) fn money_burn_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx];
    let params: MoneyBurnParamsV1 = deserialize(&self_.data.data[1..])?;

    if params.inputs.is_empty() {
        msg!("[BurnV1] Error: No inputs in the call");
        return Err(MoneyError::BurnMissingInputs.into())
    }

    // Access the necessary databases where there is information to
    // validate this state transition.
    let nullifiers_db = wasm::db::db_lookup(cid, MONEY_CONTRACT_NULLIFIERS_TREE)?;
    let coin_roots_db = wasm::db::db_lookup(cid, MONEY_CONTRACT_COIN_ROOTS_TREE)?;

    let hasher = PoseidonFp::new();
    let empty_leaf = pallas::Base::ZERO;
    let smt_store = SmtWasmDbStorage::new(nullifiers_db);
    let smt = SmtWasmFp::new(smt_store, hasher, &EMPTY_NODES_FP);

    // Grab the expected token commitment. All inputs must use the
    // same token type.
    let tokcom = params.inputs[0].token_commit;

    // ===================================
    // Perform the actual state transition
    // ===================================

    let mut new_nullifiers = Vec::with_capacity(params.inputs.len());

    msg!("[BurnV1] Iterating over anonymous inputs");
    for (i, input) in params.inputs.iter().enumerate() {
        // The Merkle root is used to know whether this is a coin that
        // existed in a previous state.
        if input.intra_tx {
            if !wasm::tx_local::coin_roots_contains(&input.merkle_root)? {
                msg!("[BurnV1] Error: Merkle root not found in tx-local state (input {})", i);
                return Err(MoneyError::TransferMerkleRootNotFound.into())
            }
        } else {
            if !wasm::db::db_contains_key(coin_roots_db, &serialize(&input.merkle_root))? {
                msg!("[BurnV1] Error: Merkle root not found in previous state (input {})", i);
                return Err(MoneyError::TransferMerkleRootNotFound.into())
            }
        }

        // The nullifiers should not already exist. It is the double-spend protection.
        if new_nullifiers.contains(&input.nullifier) ||
            smt.get_leaf(&input.nullifier.inner()) != empty_leaf
        {
            msg!("[BurnV1] Error: Duplicate nullifier found in input {}", i);
            return Err(MoneyError::DuplicateNullifier.into())
        }

        // Verify the token commitment is the expected one
        if tokcom != input.token_commit {
            msg!("[BurnV1] Error: Token commitment mismatch in input {}", i);
            return Err(MoneyError::TokenMismatch.into())
        }

        new_nullifiers.push(input.nullifier);
    }

    // No outputs, no value commitment balance check.
    // The value committed in the inputs is permanently destroyed.

    let update = MoneyBurnUpdateV1 { nullifiers: new_nullifiers };
    Ok(serialize(&update))
}

/// `process_update` function for `Money::BurnV1`
pub(crate) fn money_burn_process_update_v1(
    cid: ContractId,
    update: MoneyBurnUpdateV1,
) -> ContractResult {
    let info_db = wasm::db::db_lookup(cid, MONEY_CONTRACT_INFO_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, MONEY_CONTRACT_NULLIFIERS_TREE)?;
    let nullifier_roots_db = wasm::db::db_lookup(cid, MONEY_CONTRACT_NULLIFIER_ROOTS_TREE)?;

    msg!("[BurnV1] Adding new nullifiers to the set");
    wasm::merkle::sparse_merkle_insert_batch(
        info_db,
        nullifiers_db,
        nullifier_roots_db,
        MONEY_CONTRACT_LATEST_NULLIFIER_ROOT,
        &update.nullifiers.iter().map(|n| n.inner()).collect::<Vec<_>>(),
    )?;

    Ok(())
}
