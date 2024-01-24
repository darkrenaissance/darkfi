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

use darkfi_sdk::{
    crypto::{
        pasta_prelude::*, pedersen_commitment_u64, poseidon_hash, ContractId, MerkleNode,
        PublicKey, DARK_TOKEN_ID,
    },
    dark_tree::DarkLeaf,
    db::{db_contains_key, db_get, db_lookup, db_set},
    error::{ContractError, ContractResult},
    merkle_add, msg,
    pasta::pallas,
    ContractCall,
};
use darkfi_serial::{deserialize, serialize, Encodable, WriteExt};

use crate::{
    error::MoneyError,
    model::{MoneyFeeParamsV1, MoneyFeeUpdateV1},
    MoneyFunction, MONEY_CONTRACT_COINS_TREE, MONEY_CONTRACT_COIN_MERKLE_TREE,
    MONEY_CONTRACT_COIN_ROOTS_TREE, MONEY_CONTRACT_INFO_TREE, MONEY_CONTRACT_LATEST_COIN_ROOT,
    MONEY_CONTRACT_NULLIFIERS_TREE, MONEY_CONTRACT_TOTAL_FEES_PAID, MONEY_CONTRACT_ZKAS_FEE_NS_V1,
};

/// `get_metadata` function for `Money::FeeV1`
pub(crate) fn money_fee_get_metadata_v1(
    _cid: ContractId,
    call_idx: u32,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx as usize].data;
    // The first 8 bytes here is the u64 fee, so we get the params from that offset.
    // (Plus 1, which is the function identifier byte)
    let params: MoneyFeeParamsV1 = deserialize(&self_.data[9..])?;

    // Public inputs for the ZK proofs we have to verify
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    // Public keys for the transaction signatures we have to verify
    let signature_pubkeys: Vec<PublicKey> = vec![params.input.signature_public];

    // Grab the Pedersen commitments and the signature pubkey from the params
    let input_value_coords = params.input.value_commit.to_affine().coordinates().unwrap();
    let output_value_coords = params.output.value_commit.to_affine().coordinates().unwrap();
    let (sig_x, sig_y) = params.input.signature_public.xy();

    zk_public_inputs.push((
        MONEY_CONTRACT_ZKAS_FEE_NS_V1.to_string(),
        vec![
            params.input.nullifier.inner(),
            *input_value_coords.x(),
            *input_value_coords.y(),
            params.input.token_commit,
            params.input.merkle_root.inner(),
            params.input.user_data_enc,
            params.input.spend_hook,
            sig_x,
            sig_y,
            params.output.coin.inner(),
            *output_value_coords.x(),
            *output_value_coords.y(),
        ],
    ));

    // Serialize everything gathered and return it
    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    signature_pubkeys.encode(&mut metadata)?;

    Ok(metadata)
}

/// `process_instruction` function for `Money::FeeV1`
pub(crate) fn money_fee_process_instruction_v1(
    cid: ContractId,
    call_idx: u32,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx as usize];
    let fee: u64 = deserialize(&self_.data.data[1..9])?;
    let params: MoneyFeeParamsV1 = deserialize(&self_.data.data[9..])?;

    // We should have _some_ fee paid...
    if fee == 0 {
        msg!("[FeeV1] Error: Paid fee is 0");
        return Err(MoneyError::InsufficientFee.into())
    }

    // Access the necessary databases where there is information to
    // validate this state transition.
    let info_db = db_lookup(cid, MONEY_CONTRACT_INFO_TREE)?;
    let coins_db = db_lookup(cid, MONEY_CONTRACT_COINS_TREE)?;
    let nullifiers_db = db_lookup(cid, MONEY_CONTRACT_NULLIFIERS_TREE)?;
    let coin_roots_db = db_lookup(cid, MONEY_CONTRACT_COIN_ROOTS_TREE)?;

    // Fees can only be paid using the native token, so we'll compare
    // the token commitments with this one:
    let native_token_commit = poseidon_hash([DARK_TOKEN_ID.inner(), params.token_blind]);

    // ===================================
    // Perform the actual state transition
    // ===================================
    if params.input.token_commit != native_token_commit {
        msg!("[FeeV1] Error: Input token commitment is not the native token");
        return Err(MoneyError::TokenMismatch.into())
    }

    // Verify that the token commitment matches
    if params.output.token_commit != native_token_commit {
        msg!("[FeeV1] Error: Output token commitment is not native token");
        return Err(MoneyError::TokenMismatch.into())
    }

    // The spend hook must be zero.
    if params.input.spend_hook != pallas::Base::ZERO {
        msg!("[FeeV1] Error: Input spend hook is nonzero");
        return Err(MoneyError::SpendHookNonZero.into())
    }

    // The Merkle root is used to know whether this is a coin that
    // existed in a previous state.
    if !db_contains_key(coin_roots_db, &serialize(&params.input.merkle_root))? {
        msg!("[FeeV1] Error: Input Merkle root not found in previous state");
        return Err(MoneyError::CoinMerkleRootNotFound.into())
    }

    // The nullifiers should not already exist. It is the double-spend protection.
    if db_contains_key(nullifiers_db, &serialize(&params.input.nullifier))? {
        msg!("[FeeV1] Error: Duplicate nullifier found");
        return Err(MoneyError::DuplicateNullifier.into())
    }

    // The new coin should not exist
    if db_contains_key(coins_db, &serialize(&params.output.coin))? {
        msg!("[FeeV1] Error: Duplicate coin found");
        return Err(MoneyError::DuplicateCoin.into())
    }

    // Accumulator for the value commitments. We add inputs to it, and
    // subtract the outputs and the fee from it. For the commitments to
    // be valid, the accumulatior must be in its initial state after
    // performing the arithmetics.
    let mut valcom_total = pallas::Point::identity();

    // Accumulate the input value commitment.
    valcom_total += params.input.value_commit;

    // Subtract the output value commitment
    valcom_total -= params.output.value_commit;

    // Now subtract the fee from the accumulator
    valcom_total -= pedersen_commitment_u64(fee, params.fee_value_blind);

    // If the accumulator is not back in its initial; state, that means there
    // is a value mismatch betweeen inputs and outputs.
    if valcom_total != pallas::Point::identity() {
        msg!("[FeeV1] Error: Value commitments do not result in identity");
        return Err(MoneyError::ValueMismatch.into())
    }

    // Accumulate the paid fee
    let mut paid_fee: u64 =
        deserialize(&db_get(info_db, MONEY_CONTRACT_TOTAL_FEES_PAID)?.unwrap())?;
    paid_fee += fee;

    // At this point the state transition has passed, so we create a state update.
    let update = MoneyFeeUpdateV1 {
        nullifier: params.input.nullifier,
        coin: params.output.coin,
        fee: paid_fee,
    };
    let mut update_data = vec![];
    update_data.write_u8(MoneyFunction::FeeV1 as u8)?;
    update.encode(&mut update_data)?;
    // and return it
    Ok(update_data)
}

/// `process_update` function for `Money::FeeV1`
pub(crate) fn money_fee_process_update_v1(
    cid: ContractId,
    update: MoneyFeeUpdateV1,
) -> ContractResult {
    // Grab all necessary db handles for where we want to write
    let info_db = db_lookup(cid, MONEY_CONTRACT_INFO_TREE)?;
    let coins_db = db_lookup(cid, MONEY_CONTRACT_COINS_TREE)?;
    let nullifiers_db = db_lookup(cid, MONEY_CONTRACT_NULLIFIERS_TREE)?;
    let coin_roots_db = db_lookup(cid, MONEY_CONTRACT_COIN_ROOTS_TREE)?;

    db_set(info_db, MONEY_CONTRACT_TOTAL_FEES_PAID, &serialize(&update.fee))?;
    db_set(nullifiers_db, &serialize(&update.nullifier), &[])?;
    db_set(coins_db, &serialize(&update.coin), &[])?;

    merkle_add(
        info_db,
        coin_roots_db,
        MONEY_CONTRACT_LATEST_COIN_ROOT,
        MONEY_CONTRACT_COIN_MERKLE_TREE,
        &[MerkleNode::from(update.coin.inner())],
    )?;

    Ok(())
}
