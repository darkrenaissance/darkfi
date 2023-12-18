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
    error::MoneyError, CONSENSUS_CONTRACT_INFO_TREE, CONSENSUS_CONTRACT_STAKED_COINS_TREE,
    CONSENSUS_CONTRACT_STAKED_COIN_LATEST_COIN_ROOT, CONSENSUS_CONTRACT_STAKED_COIN_MERKLE_TREE,
    CONSENSUS_CONTRACT_STAKED_COIN_ROOTS_TREE, CONSENSUS_CONTRACT_UNSTAKED_COINS_TREE,
    CONSENSUS_CONTRACT_ZKAS_MINT_NS_V1,
};
use darkfi_sdk::{
    crypto::{pasta_prelude::*, pedersen_commitment_u64, ContractId, MerkleNode, DARK_TOKEN_ID},
    dark_tree::DarkLeaf,
    db::{db_contains_key, db_lookup, db_set},
    error::{ContractError, ContractResult},
    merkle_add, msg,
    pasta::pallas,
    util::get_verifying_slot,
    ContractCall,
};
use darkfi_serial::{deserialize, serialize, Encodable, WriteExt};

use crate::{
    model::{ConsensusGenesisStakeParamsV1, ConsensusGenesisStakeUpdateV1},
    ConsensusFunction,
};

/// `get_metadata` function for `Consensus::GenesisStakeV1`
///
/// Here we gather the signature pubkey from the clear input in order
/// to verify the transaction, and we extract the necessary public inputs
/// that go into the `ConsensusMint_V1` proof verification.
pub(crate) fn consensus_genesis_stake_get_metadata_v1(
    _cid: ContractId,
    call_idx: u32,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx as usize].data;
    let params: ConsensusGenesisStakeParamsV1 = deserialize(&self_.data[1..])?;

    // Public inputs for the ZK proofs we have to verify
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    // Public keys for the transaction signatures we have to verify
    let signature_pubkeys = vec![params.input.signature_public];

    // Genesis stake only happens on epoch 0
    let epoch = pallas::Base::ZERO;

    // Grab the pedersen commitment from the anonymous output
    let value_coords = params.output.value_commit.to_affine().coordinates().unwrap();

    zk_public_inputs.push((
        CONSENSUS_CONTRACT_ZKAS_MINT_NS_V1.to_string(),
        vec![epoch, params.output.coin.inner(), *value_coords.x(), *value_coords.y()],
    ));

    // Serialize everything gathered and return it
    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    signature_pubkeys.encode(&mut metadata)?;

    Ok(metadata)
}

/// `process_instruction` function for `Consensus::GenesisStakeV1`
pub(crate) fn consensus_genesis_stake_process_instruction_v1(
    cid: ContractId,
    call_idx: u32,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx as usize].data;
    let params: ConsensusGenesisStakeParamsV1 = deserialize(&self_.data[1..])?;

    // Verify this contract call is verified on the genesis slot (0).
    let verifying_slot = get_verifying_slot();
    if verifying_slot != 0 {
        msg!(
            "[ConsensusGenesisStakeV1] Error: Call is executed for slot {}, not genesis",
            verifying_slot
        );
        return Err(MoneyError::GenesisCallNonGenesisSlot.into())
    }

    // Only DARK_TOKEN_ID can be minted and staked on genesis slot.
    if params.input.token_id != *DARK_TOKEN_ID {
        msg!("[ConsensusGenesisStakeV1] Error: Clear input used non-native token");
        return Err(MoneyError::TransferClearInputNonNativeToken.into())
    }

    // Access the necessary databases where there is information to
    // validate this state transition.
    let staked_coins_db = db_lookup(cid, CONSENSUS_CONTRACT_STAKED_COINS_TREE)?;
    let unstaked_coins_db = db_lookup(cid, CONSENSUS_CONTRACT_UNSTAKED_COINS_TREE)?;

    // Check that the coin from the output hasn't existed before.
    let coin = serialize(&params.output.coin);
    if db_contains_key(staked_coins_db, &coin)? {
        msg!("[ConsensusGenesisStakeV1] Error: Output coin was already seen in the set of staked coins");
        return Err(MoneyError::DuplicateCoin.into())
    }

    // Check that the coin from the output hasn't existed before in unstake set.
    if db_contains_key(unstaked_coins_db, &coin)? {
        msg!("[ConsensusGenesisStakeV1] Error: Output coin was already seen in the set of unstaked coins");
        return Err(MoneyError::DuplicateCoin.into())
    }

    // Verify that the value commitments match. In here we just confirm
    // that the clear input and the anon output have the same commitment.
    if pedersen_commitment_u64(params.input.value, params.input.value_blind) !=
        params.output.value_commit
    {
        msg!("[ConsensusGenesisStakeV1] Error: Value commitment mismatch");
        return Err(MoneyError::ValueMismatch.into())
    }

    // Create a state update.
    let update = ConsensusGenesisStakeUpdateV1 { coin: params.output.coin };
    let mut update_data = vec![];
    update_data.write_u8(ConsensusFunction::StakeV1 as u8)?;
    update.encode(&mut update_data)?;

    Ok(update_data)
}

/// `process_update` function for `Consensus::GenesisStakeV1`
pub(crate) fn consensus_genesis_stake_process_update_v1(
    cid: ContractId,
    update: ConsensusGenesisStakeUpdateV1,
) -> ContractResult {
    // Grab all necessary db handles for where we want to write
    let info_db = db_lookup(cid, CONSENSUS_CONTRACT_INFO_TREE)?;
    let staked_coins_db = db_lookup(cid, CONSENSUS_CONTRACT_STAKED_COINS_TREE)?;
    let staked_coin_roots_db = db_lookup(cid, CONSENSUS_CONTRACT_STAKED_COIN_ROOTS_TREE)?;

    msg!("[ConsensusGenesisStakeV1] Adding new coin to the set");
    db_set(staked_coins_db, &serialize(&update.coin), &[])?;

    msg!("[ConsensusGenesisStakeV1] Adding new coin to the Merkle tree");
    let coins: Vec<_> = vec![MerkleNode::from(update.coin.inner())];
    merkle_add(
        info_db,
        staked_coin_roots_db,
        &serialize(&CONSENSUS_CONTRACT_STAKED_COIN_LATEST_COIN_ROOT),
        &serialize(&CONSENSUS_CONTRACT_STAKED_COIN_MERKLE_TREE),
        &coins,
    )?;

    Ok(())
}
