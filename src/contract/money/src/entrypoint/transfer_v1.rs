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
        pasta_prelude::*, pedersen_commitment_base, pedersen_commitment_u64, ContractId,
        MerkleNode, PublicKey, DARK_TOKEN_ID,
    },
    db::{db_contains_key, db_get, db_lookup, db_set},
    error::{ContractError, ContractResult},
    merkle_add, msg,
    pasta::pallas,
    ContractCall,
};
use darkfi_serial::{deserialize, serialize, Encodable, WriteExt};

use crate::{
    error::MoneyError,
    model::{MoneyTransferParamsV1, MoneyTransferUpdateV1},
    MoneyFunction, MONEY_CONTRACT_COINS_TREE, MONEY_CONTRACT_COIN_MERKLE_TREE,
    MONEY_CONTRACT_COIN_ROOTS_TREE, MONEY_CONTRACT_FAUCET_PUBKEYS, MONEY_CONTRACT_INFO_TREE,
    MONEY_CONTRACT_NULLIFIERS_TREE, MONEY_CONTRACT_ZKAS_BURN_NS_V1, MONEY_CONTRACT_ZKAS_MINT_NS_V1,
};

/// `get_metadata` function for `Money::TransferV1`
pub(crate) fn money_transfer_get_metadata_v1(
    _cid: ContractId,
    call_idx: u32,
    calls: Vec<ContractCall>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx as usize];
    let params: MoneyTransferParamsV1 = deserialize(&self_.data[1..])?;

    // Public inputs for the ZK proofs we have to verify
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    // Public keys for the transaction signatures we have to verify
    let mut signature_pubkeys: Vec<PublicKey> = vec![];

    // Take all the pubkeys from any clear inputs
    for input in &params.clear_inputs {
        signature_pubkeys.push(input.signature_public);
    }

    // Grab the pedersen commitments and signature pubkeys from the
    // anonymous inputs
    for input in &params.inputs {
        let value_coords = input.value_commit.to_affine().coordinates().unwrap();
        let token_coords = input.token_commit.to_affine().coordinates().unwrap();
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

    // Grab the pedersen commitments from the anonymous outputs
    for output in &params.outputs {
        let value_coords = output.value_commit.to_affine().coordinates().unwrap();
        let token_coords = output.token_commit.to_affine().coordinates().unwrap();

        zk_public_inputs.push((
            MONEY_CONTRACT_ZKAS_MINT_NS_V1.to_string(),
            vec![
                output.coin.inner(),
                *value_coords.x(),
                *value_coords.y(),
                *token_coords.x(),
                *token_coords.y(),
            ],
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
    call_idx: u32,
    calls: Vec<ContractCall>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx as usize];
    let params: MoneyTransferParamsV1 = deserialize(&self_.data[1..])?;

    if params.clear_inputs.len() + params.inputs.len() < 1 {
        msg!("[TransferV1] Error: No inputs in the call");
        return Err(MoneyError::TransferMissingInputs.into())
    }

    if params.outputs.is_empty() {
        msg!("[TransferV1] Error: No outputs in the call");
        return Err(MoneyError::TransferMissingOutputs.into())
    }

    // Access the necessary databases where there is information to
    // validate this state transition.
    let info_db = db_lookup(cid, MONEY_CONTRACT_INFO_TREE)?;
    let coins_db = db_lookup(cid, MONEY_CONTRACT_COINS_TREE)?;
    let nullifiers_db = db_lookup(cid, MONEY_CONTRACT_NULLIFIERS_TREE)?;
    let coin_roots_db = db_lookup(cid, MONEY_CONTRACT_COIN_ROOTS_TREE)?;

    // Grab faucet pubkeys. They're allowed to create clear inputs.
    // Currently we use them for airdrops in the testnet.
    let Some(faucet_pubkeys) = db_get(info_db, &serialize(&MONEY_CONTRACT_FAUCET_PUBKEYS))? else {
        msg!("[TransferV1] Error: Missing faucet pubkeys from info db");
        return Err(MoneyError::TransferMissingFaucetKeys.into())
    };
    let faucet_pubkeys: Vec<PublicKey> = deserialize(&faucet_pubkeys)?;

    // Accumulator for the value commitments. We add inputs to it, and subtract
    // outputs from it. For the commitments to be valid, the accumulator must
    // be in its initial state after performing the arithmetics.
    let mut valcom_total = pallas::Point::identity();

    // ===================================
    // Perform the actual state transition
    // ===================================

    // For clear inputs, we only allow the whitelisted faucet(s) to create them.
    // Additionally, only DARK_TOKEN_ID is able to be here. For any arbitrary
    // tokens, there is another functionality in this contract called `Mint` which
    // allows users to mint their own tokens.
    msg!("[TransferV1] Iterating over clear inputs");
    for (i, input) in params.clear_inputs.iter().enumerate() {
        if input.token_id != *DARK_TOKEN_ID {
            msg!("[TransferV1] Error: Clear input {} used non-native token", i);
            return Err(MoneyError::TransferClearInputNonNativeToken.into())
        }

        if !faucet_pubkeys.contains(&input.signature_public) {
            msg!("[TransferV1] Error: Clear input {} used unauthorised pubkey", i);
            return Err(MoneyError::TransferClearInputUnauthorised.into())
        }

        // Add this input to the value commitment accumulator
        valcom_total += pedersen_commitment_u64(input.value, input.value_blind);
    }

    // For anonymous inputs, we must also gather all the new nullifiers
    // that are introduced.
    let mut new_nullifiers = Vec::with_capacity(params.inputs.len());
    msg!("[TransferV1] Iterating over anonymous inputs");
    for (i, input) in params.inputs.iter().enumerate() {
        // The Merkle root is used to know whether this is a coin that
        // existed in a previous state.
        if !db_contains_key(coin_roots_db, &serialize(&input.merkle_root))? {
            msg!("[TransferV1] Error: Merkle root not found in previous state (input {})", i);
            return Err(MoneyError::TransferMerkleRootNotFound.into())
        }

        // The nullifiers should not already exist. It is the double-spend protection.
        if new_nullifiers.contains(&input.nullifier) ||
            db_contains_key(nullifiers_db, &serialize(&input.nullifier))?
        {
            msg!("[TransferV1] Error: Duplicate nullifier found (input {})", i);
            return Err(MoneyError::DuplicateNullifier.into())
        }

        // If spend hook is set, check its correctness
        if input.spend_hook != pallas::Base::ZERO {
            let next_call_idx = call_idx + 1;
            if next_call_idx >= calls.len() as u32 {
                msg!("[TransferV1] Error: next_call_idx out of bounds (input {})", i);
                return Err(MoneyError::CallIdxOutOfBounds.into())
            }

            let next = &calls[next_call_idx as usize];
            if next.contract_id.inner() != input.spend_hook {
                msg!("[TransferV1] Error: Invoking contract call does not match spend hook in input {}", i);
                return Err(MoneyError::SpendHookMismatch.into())
            }
        }

        // Append this new nullifier to seen nullifiers, and accumulate the value commitment
        new_nullifiers.push(input.nullifier);
        valcom_total += input.value_commit;
    }

    // Newly created coins for this call are in the outputs. Here we gather them,
    // and we also check that they haven't existed before.
    let mut new_coins = Vec::with_capacity(params.outputs.len());
    for (i, output) in params.outputs.iter().enumerate() {
        if new_coins.contains(&output.coin) || db_contains_key(coins_db, &serialize(&output.coin))?
        {
            msg!("[TransferV1] Error: Duplicate coin found in output {}", i);
            return Err(MoneyError::DuplicateCoin.into())
        }

        // Append this new coin to seen coins, and subtract the value commitment
        new_coins.push(output.coin);
        valcom_total -= output.value_commit;
    }

    // If the accumulator is not back in its initial state, that means there
    // is a value mismatch between inputs and outputs.
    if valcom_total != pallas::Point::identity() {
        msg!("[TransferV1] Error: Value commitments do not result in identity");
        return Err(MoneyError::ValueMismatch.into())
    }

    // We also need to verify that all token commitments are the same.
    // In the basic transfer, we only allow the same token type to be
    // transferred. For exchanging we use another functionality of this
    // contract called `OtcSwap`.
    let tokcom = params.outputs[0].token_commit;
    let mut failed_tokcom = params.inputs.iter().any(|x| x.token_commit != tokcom);
    failed_tokcom = failed_tokcom || params.outputs.iter().any(|x| x.token_commit != tokcom);
    failed_tokcom = failed_tokcom ||
        params
            .clear_inputs
            .iter()
            .any(|x| pedersen_commitment_base(x.token_id.inner(), x.token_blind) != tokcom);

    if failed_tokcom {
        msg!("[TransferV1] Error: Token commitments do not match");
        return Err(MoneyError::TokenMismatch.into())
    }

    // At this point the state transition has passed, so we create a state update
    let update = MoneyTransferUpdateV1 { nullifiers: new_nullifiers, coins: new_coins };
    let mut update_data = vec![];
    update_data.write_u8(MoneyFunction::TransferV1 as u8)?;
    update.encode(&mut update_data)?;
    // and return it
    Ok(update_data)
}

/// `process_update` function for `Money::TransferV1`
pub(crate) fn money_transfer_process_update_v1(
    cid: ContractId,
    update: MoneyTransferUpdateV1,
) -> ContractResult {
    // Grab all necessary db handles for where we want to write
    let info_db = db_lookup(cid, MONEY_CONTRACT_INFO_TREE)?;
    let coins_db = db_lookup(cid, MONEY_CONTRACT_COINS_TREE)?;
    let nullifiers_db = db_lookup(cid, MONEY_CONTRACT_NULLIFIERS_TREE)?;
    let coin_roots_db = db_lookup(cid, MONEY_CONTRACT_COIN_ROOTS_TREE)?;

    msg!("[TransferV1] Adding new nullifiers to the set");
    for nullifier in update.nullifiers {
        db_set(nullifiers_db, &serialize(&nullifier), &[])?;
    }

    msg!("[TransferV1] Adding new coins to the set");
    for coin in &update.coins {
        db_set(coins_db, &serialize(coin), &[])?;
    }

    msg!("[TransferV1] Adding new coins to the Merkle tree");
    let coins: Vec<_> = update.coins.iter().map(|x| MerkleNode::from(x.inner())).collect();
    merkle_add(info_db, coin_roots_db, &serialize(&MONEY_CONTRACT_COIN_MERKLE_TREE), &coins)?;

    Ok(())
}
