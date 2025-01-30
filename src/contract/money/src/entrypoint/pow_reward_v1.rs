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
    blockchain::expected_reward,
    crypto::{pasta_prelude::*, pedersen_commitment_u64, poseidon_hash, ContractId, MerkleNode},
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg,
    pasta::pallas,
    wasm, ContractCall,
};
use darkfi_serial::{deserialize, serialize, Encodable, WriteExt};

use crate::{
    error::MoneyError,
    model::{MoneyPoWRewardParamsV1, MoneyPoWRewardUpdateV1, DARK_TOKEN_ID},
    MoneyFunction, MONEY_CONTRACT_COINS_TREE, MONEY_CONTRACT_COIN_MERKLE_TREE,
    MONEY_CONTRACT_COIN_ROOTS_TREE, MONEY_CONTRACT_FEES_TREE, MONEY_CONTRACT_INFO_TREE,
    MONEY_CONTRACT_LATEST_COIN_ROOT, MONEY_CONTRACT_LATEST_NULLIFIER_ROOT,
    MONEY_CONTRACT_NULLIFIERS_TREE, MONEY_CONTRACT_NULLIFIER_ROOTS_TREE,
    MONEY_CONTRACT_ZKAS_MINT_NS_V1,
};

/// `get_metadata` function for `Money::PoWRewardV1`
pub(crate) fn money_pow_reward_get_metadata_v1(
    _cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
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
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: MoneyPoWRewardParamsV1 = deserialize(&self_.data[1..])?;

    // Verify this contract call is not verified against genesis block
    let verifying_block_height = wasm::util::get_verifying_block_height()?;
    if verifying_block_height == 0 {
        msg!("[PoWRewardV1] Error: Call is executed for genesis block");
        return Err(MoneyError::PoWRewardCallOnGenesisBlock.into())
    }

    // Verify this contract call is verified against next block height
    let Some(last_block_height) = wasm::util::get_last_block_height()? else {
        msg!("[PoWRewardV1] Error: Could not receive last block height from db");
        return Err(MoneyError::PoWRewardRetrieveLastBlockHeightError.into())
    };
    let last_block_height: u32 = deserialize(&last_block_height)?;
    if verifying_block_height != last_block_height + 1 {
        msg!(
            "[PoWRewardV1] Error: Call is executed for block height {}, not next one: {}",
            verifying_block_height,
            last_block_height
        );
        return Err(MoneyError::PoWRewardCallNotOnNextBlockHeight.into())
    }

    // Only DARK_TOKEN_ID can be minted as PoW reward.
    if params.input.token_id != *DARK_TOKEN_ID {
        msg!("[PoWRewardV1] Error: Clear input used non-native token");
        return Err(MoneyError::TransferClearInputNonNativeToken.into())
    }

    // Grab the currect height accumulated fees
    let fees_db = wasm::db::db_lookup(cid, MONEY_CONTRACT_FEES_TREE)?;
    let paid_fee: u64 =
        deserialize(&wasm::db::db_get(fees_db, &serialize(&verifying_block_height))?.unwrap())?;

    // Verify reward value matches the expected one for this block height,
    // including the paid fees.
    let expected_reward = expected_reward(verifying_block_height) + paid_fee;
    if params.input.value != expected_reward {
        msg!(
            "[PoWRewardV1] Error: Reward value({}) is not the expected one: {}",
            params.input.value,
            expected_reward
        );
        return Err(MoneyError::ValueMismatch.into())
    }

    // Access the necessary databases where there is information to
    // validate this state transition.
    let coins_db = wasm::db::db_lookup(cid, MONEY_CONTRACT_COINS_TREE)?;

    // Check that the coin from the output hasn't existed before.
    if wasm::db::db_contains_key(coins_db, &serialize(&params.output.coin))? {
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

    if poseidon_hash([params.input.token_id.inner(), params.input.token_blind.inner()]) !=
        params.output.token_commit
    {
        msg!("[PoWRewardV1] Error: Token commitment mismatch");
        return Err(MoneyError::TokenMismatch.into())
    }

    // Create a state update. We only need the new coin.
    let update =
        MoneyPoWRewardUpdateV1 { coin: params.output.coin, height: verifying_block_height };
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
    let info_db = wasm::db::db_lookup(cid, MONEY_CONTRACT_INFO_TREE)?;
    let coins_db = wasm::db::db_lookup(cid, MONEY_CONTRACT_COINS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, MONEY_CONTRACT_NULLIFIERS_TREE)?;
    let coin_roots_db = wasm::db::db_lookup(cid, MONEY_CONTRACT_COIN_ROOTS_TREE)?;
    let nullifier_roots_db = wasm::db::db_lookup(cid, MONEY_CONTRACT_NULLIFIER_ROOTS_TREE)?;
    let fees_db = wasm::db::db_lookup(cid, MONEY_CONTRACT_FEES_TREE)?;

    // Generate the accumulator for the next height
    msg!("[PowRewardV1] Creating next height fees acummulator");
    wasm::db::db_set(fees_db, &serialize(&(update.height + 1)), &serialize(&0_u64))?;

    // This will just make a snapshot to match the coins one
    msg!("[PowRewardV1] Updating nullifiers snapshot");
    wasm::merkle::sparse_merkle_insert_batch(
        info_db,
        nullifiers_db,
        nullifier_roots_db,
        MONEY_CONTRACT_LATEST_NULLIFIER_ROOT,
        &[],
    )?;

    msg!("[PoWRewardV1] Adding new coin to the set");
    wasm::db::db_set(coins_db, &serialize(&update.coin), &[])?;

    msg!("[PoWRewardV1] Adding new coin to the Merkle tree");
    let coins = vec![MerkleNode::from(update.coin.inner())];
    wasm::merkle::merkle_add(
        info_db,
        coin_roots_db,
        MONEY_CONTRACT_LATEST_COIN_ROOT,
        MONEY_CONTRACT_COIN_MERKLE_TREE,
        &coins,
    )?;

    Ok(())
}
