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

use darkfi_sdk::{
    crypto::{
        pasta_prelude::*,
        smt::{
            wasmdb::{SmtWasmDbStorage, SmtWasmFp},
            PoseidonFp, EMPTY_NODES_FP,
        },
        ContractId,
    },
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg,
    pasta::pallas,
    wasm, ContractCall,
};
use darkfi_serial::{deserialize, serialize};

use super::transfer_v1::{money_transfer_get_metadata_v1, money_transfer_process_update_v1};
use crate::{
    error::MoneyError,
    model::{MoneyTransferParamsV1, MoneyTransferUpdateV1},
    MONEY_CONTRACT_COINS_TREE, MONEY_CONTRACT_COIN_ROOTS_TREE, MONEY_CONTRACT_NULLIFIERS_TREE,
};

/// `get_metadata` function for `Money::OtcSwapV1`
pub(crate) fn money_otcswap_get_metadata_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    // In here we can use the same function as we use in `TransferV1`.
    money_transfer_get_metadata_v1(cid, call_idx, calls)
}

/// `process_instruction` function for `Money::OtcSwapV1`
pub(crate) fn money_otcswap_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: MoneyTransferParamsV1 = deserialize(&self_.data[1..])?;

    // The atomic swap is able to use the same parameters as `TransferV1`.
    // In here we just have a different state transition where we enforce
    // 2 anonymous inputs and 2 anonymous outputs. This is enforced so that
    // every atomic swap looks the same on the network, therefore there is
    // no special anonymity leak for different swaps that are being done,
    // at least in the scope of this contract call.
    if params.inputs.len() != 2 {
        msg!("[OtcSwapV1] Error: Expected 2 inputs");
        return Err(MoneyError::InvalidNumberOfInputs.into())
    }

    if params.outputs.len() != 2 {
        msg!("[OtcSwapV1] Error: Expected 2 outputs");
        return Err(MoneyError::InvalidNumberOfOutputs.into())
    }

    // Grab the db handles we'll be using here
    let coins_db = wasm::db::db_lookup(cid, MONEY_CONTRACT_COINS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, MONEY_CONTRACT_NULLIFIERS_TREE)?;
    let coin_roots_db = wasm::db::db_lookup(cid, MONEY_CONTRACT_COIN_ROOTS_TREE)?;

    // We expect two new nullifiers and two new coins
    let mut new_nullifiers = Vec::with_capacity(2);
    let mut new_coins = Vec::with_capacity(2);

    // inputs[0] is being swapped to outputs[1]
    // inputs[1] is being swapped to outputs[0]
    // so that's how we check the value and token commitments.
    if params.inputs[0].value_commit != params.outputs[1].value_commit {
        msg!("[OtcSwapV1] Error: Value commitments for input 0 and output 1 mismatch");
        return Err(MoneyError::ValueMismatch.into())
    }

    if params.inputs[1].value_commit != params.outputs[0].value_commit {
        msg!("[OtcSwapV1] Error: Value commitments for input 1 and ouptut 0 mismatch");
        return Err(MoneyError::ValueMismatch.into())
    }

    if params.inputs[0].token_commit != params.outputs[1].token_commit {
        msg!("[OtcSwapV1] Error: Token commitments for input 0 and output 1 mismatch");
        return Err(MoneyError::TokenMismatch.into())
    }

    if params.inputs[1].token_commit != params.outputs[0].token_commit {
        msg!("[OtcSwapV1] Error: Token commitments for input 1 and output 0 mismatch");
        return Err(MoneyError::TokenMismatch.into())
    }

    let hasher = PoseidonFp::new();
    let empty_leaf = pallas::Base::ZERO;
    let smt_store = SmtWasmDbStorage::new(nullifiers_db);
    let smt = SmtWasmFp::new(smt_store, hasher, &EMPTY_NODES_FP);

    msg!("[OtcSwapV1] Iterating over anonymous inputs");
    for (i, input) in params.inputs.iter().enumerate() {
        // The Merkle root is used to know whether this coin
        // has existed in a previous state.
        if !wasm::db::db_contains_key(coin_roots_db, &serialize(&input.merkle_root))? {
            msg!("[OtcSwapV1] Error: Merkle root not found in previous state (input {})", i);
            return Err(MoneyError::SwapMerkleRootNotFound.into())
        }

        // The nullifiers should not already exist. It is the double-spend protection.
        if new_nullifiers.contains(&input.nullifier) ||
            smt.get_leaf(&input.nullifier.inner()) != empty_leaf
        {
            msg!("[OtcSwapV1] Error: Duplicate nullifier found in input {}", i);
            return Err(MoneyError::DuplicateNullifier.into())
        }

        new_nullifiers.push(input.nullifier);
    }

    // Newly created coins for this call are in the outputs
    for (i, output) in params.outputs.iter().enumerate() {
        if new_coins.contains(&output.coin) ||
            wasm::db::db_contains_key(coins_db, &serialize(&output.coin))?
        {
            msg!("[OtcSwapV1] Error: Duplicate coin found in output {}", i);
            return Err(MoneyError::DuplicateCoin.into())
        }

        new_coins.push(output.coin);
    }

    // Create a state update. We also use `MoneyTransferUpdateV1` because
    // they're essentially the same thing, just with a different transition
    // ruleset.
    let update = MoneyTransferUpdateV1 { nullifiers: new_nullifiers, coins: new_coins };
    Ok(serialize(&update))
}

/// `process_update` function for `Money::OtcSwapV1`
pub(crate) fn money_otcswap_process_update_v1(
    cid: ContractId,
    update: MoneyTransferUpdateV1,
) -> ContractResult {
    // In here we can use the same function as we use in `TransferV1`.
    money_transfer_process_update_v1(cid, update)
}
