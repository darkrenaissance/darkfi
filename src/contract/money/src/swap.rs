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

use darkfi_sdk::{
    crypto::{Coin, ContractId},
    db::{db_contains_key, db_lookup},
    error::ContractError,
    msg,
    pasta::pallas,
    ContractCall,
};
use darkfi_serial::{deserialize, serialize, Encodable, WriteExt};

use crate::{
    money_transfer_get_metadata, money_transfer_process_update, MoneyFunction, MoneyTransferParams,
    MoneyTransferUpdate, MONEY_CONTRACT_COIN_ROOTS_TREE, MONEY_CONTRACT_NULLIFIERS_TREE,
};

pub fn money_otcswap_get_metadata(
    cid: ContractId,
    call_idx: u32,
    calls: Vec<ContractCall>,
) -> Result<Vec<u8>, ContractError> {
    Ok(money_transfer_get_metadata(cid, call_idx, calls)?)
}

pub fn money_otcswap_process_instruction(
    cid: ContractId,
    call_idx: u32,
    calls: Vec<ContractCall>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx as usize];
    let params: MoneyTransferParams = deserialize(&self_.data[1..])?;

    // State transition for OTC atomic swaps.
    // We enforce 2 inputs and 2 outputs so every atomic swap looks the same.
    if !params.clear_inputs.is_empty() {
        msg!("[OtcSwap] Error: Clear inputs are not empty");
        return Err(ContractError::Custom(12))
    }

    if params.inputs.len() != 2 {
        msg!("[OtcSwap] Error: Expected 2 inputs");
        return Err(ContractError::Custom(13))
    }

    if params.outputs.len() != 2 {
        msg!("[OtcSwap] Error: Expected 2 outputs");
        return Err(ContractError::Custom(14))
    }

    let nullifiers_db = db_lookup(cid, MONEY_CONTRACT_NULLIFIERS_TREE)?;
    let coin_roots_db = db_lookup(cid, MONEY_CONTRACT_COIN_ROOTS_TREE)?;

    let mut new_nullifiers = Vec::with_capacity(2);

    // inputs[0] is being swapped to outputs[1]
    // inputs[1] is being swapped to outputs[0]
    // So that's how we check the value and token commitments
    if params.inputs[0].value_commit != params.outputs[1].value_commit {
        msg!("[OtcSwap] Error: Value commitments for input 0 and output 1 do not match");
        return Err(ContractError::Custom(10))
    }

    if params.inputs[1].value_commit != params.outputs[0].value_commit {
        msg!("[OtcSwap] Error: Value commitments for input 1 and output 0 do not match");
        return Err(ContractError::Custom(10))
    }

    if params.inputs[0].token_commit != params.outputs[1].token_commit {
        msg!("[OtcSwap] Error: Token commitments for input 0 and output 1 do not match");
        return Err(ContractError::Custom(11))
    }

    if params.inputs[1].token_commit != params.outputs[0].token_commit {
        msg!("[OtcSwap] Error: Token commitments for input 1 and output 0 do not match");
        return Err(ContractError::Custom(11))
    }

    msg!("[OtcSwap] Iternating over anonymous inputs");
    for (i, input) in params.inputs.iter().enumerate() {
        // For now, make sure that the inputs' spend hooks are zero.
        // This should however be allowed to some extent.
        if input.spend_hook != pallas::Base::zero() {
            msg!("[OtcSwap] Error: Unable to swap coins with spend_hook != 0 (input {})", i);
            return Err(ContractError::Custom(17))
        }

        // The Merkle root is used to know whether this coin existed
        // in a previous state.
        if !db_contains_key(coin_roots_db, &serialize(&input.merkle_root))? {
            msg!("[OtcSwap] Error: Merkle root not found in previous state (input {})", i);
            return Err(ContractError::Custom(5))
        }

        // The nullifiers should not already exist. It is the double-spend protection.
        if new_nullifiers.contains(&input.nullifier) ||
            db_contains_key(nullifiers_db, &serialize(&input.nullifier))?
        {
            msg!("[OtcSwap] Error: Duplicate nullifier found in input {}", i);
            return Err(ContractError::Custom(6))
        }

        new_nullifiers.push(input.nullifier);
    }

    // Newly created coins for this transaction are in the outputs.
    let mut new_coins = Vec::with_capacity(2);
    for (i, output) in params.outputs.iter().enumerate() {
        // TODO: Coins should exist in a sled tree in order to check dupes.
        if new_coins.contains(&Coin::from(output.coin)) {
            msg!("[OtcSwap] Error: Duplicate coin found in output {}", i);
            return Err(ContractError::Custom(9))
        }

        new_coins.push(Coin::from(output.coin));
    }

    // Create a state update. We also use the `MoneyTransferUpdate` because
    // they're essentially the same thing, just with a different transition
    // rule set.
    let update = MoneyTransferUpdate { nullifiers: new_nullifiers, coins: new_coins };
    let mut update_data = vec![];
    update_data.write_u8(MoneyFunction::OtcSwap as u8)?;
    update.encode(&mut update_data)?;

    Ok(update_data)
}

pub fn money_otcswap_process_update(
    cid: ContractId,
    update: MoneyTransferUpdate,
) -> Result<(), ContractError> {
    Ok(money_transfer_process_update(cid, update)?)
}
