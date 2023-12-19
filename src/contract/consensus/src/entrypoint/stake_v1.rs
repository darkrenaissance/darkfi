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
    error::MoneyError,
    model::{ConsensusStakeParamsV1, ConsensusStakeUpdateV1, MoneyStakeParamsV1},
    MoneyFunction, CONSENSUS_CONTRACT_INFO_TREE, CONSENSUS_CONTRACT_STAKED_COINS_TREE,
    CONSENSUS_CONTRACT_STAKED_COIN_LATEST_COIN_ROOT, CONSENSUS_CONTRACT_STAKED_COIN_MERKLE_TREE,
    CONSENSUS_CONTRACT_STAKED_COIN_ROOTS_TREE, CONSENSUS_CONTRACT_UNSTAKED_COINS_TREE,
    CONSENSUS_CONTRACT_ZKAS_MINT_NS_V1, MONEY_CONTRACT_COIN_ROOTS_TREE,
    MONEY_CONTRACT_NULLIFIERS_TREE,
};
use darkfi_sdk::{
    crypto::{pasta_prelude::*, ContractId, MerkleNode, PublicKey, MONEY_CONTRACT_ID},
    dark_tree::DarkLeaf,
    db::{db_contains_key, db_lookup, db_set},
    error::{ContractError, ContractResult},
    merkle_add, msg,
    pasta::pallas,
    util::get_verifying_slot_epoch,
    ContractCall,
};
use darkfi_serial::{deserialize, serialize, Encodable, WriteExt};

use crate::ConsensusFunction;

/// `get_metadata` function for `Consensus::StakeV1`
pub(crate) fn consensus_stake_get_metadata_v1(
    _cid: ContractId,
    call_idx: u32,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx as usize].data;
    let params: ConsensusStakeParamsV1 = deserialize(&self_.data[1..])?;

    // Public inputs for the ZK proofs we have to verify
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    // We are already verifying this input's signature through `Money::Stake`,
    // so it's redundant to verify it here again. However it's important to
    // compare it with the previous call when we do the state transition to
    // ensure they're the same.
    let signature_pubkeys: Vec<PublicKey> = vec![];

    // Grab the minting epoch of the verifying slot
    let epoch = get_verifying_slot_epoch();

    // Grab the pedersen commitment from the anonymous output
    let output = &params.output;
    let value_coords = output.value_commit.to_affine().coordinates().unwrap();

    zk_public_inputs.push((
        CONSENSUS_CONTRACT_ZKAS_MINT_NS_V1.to_string(),
        vec![epoch.into(), output.coin.inner(), *value_coords.x(), *value_coords.y()],
    ));

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
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx as usize];
    let params: ConsensusStakeParamsV1 = deserialize(&self_.data.data[1..])?;

    // Check child call is money contract
    if call_idx == 0 {
        msg!("[ConsensusStakeV1] Error: child_call_idx will be out of bounds");
        return Err(MoneyError::CallIdxOutOfBounds.into())
    }

    let child_call_indexes = &self_.children_indexes;
    if child_call_indexes.len() != 1 {
        msg!("[ConsensusStakeV1] Error: child_call_idx is missing");
        return Err(MoneyError::StakeChildCallNotMoneyContract.into())
    }
    let child_call_idx = child_call_indexes[0];

    // Verify child call corresponds to Money::StakeV1
    let child = &calls[child_call_idx].data;
    if child.contract_id.inner() != MONEY_CONTRACT_ID.inner() {
        msg!("[ConsensusStakeV1] Error: Child contract call is not money contract");
        return Err(MoneyError::StakeChildCallNotMoneyContract.into())
    }

    if child.data[0] != MoneyFunction::StakeV1 as u8 {
        msg!("[ConsensusStakeV1] Error: Child call function mismatch");
        return Err(MoneyError::ChildCallFunctionMismatch.into())
    }

    // Verify that the child call's input is the same as this one's
    let child_params: MoneyStakeParamsV1 = deserialize(&child.data[1..])?;
    let child_input = &child_params.input;
    if child_input != &params.input {
        msg!("[ConsensusStakeV1] Error: Child call input mismatch");
        return Err(MoneyError::ChildCallInputMismatch.into())
    }

    // Access the necessary databases where there is information to
    // validate this state transition.
    let consensus_coins_db = db_lookup(cid, CONSENSUS_CONTRACT_STAKED_COINS_TREE)?;
    let consensus_unstaked_coins_db = db_lookup(cid, CONSENSUS_CONTRACT_UNSTAKED_COINS_TREE)?;
    let money_nullifiers_db = db_lookup(*MONEY_CONTRACT_ID, MONEY_CONTRACT_NULLIFIERS_TREE)?;
    let money_coin_roots_db = db_lookup(*MONEY_CONTRACT_ID, MONEY_CONTRACT_COIN_ROOTS_TREE)?;

    // ===================================
    // Perform the actual state transition
    // ===================================

    msg!("[ConsensusStakeV1] Validating anonymous output");
    let input = &params.input;
    let output = &params.output;

    // Verify value commitments match
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

    // The nullifiers should not already exist. It is the double-mint protection.
    if !db_contains_key(money_nullifiers_db, &serialize(&input.nullifier))? {
        msg!("[ConsensusStakeV1] Error: Missing nullifier");
        return Err(MoneyError::StakeMissingNullifier.into())
    }

    // Newly created coin for this call is in the output. Here we gather it,
    // and we also check that it hasn't existed before.
    let coin = serialize(&output.coin);
    if db_contains_key(consensus_coins_db, &coin)? {
        msg!("[ConsensusStakeV1] Error: Duplicate coin found in output");
        return Err(MoneyError::DuplicateCoin.into())
    }

    // Check that the coin hasn't existed before in unstake set.
    if db_contains_key(consensus_unstaked_coins_db, &coin)? {
        msg!("[ConsensusStakeV1] Error: Unstaked coin found in output");
        return Err(MoneyError::DuplicateCoin.into())
    }

    // Create a state update.
    let update = ConsensusStakeUpdateV1 { coin: output.coin };
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
    let staked_coins_db = db_lookup(cid, CONSENSUS_CONTRACT_STAKED_COINS_TREE)?;
    let staked_coin_roots_db = db_lookup(cid, CONSENSUS_CONTRACT_STAKED_COIN_ROOTS_TREE)?;

    msg!("[ConsensusStakeV1] Adding new coin to the set");
    db_set(staked_coins_db, &serialize(&update.coin), &[])?;

    msg!("[ConsensusStakeV1] Adding new coin to the Merkle tree");
    let coins: Vec<_> = vec![MerkleNode::from(update.coin.inner())];
    merkle_add(
        info_db,
        staked_coin_roots_db,
        CONSENSUS_CONTRACT_STAKED_COIN_LATEST_COIN_ROOT,
        CONSENSUS_CONTRACT_STAKED_COIN_MERKLE_TREE,
        &coins,
    )?;

    Ok(())
}
