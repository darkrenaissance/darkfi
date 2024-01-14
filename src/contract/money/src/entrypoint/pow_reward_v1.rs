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
    blockchain::{expected_reward, Slot, POW_CUTOFF},
    crypto::{
        pasta_prelude::*, pedersen_commitment_u64, poseidon_hash, ContractId, MerkleNode,
        DARK_TOKEN_ID,
    },
    dark_tree::DarkLeaf,
    db::{db_contains_key, db_lookup, db_set},
    error::{ContractError, ContractResult},
    merkle_add, msg,
    pasta::pallas,
    util::{get_slot, get_verifying_slot},
    ContractCall,
};
use darkfi_serial::{deserialize, serialize, Encodable, WriteExt};

use crate::{
    error::MoneyError,
    model::{MoneyPoWRewardParamsV1, MoneyPoWRewardUpdateV1},
    MoneyFunction, MONEY_CONTRACT_COINS_TREE, MONEY_CONTRACT_COIN_MERKLE_TREE,
    MONEY_CONTRACT_COIN_ROOTS_TREE, MONEY_CONTRACT_INFO_TREE, MONEY_CONTRACT_LATEST_COIN_ROOT,
    MONEY_CONTRACT_ZKAS_MINT_NS_V1,
};

/// `get_metadata` function for `Money::PoWRewardV1`
pub(crate) fn money_pow_reward_get_metadata_v1(
    _cid: ContractId,
    call_idx: u32,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx as usize].data;
    let params: MoneyPoWRewardParamsV1 = deserialize(&self_.data[1..])?;

    // Public inputs for the ZK proofs we have to verify
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    // Public keys for the transaction signatures we have to verify
    let signature_pubkeys = vec![params.input.signature_public];

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

/// `process_instruction` function for `Money::PoWRewardV1`
pub(crate) fn money_pow_reward_process_instruction_v1(
    cid: ContractId,
    call_idx: u32,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx as usize].data;
    let params: MoneyPoWRewardParamsV1 = deserialize(&self_.data[1..])?;

    // Verify this contract call is verified against a slot(block height) before PoS transition,
    // excluding genesis.
    let verifying_slot = get_verifying_slot();
    if verifying_slot == 0 || verifying_slot > POW_CUTOFF {
        msg!(
            "[PoWRewardV1] Error: Call is executed for slot {}(cutoff slot {})",
            verifying_slot,
            POW_CUTOFF
        );
        return Err(MoneyError::PoWRewardCallAfterCutoffSlot.into())
    }

    // Grab the slot to validate consensus params against
    let Some(slot) = get_slot(verifying_slot)? else {
        msg!("[PoWRewardV1] Error: Missing slot {} from db", verifying_slot);
        return Err(MoneyError::PoWRewardMissingSlot.into())
    };
    let slot: Slot = deserialize(&slot)?;

    // Verify proposal extends a known fork
    if !slot.previous.last_hashes.contains(&params.fork_hash) {
        msg!("[PoWRewardV1] Error: Block extends unknown fork {}", params.fork_hash);
        return Err(MoneyError::PoWRewardExtendsUnknownFork.into())
    }

    // Verify sequence is correct
    if !slot.previous.second_to_last_hashes.contains(&params.fork_previous_hash) {
        let fork_prev = &params.fork_previous_hash;
        msg!("[PoWRewardV1] Error: Block extends unknown fork {}", fork_prev);
        return Err(MoneyError::PoWRewardExtendsUnknownFork.into())
    }

    // Construct VRF input
    let mut vrf_input = Vec::with_capacity(32 + blake3::OUT_LEN + 32);
    vrf_input.extend_from_slice(&slot.last_nonce.to_repr());
    vrf_input.extend_from_slice(params.fork_previous_hash.as_bytes());
    vrf_input.extend_from_slice(&pallas::Base::from(slot.id).to_repr());

    // Verify VRF proof
    if !params.vrf_proof.verify(params.input.signature_public, &vrf_input) {
        msg!("[PoWRewardV1] Error: VRF proof couldn't be verified");
        return Err(MoneyError::PoWRewardErroneousVrfProof.into())
    }

    // Only DARK_TOKEN_ID can be minted as PoW reward.
    if params.input.token_id != *DARK_TOKEN_ID {
        msg!("[PoWRewardV1] Error: Clear input used non-native token");
        return Err(MoneyError::TransferClearInputNonNativeToken.into())
    }

    // Verify reward value matches the expected one for this slot(block height)
    let expected_reward = expected_reward(verifying_slot);
    if params.input.value != expected_reward {
        msg!(
            "[PoWRewardV1] Error: Reward value({}) is not the block height({}) expected one: {}",
            params.input.value,
            verifying_slot,
            expected_reward
        );
        return Err(MoneyError::ValueMismatch.into())
    }

    // Access the necessary databases where there is information to
    // validate this state transition.
    let coins_db = db_lookup(cid, MONEY_CONTRACT_COINS_TREE)?;

    // Check that the coin from the output hasn't existed before.
    if db_contains_key(coins_db, &serialize(&params.output.coin))? {
        msg!("[PoWRewardV1] Error: Duplicate coin in output");
        return Err(MoneyError::DuplicateCoin.into())
    }

    // Verify that the value and token commitments match. In here we just
    // confirm that the clear input and the anon output have the same
    // commitments.
    if pedersen_commitment_u64(params.input.value, params.input.value_blind) !=
        params.output.value_commit
    {
        msg!("[PoWRewardV1] Error: Value commitment mismatch");
        return Err(MoneyError::ValueMismatch.into())
    }

    if poseidon_hash([params.input.token_id.inner(), params.input.token_blind]) !=
        params.output.token_commit
    {
        msg!("[PoWRewardV1] Error: Token commitment mismatch");
        return Err(MoneyError::TokenMismatch.into())
    }

    // Create a state update. We only need the new coin.
    let update = MoneyPoWRewardUpdateV1 { coin: params.output.coin };
    let mut update_data = vec![];
    update_data.write_u8(MoneyFunction::PoWRewardV1 as u8)?;
    update.encode(&mut update_data)?;

    Ok(update_data)
}

/// `process_update` function for `Money::PoWRewardV1`
pub(crate) fn money_pow_reward_process_update_v1(
    cid: ContractId,
    update: MoneyPoWRewardUpdateV1,
) -> ContractResult {
    // Grab all db handles we want to work on
    let info_db = db_lookup(cid, MONEY_CONTRACT_INFO_TREE)?;
    let coins_db = db_lookup(cid, MONEY_CONTRACT_COINS_TREE)?;
    let coin_roots_db = db_lookup(cid, MONEY_CONTRACT_COIN_ROOTS_TREE)?;

    msg!("[PoWRewardV1] Adding new coin to the set");
    db_set(coins_db, &serialize(&update.coin), &[])?;

    msg!("[PoWRewardV1] Adding new coin to the Merkle tree");
    let coins = vec![MerkleNode::from(update.coin.inner())];
    merkle_add(
        info_db,
        coin_roots_db,
        MONEY_CONTRACT_LATEST_COIN_ROOT,
        MONEY_CONTRACT_COIN_MERKLE_TREE,
        &coins,
    )?;

    Ok(())
}
