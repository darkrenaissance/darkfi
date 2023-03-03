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

use darkfi_money_contract::{
    error::MoneyError, model::MoneyStakeParamsV1, CONSENSUS_CONTRACT_COINS_TREE,
    CONSENSUS_CONTRACT_COIN_MERKLE_TREE, CONSENSUS_CONTRACT_COIN_ROOTS_TREE,
    CONSENSUS_CONTRACT_INFO_TREE, CONSENSUS_CONTRACT_ZKAS_MINT_NS_V1,
    MONEY_CONTRACT_COIN_ROOTS_TREE, MONEY_CONTRACT_NULLIFIERS_TREE,
};
use darkfi_sdk::{
    crypto::{
        pasta_prelude::*, pedersen_commitment_base, Coin, ContractId, MerkleNode, PublicKey,
        CONSENSUS_CONTRACT_ID, DARK_TOKEN_ID, MONEY_CONTRACT_ID,
    },
    db::{db_contains_key, db_lookup, db_set},
    error::{ContractError, ContractResult},
    merkle_add, msg,
    pasta::pallas,
    ContractCall,
};
use darkfi_serial::{deserialize, serialize, Encodable, WriteExt};

use crate::{
    model::{ConsensusStakeParamsV1, ConsensusStakeUpdateV1},
    ConsensusFunction,
};

/// `get_metadata` function for `Consensus::StakeV1`
pub(crate) fn consensus_stake_get_metadata_v1(
    _cid: ContractId,
    call_idx: u32,
    calls: Vec<ContractCall>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx as usize];
    let params: ConsensusStakeParamsV1 = deserialize(&self_.data[1..])?;

    // Public inputs for the ZK proofs we have to verify
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    // Public keys for the transaction signatures we have to verify
    let mut signature_pubkeys: Vec<PublicKey> = vec![];

    // Grab the pedersen commitment from the anonymous output
    let output = &params.output;
    let value_coords = output.value_commit.to_affine().coordinates().unwrap();
    let token_coords = output.token_commit.to_affine().coordinates().unwrap();

    zk_public_inputs.push((
        CONSENSUS_CONTRACT_ZKAS_MINT_NS_V1.to_string(),
        vec![
            output.coin.inner(),
            *value_coords.x(),
            *value_coords.y(),
            *token_coords.x(),
            *token_coords.y(),
        ],
    ));

    signature_pubkeys.push(params.input.signature_public);

    // Serialize everything gathered and return it
    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    signature_pubkeys.encode(&mut metadata)?;

    Ok(metadata)
}

/// `process_instruction` function for `Consensus::StakeV1`
pub(crate) fn consensus_stake_process_instruction_v1(
    cid: ContractId,
    call_idx: u32,
    calls: Vec<ContractCall>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx as usize];
    let params: ConsensusStakeParamsV1 = deserialize(&self_.data[1..])?;

    // Access the necessary databases where there is information to
    // validate this state transition.
    let consensus_coins_db = db_lookup(cid, CONSENSUS_CONTRACT_COINS_TREE)?;
    let money_nullifiers_db = db_lookup(*MONEY_CONTRACT_ID, MONEY_CONTRACT_NULLIFIERS_TREE)?;
    let money_coin_roots_db = db_lookup(*MONEY_CONTRACT_ID, MONEY_CONTRACT_COIN_ROOTS_TREE)?;

    // ===================================
    // Perform the actual state transition
    // ===================================

    msg!("[ConsensusStakeV1] Validating anonymous output");
    let input = &params.input;
    let output = &params.output;

    // Only native token can be staked
    if output.token_commit != pedersen_commitment_base(DARK_TOKEN_ID.inner(), input.token_blind) {
        msg!("[ConsensusStakeV1] Error: Input used non-native token");
        return Err(MoneyError::StakeInputNonNativeToken.into())
    }

    // Verify value commits match
    if output.value_commit != input.value_commit {
        msg!("[ConsensusStakeV1] Error: Value commitments do not match");
        return Err(MoneyError::ValueMismatch.into())
    }

    // The Merkle root is used to know whether this is a coin that
    // existed in a previous state.
    if !db_contains_key(money_coin_roots_db, &serialize(&input.merkle_root))? {
        msg!("[ConsensusStakeV1] Error: Merkle root not found in previous state");
        return Err(MoneyError::TransferMerkleRootNotFound.into())
    }

    // The nullifiers should already exist. It is the double-mint protection.
    if db_contains_key(money_nullifiers_db, &serialize(&input.nullifier))? {
        msg!("[ConsensusStakeV1] Error: Missing nullifier");
        return Err(MoneyError::StakeMissingNullifier.into())
    }

    // Check previous call is money contract
    if call_idx == 0 {
        msg!("[ConsensusStakeV1] Error: previous_call_idx will be out of bounds");
        return Err(MoneyError::SpendHookOutOfBounds.into())
    }

    let previous_call_idx = call_idx - 1;
    let previous = &calls[previous_call_idx as usize];
    if previous.contract_id.inner() != MONEY_CONTRACT_ID.inner() {
        msg!("[ConsensusStakeV1] Error: Previous contract call is not consensus contract");
        return Err(MoneyError::StakePreviousCallNotMoneyContract.into())
    }

    // Verify previous call corresponds to Money::StakeV1 (0x05)
    if previous.data[0] != 0x05 {
        msg!("[ConsensusStakeV1] Error: Previous call function mismatch");
        return Err(MoneyError::PreviousCallFunctionMissmatch.into())
    }

    // Verify previous call input is the same as this calls StakeInput
    let previous_params: MoneyStakeParamsV1 = deserialize(&previous.data[1..])?;
    let previous_input = &previous_params.input;
    if &previous_input != &input {
        msg!("[ConsensusStakeV1] Error: Previous call input mismatch");
        return Err(MoneyError::PreviousCallInputMissmatch.into())
    }

    // If spend hook is set, check its correctness
    if previous_input.spend_hook != pallas::Base::zero() &&
        previous_input.spend_hook != CONSENSUS_CONTRACT_ID.inner()
    {
        msg!("[ConsensusStakeV1] Error: Invoking contract call does not match spend hook in input");
        return Err(MoneyError::SpendHookMismatch.into())
    }

    // Newly created coin for this call is in the output. Here we gather it,
    // and we also check that it hasn't existed before.
    if db_contains_key(consensus_coins_db, &serialize(&output.coin))? {
        msg!("[ConsensusStakeV1] Error: Duplicate coin found in output");
        return Err(MoneyError::DuplicateCoin.into())
    }
    let coin = Coin::from(output.coin);

    // Create a state update.
    let update = ConsensusStakeUpdateV1 { coin };
    let mut update_data = vec![];
    update_data.write_u8(ConsensusFunction::StakeV1 as u8)?;
    update.encode(&mut update_data)?;

    Ok(update_data)
}

/// `process_update` function for `Consensus::StakeV1`
pub(crate) fn consensus_stake_process_update_v1(
    cid: ContractId,
    update: ConsensusStakeUpdateV1,
) -> ContractResult {
    // Grab all necessary db handles for where we want to write
    let info_db = db_lookup(cid, CONSENSUS_CONTRACT_INFO_TREE)?;
    let coins_db = db_lookup(cid, CONSENSUS_CONTRACT_COINS_TREE)?;
    let coin_roots_db = db_lookup(cid, CONSENSUS_CONTRACT_COIN_ROOTS_TREE)?;

    msg!("[ConsensusStakeV1] Adding new coin to the set");
    db_set(coins_db, &serialize(&update.coin), &[])?;

    msg!("[ConsensusStakeV1] Adding new coin to the Merkle tree");
    let coins: Vec<_> = vec![MerkleNode::from(update.coin.inner())];
    merkle_add(info_db, coin_roots_db, &serialize(&CONSENSUS_CONTRACT_COIN_MERKLE_TREE), &coins)?;

    Ok(())
}
