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
    crypto::{
        pasta_prelude::*, pedersen_commitment_base, pedersen_commitment_u64, Coin, ContractId,
        MerkleNode, PublicKey, DARK_TOKEN_ID,
    },
    db::{db_contains_key, db_get, db_lookup, db_set},
    error::ContractError,
    merkle_add, msg,
    pasta::pallas,
    ContractCall,
};
use darkfi_serial::{deserialize, serialize, Encodable, WriteExt};

use crate::{
    MoneyFunction, MoneyTransferParams, MoneyTransferUpdate, MONEY_CONTRACT_COIN_MERKLE_TREE,
    MONEY_CONTRACT_COIN_ROOTS_TREE, MONEY_CONTRACT_FAUCET_PUBKEYS, MONEY_CONTRACT_INFO_TREE,
    MONEY_CONTRACT_NULLIFIERS_TREE, MONEY_CONTRACT_ZKAS_BURN_NS_V1, MONEY_CONTRACT_ZKAS_MINT_NS_V1,
};

pub fn money_transfer_get_metadata(
    _cid: ContractId,
    call_idx: u32,
    calls: Vec<ContractCall>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx as usize];
    let params: MoneyTransferParams = deserialize(&self_.data[1..])?;

    let mut zk_public_values: Vec<(String, Vec<pallas::Base>)> = vec![];
    let mut signature_pubkeys: Vec<PublicKey> = vec![];

    for input in &params.clear_inputs {
        signature_pubkeys.push(input.signature_public);
    }

    for input in &params.inputs {
        let value_coords = input.value_commit.to_affine().coordinates().unwrap();
        let token_coords = input.token_commit.to_affine().coordinates().unwrap();
        let (sig_x, sig_y) = input.signature_public.xy();

        zk_public_values.push((
            MONEY_CONTRACT_ZKAS_BURN_NS_V1.to_string(),
            vec![
                input.nullifier.inner(),
                *value_coords.x(),
                *value_coords.y(),
                *token_coords.x(),
                *token_coords.y(),
                input.merkle_root.inner(),
                input.user_data_enc,
                sig_x,
                sig_y,
            ],
        ));

        signature_pubkeys.push(input.signature_public);
    }

    for output in &params.outputs {
        let value_coords = output.value_commit.to_affine().coordinates().unwrap();
        let token_coords = output.token_commit.to_affine().coordinates().unwrap();

        zk_public_values.push((
            MONEY_CONTRACT_ZKAS_MINT_NS_V1.to_string(),
            vec![
                output.coin,
                *value_coords.x(),
                *value_coords.y(),
                *token_coords.x(),
                *token_coords.y(),
            ],
        ));
    }

    let mut metadata = vec![];
    zk_public_values.encode(&mut metadata)?;
    signature_pubkeys.encode(&mut metadata)?;

    Ok(metadata)
}

pub fn money_transfer_process_instruction(
    cid: ContractId,
    call_idx: u32,
    calls: Vec<ContractCall>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx as usize];
    let params: MoneyTransferParams = deserialize(&self_.data[1..])?;

    if params.clear_inputs.len() + params.inputs.len() < 1 {
        msg!("[Transfer] Error: No inputs in the call");
        return Err(ContractError::Custom(1))
    }

    if params.outputs.is_empty() {
        msg!("[Transfer] Error: No outputs in the call");
        return Err(ContractError::Custom(2))
    }

    let info_db = db_lookup(cid, MONEY_CONTRACT_INFO_TREE)?;
    let nullifiers_db = db_lookup(cid, MONEY_CONTRACT_NULLIFIERS_TREE)?;
    let coin_roots_db = db_lookup(cid, MONEY_CONTRACT_COIN_ROOTS_TREE)?;

    let Some(faucet_pubkeys) = db_get(info_db, &serialize(&MONEY_CONTRACT_FAUCET_PUBKEYS))? else {
        msg!("[Transfer] Error: Missing faucet pubkeys from info db");
        return Err(ContractError::Internal)
    };
    let faucet_pubkeys: Vec<PublicKey> = deserialize(&faucet_pubkeys)?;

    // Accumulator for the value commitments
    let mut valcom_total = pallas::Point::identity();

    // State transition for payments
    msg!("[Transfer] Iterating over clear inputs");
    for (i, input) in params.clear_inputs.iter().enumerate() {
        if input.token_id != *DARK_TOKEN_ID {
            msg!("[Transfer] Error: Clear input {} used non-native token", i);
            return Err(ContractError::Custom(3))
        }

        if !faucet_pubkeys.contains(&input.signature_public) {
            msg!("[Transfer] Error: Clear input {} used unauthorised pubkey", i);
            return Err(ContractError::Custom(4))
        }

        valcom_total += pedersen_commitment_u64(input.value, input.value_blind);
    }

    let mut new_nullifiers = Vec::with_capacity(params.inputs.len());
    msg!("[Transfer] Iterating over anonymous inputs");
    for (i, input) in params.inputs.iter().enumerate() {
        // The Merkle root is used to know whether this is a coin that
        // existed in a previous state.
        if !db_contains_key(coin_roots_db, &serialize(&input.merkle_root))? {
            msg!("[Transfer] Error: Merkle root not found in previous state (input {})", i);
            return Err(ContractError::Custom(5))
        }

        // The nullifiers should not already exist. It is the double-spend protection.
        if new_nullifiers.contains(&input.nullifier) ||
            db_contains_key(nullifiers_db, &serialize(&input.nullifier))?
        {
            msg!("[Transfer] Error: Duplicate nullifier found in input {}", i);
            return Err(ContractError::Custom(6))
        }

        // Check the invoked contract if spend hook is set
        if input.spend_hook != pallas::Base::zero() {
            let next_call_idx = call_idx + 1;
            if next_call_idx >= calls.len() as u32 {
                msg!(
                    "[Transfer] Error: next_call_idx={} but len(calls)={} (input {})",
                    next_call_idx,
                    calls.len(),
                    i
                );
                return Err(ContractError::Custom(7))
            }

            let next = &calls[next_call_idx as usize];
            if next.contract_id.inner() != input.spend_hook {
                msg!("[Transfer] Error: Invoking contract call does not match spend hook in input {}", i);
                return Err(ContractError::Custom(8))
            }
        }

        new_nullifiers.push(input.nullifier);
        valcom_total += input.value_commit;
    }

    // Newly created coins for this transaction are in the outputs.
    let mut new_coins = Vec::with_capacity(params.outputs.len());
    for (i, output) in params.outputs.iter().enumerate() {
        // TODO: Coins should exist in a sled tree in order to check dupes.
        if new_coins.contains(&Coin::from(output.coin)) {
            msg!("[Transfer] Error: Duplicate coin found in output {}", i);
            return Err(ContractError::Custom(9))
        }

        // FIXME: Needs some work on types and their place within all these libraries
        new_coins.push(Coin::from(output.coin));
        valcom_total -= output.value_commit;
    }

    // If the accumulator is not back in its initial state, there's a value mismatch.
    if valcom_total != pallas::Point::identity() {
        msg!("[Transfer] Error: Value commitments do not result in identity");
        return Err(ContractError::Custom(10))
    }

    // Verify that the token commitments are all for the same token
    let tokcom = params.outputs[0].token_commit;
    let mut failed_tokcom = params.inputs.iter().any(|input| input.token_commit != tokcom);
    failed_tokcom =
        failed_tokcom || params.outputs.iter().any(|output| output.token_commit != tokcom);
    failed_tokcom = failed_tokcom ||
        params.clear_inputs.iter().any(|input| {
            pedersen_commitment_base(input.token_id.inner(), input.token_blind) != tokcom
        });

    if failed_tokcom {
        msg!("[Transfer] Error: Token commitments do not match");
        return Err(ContractError::Custom(11))
    }

    // Create a state update
    let update = MoneyTransferUpdate { nullifiers: new_nullifiers, coins: new_coins };
    let mut update_data = vec![];
    update_data.write_u8(MoneyFunction::Transfer as u8)?;
    update.encode(&mut update_data)?;

    Ok(update_data)
}

pub fn money_transfer_process_update(
    cid: ContractId,
    update: MoneyTransferUpdate,
) -> Result<(), ContractError> {
    let info_db = db_lookup(cid, MONEY_CONTRACT_INFO_TREE)?;
    let nullifiers_db = db_lookup(cid, MONEY_CONTRACT_NULLIFIERS_TREE)?;
    let coin_roots_db = db_lookup(cid, MONEY_CONTRACT_COIN_ROOTS_TREE)?;

    msg!("[Transfer] Adding new nullifiers to the set");
    for nullifier in update.nullifiers {
        db_set(nullifiers_db, &serialize(&nullifier), &[])?;
    }

    let coins: Vec<_> = update.coins.iter().map(|x| MerkleNode::from(x.inner())).collect();

    msg!("[Transfer] Adding new coins to Merkle tree");
    merkle_add(info_db, coin_roots_db, &serialize(&MONEY_CONTRACT_COIN_MERKLE_TREE), &coins)?;

    Ok(())
}
