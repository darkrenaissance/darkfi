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
        pasta_prelude::*, poseidon_hash, ContractId, MerkleNode, PublicKey, CONSENSUS_CONTRACT_ID,
        DARK_TOKEN_ID,
    },
    db::{db_contains_key, db_lookup, db_set},
    error::{ContractError, ContractResult},
    merkle_add, msg,
    pasta::pallas,
    ContractCall,
};
use darkfi_serial::{deserialize, serialize, Encodable, WriteExt};

use crate::{
    error::MoneyError,
    model::{ConsensusUnstakeParamsV1, MoneyUnstakeParamsV1, MoneyUnstakeUpdateV1},
    MoneyFunction, CONSENSUS_CONTRACT_NULLIFIERS_TREE, CONSENSUS_CONTRACT_UNSTAKED_COIN_ROOTS_TREE,
    MONEY_CONTRACT_COINS_TREE, MONEY_CONTRACT_COIN_MERKLE_TREE, MONEY_CONTRACT_COIN_ROOTS_TREE,
    MONEY_CONTRACT_INFO_TREE, MONEY_CONTRACT_LATEST_COIN_ROOT, MONEY_CONTRACT_ZKAS_MINT_NS_V1,
};

/// `get_metadata` function for `Money::UnstakeV1`
pub(crate) fn money_unstake_get_metadata_v1(
    _cid: ContractId,
    call_idx: u32,
    calls: Vec<ContractCall>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx as usize];
    let params: MoneyUnstakeParamsV1 = deserialize(&self_.data[1..])?;

    // Public inputs for the ZK proofs we have to verify
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    // We don't have to verify any signatures here, since they're already
    // in the previous contract call (Consensus::UnstakeV1)
    let signature_pubkeys: Vec<PublicKey> = vec![];

    // Grab the pedersen commitment from the anonymous output
    let value_coords = params.output.value_commit.to_affine().coordinates().unwrap();

    zk_public_inputs.push((
        MONEY_CONTRACT_ZKAS_MINT_NS_V1.to_string(),
        vec![
            params.output.coin.inner(),
            *value_coords.x(),
            *value_coords.y(),
            params.output.token_commit,
        ],
    ));

    // Serialize everything gathered and return it
    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    signature_pubkeys.encode(&mut metadata)?;
    Ok(metadata)
}

/// `process_instruction` function for `Money::UnstakeV1`
pub(crate) fn money_unstake_process_instruction_v1(
    cid: ContractId,
    call_idx: u32,
    calls: Vec<ContractCall>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx as usize];
    let params: MoneyUnstakeParamsV1 = deserialize(&self_.data[1..])?;
    let input = &params.input;
    let output = &params.output;

    // Access the necessary databases where there is information to
    // validate this state transition.
    let money_coins_db = db_lookup(cid, MONEY_CONTRACT_COINS_TREE)?;
    let consensus_nullifiers_db =
        db_lookup(*CONSENSUS_CONTRACT_ID, CONSENSUS_CONTRACT_NULLIFIERS_TREE)?;
    let consensus_unstaked_coin_roots_db =
        db_lookup(*CONSENSUS_CONTRACT_ID, CONSENSUS_CONTRACT_UNSTAKED_COIN_ROOTS_TREE)?;

    // ===================================
    // Perform the actual state transition
    // ===================================

    // Check previous call is consensus contract
    if call_idx == 0 {
        msg!("[MoneyUnstakeV1] Error: previous_call_idx will be out of bounds");
        return Err(MoneyError::CallIdxOutOfBounds.into())
    }

    let previous_call_idx = call_idx - 1;
    let previous = &calls[previous_call_idx as usize];
    if previous.contract_id.inner() != CONSENSUS_CONTRACT_ID.inner() {
        msg!("[MoneyUnstakeV1] Error: Previous contract call is not consensus contract");
        return Err(MoneyError::UnstakePreviousCallNotConsensusContract.into())
    }

    // Verify previous call corresponds to Consensus::UnstakeV1 (0x04)
    if previous.data[0] != 0x04 {
        msg!("[MoneyUnstakeV1] Error: Previous call function mismatch");
        return Err(MoneyError::PreviousCallFunctionMismatch.into())
    }

    // Verify previous call input is the same as this calls StakeInput
    let previous_params: ConsensusUnstakeParamsV1 = deserialize(&previous.data[1..])?;
    let previous_input = &previous_params.input;
    if previous_input != input {
        msg!("[MoneyUnstakeV1] Error: Previous call input mismatch");
        return Err(MoneyError::PreviousCallInputMismatch.into())
    }

    msg!("[MoneyUnstakeV1] Validating anonymous output");
    // Only native token can be minted here.
    // Since consensus coins don't have token commitments, we use zero as
    // the token blind for the token commitment of the newly minted token
    if output.token_commit != poseidon_hash([DARK_TOKEN_ID.inner(), pallas::Base::ZERO]) {
        msg!("[MoneyUnstakeV1] Error: Input used non-native token");
        return Err(MoneyError::StakeInputNonNativeToken.into())
    }

    // Verify value commits match
    if output.value_commit != input.value_commit {
        msg!("[MoneyUnstakeV1] Error: Value commitments do not match");
        return Err(MoneyError::ValueMismatch.into())
    }

    // The Merkle root is used to know whether this is a coin that
    // existed in a previous state.
    if !db_contains_key(consensus_unstaked_coin_roots_db, &serialize(&input.merkle_root))? {
        msg!("[MoneyUnstakeV1] Error: Merkle root not found in previous state");
        return Err(MoneyError::TransferMerkleRootNotFound.into())
    }

    // The nullifiers should already exist in the Consensus nullifier set
    if !db_contains_key(consensus_nullifiers_db, &serialize(&input.nullifier))? {
        msg!("[MoneyUnstakeV1] Error: Nullifier not found in Consensus nullifier set");
        return Err(MoneyError::MissingNullifier.into())
    }

    // Newly created coin for this call is in the output. Here we gather it,
    // and we also check that it hasn't existed before.
    if db_contains_key(money_coins_db, &serialize(&output.coin))? {
        msg!("[MoneyUnstakeV1] Error: Duplicate coin found in output");
        return Err(MoneyError::DuplicateCoin.into())
    }

    // Create a state update.
    let update = MoneyUnstakeUpdateV1 { coin: output.coin };
    let mut update_data = vec![];
    update_data.write_u8(MoneyFunction::UnstakeV1 as u8)?;
    update.encode(&mut update_data)?;
    Ok(update_data)
}

/// `process_update` function for `Money::UnstakeV1`
pub(crate) fn money_unstake_process_update_v1(
    cid: ContractId,
    update: MoneyUnstakeUpdateV1,
) -> ContractResult {
    // Grab all necessary db handles for where we want to write
    let info_db = db_lookup(cid, MONEY_CONTRACT_INFO_TREE)?;
    let coins_db = db_lookup(cid, MONEY_CONTRACT_COINS_TREE)?;
    let coin_roots_db = db_lookup(cid, MONEY_CONTRACT_COIN_ROOTS_TREE)?;

    msg!("[MoneyUnstakeV1] Adding new coin to the set");
    db_set(coins_db, &serialize(&update.coin), &[])?;

    msg!("[MoneyUnstakeV1] Adding new coin to the Merkle tree");
    let coins: Vec<_> = vec![MerkleNode::from(update.coin.inner())];
    merkle_add(
        info_db,
        coin_roots_db,
        &serialize(&MONEY_CONTRACT_LATEST_COIN_ROOT),
        &serialize(&MONEY_CONTRACT_COIN_MERKLE_TREE),
        &coins,
    )?;

    Ok(())
}
