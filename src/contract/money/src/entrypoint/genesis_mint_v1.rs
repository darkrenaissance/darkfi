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
    crypto::{pasta_prelude::*, pedersen_commitment_u64, poseidon_hash, ContractId, MerkleNode},
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg,
    pasta::pallas,
    wasm, ContractCall,
};
use darkfi_serial::{deserialize, serialize, Encodable};

use crate::{
    error::MoneyError,
    model::{MoneyGenesisMintParamsV1, MoneyGenesisMintUpdateV1, DARK_TOKEN_ID},
    MONEY_CONTRACT_COINS_TREE, MONEY_CONTRACT_COIN_MERKLE_TREE, MONEY_CONTRACT_COIN_ROOTS_TREE,
    MONEY_CONTRACT_INFO_TREE, MONEY_CONTRACT_LATEST_COIN_ROOT,
    MONEY_CONTRACT_LATEST_NULLIFIER_ROOT, MONEY_CONTRACT_NULLIFIERS_TREE,
    MONEY_CONTRACT_NULLIFIER_ROOTS_TREE, MONEY_CONTRACT_ZKAS_MINT_NS_V1,
};

/// `get_metadata` function for `Money::GenesisMintV1`
pub(crate) fn money_genesis_mint_get_metadata_v1(
    _cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: MoneyGenesisMintParamsV1 = deserialize(&self_.data[1..])?;

    // Public inputs for the ZK proofs we have to verify
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    // Public keys for the transaction signatures we have to verify
    let signature_pubkeys = vec![params.input.signature_public];

    // Grab the pedersen commitments from the anonymous outputs
    for output in &params.outputs {
        let value_coords = output.value_commit.to_affine().coordinates().unwrap();

        zk_public_inputs.push((
            MONEY_CONTRACT_ZKAS_MINT_NS_V1.to_string(),
            vec![output.coin.inner(), *value_coords.x(), *value_coords.y(), output.token_commit],
        ));
    }

    // Serialize everything gathered and return it
    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    signature_pubkeys.encode(&mut metadata)?;

    Ok(metadata)
}

/// `process_instruction` function for `Money::GenesisMintV1`
pub(crate) fn money_genesis_mint_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: MoneyGenesisMintParamsV1 = deserialize(&self_.data[1..])?;

    // Verify this contract call is verified against genesis block(0).
    let verifying_block_height = wasm::util::get_verifying_block_height()?;
    if verifying_block_height != 0 {
        msg!(
            "[GenesisMintV1] Error: Call is executed for block {}, not genesis",
            verifying_block_height
        );
        return Err(MoneyError::GenesisCallNonGenesisBlock.into())
    }

    // Only DARK_TOKEN_ID can be minted on genesis block
    if params.input.token_id != *DARK_TOKEN_ID {
        msg!("[GenesisMintV1] Error: Clear input used non-native token");
        return Err(MoneyError::TransferClearInputNonNativeToken.into())
    }

    // Check outputs exist
    if params.outputs.is_empty() {
        msg!("[GenesisMintV1] Error: No outputs in the call");
        return Err(MoneyError::TransferMissingOutputs.into())
    }

    // Access the necessary databases where there is information to
    // validate this state transition.
    let coins_db = wasm::db::db_lookup(cid, MONEY_CONTRACT_COINS_TREE)?;

    // Compute the expected token commitment of the outputs
    let tokcom = poseidon_hash([params.input.token_id.inner(), params.input.token_blind.inner()]);

    // Accumulator for the outputs value commitments. For the commitments to
    // be valid, the accumulator must reach the input value commitment.
    let mut valcom_total = pallas::Point::identity();

    // Newly created coins for this call are in the outputs. Here we gather them,
    // check that they haven't existed before and their token commitment is valid.
    let mut new_coins = Vec::with_capacity(params.outputs.len());
    msg!("[GenesisMintV1] Iterating over anonymous outputs");
    for (i, output) in params.outputs.iter().enumerate() {
        // Check that the coin has not existed before
        if new_coins.contains(&output.coin) ||
            wasm::db::db_contains_key(coins_db, &serialize(&output.coin))?
        {
            msg!("[GenesisMintV1] Error: Duplicate coin found in output {}", i);
            return Err(MoneyError::DuplicateCoin.into())
        }

        // Verify the token commitment is the expected one
        if tokcom != output.token_commit {
            msg!("[GenesisMintV1] Error: Token commitment mismatch in output {}", i);
            return Err(MoneyError::TokenMismatch.into())
        }

        // Append this new coin to seen coins, and accumulate the value commitment
        new_coins.push(output.coin);
        valcom_total += output.value_commit;
    }

    // If the accumulator doesn't result in the input value commitment, there
    // is a value mismatch between input and outputs.
    if valcom_total != pedersen_commitment_u64(params.input.value, params.input.value_blind) {
        msg!("[GenesisMintV1] Error: Output value commitments do not result in input value commitment");
        return Err(MoneyError::ValueMismatch.into())
    }

    // Create a state update. We only need the new coins.
    let update = MoneyGenesisMintUpdateV1 { coins: new_coins };
    let mut update_data = vec![];
    update.encode(&mut update_data)?;

    Ok(update_data)
}

/// `process_update` function for `Money::GenesisMintV1`
pub(crate) fn money_genesis_mint_process_update_v1(
    cid: ContractId,
    update: MoneyGenesisMintUpdateV1,
) -> ContractResult {
    // Grab all db handles we want to work on
    let info_db = wasm::db::db_lookup(cid, MONEY_CONTRACT_INFO_TREE)?;
    let coins_db = wasm::db::db_lookup(cid, MONEY_CONTRACT_COINS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, MONEY_CONTRACT_NULLIFIERS_TREE)?;
    let coin_roots_db = wasm::db::db_lookup(cid, MONEY_CONTRACT_COIN_ROOTS_TREE)?;
    let nullifier_roots_db = wasm::db::db_lookup(cid, MONEY_CONTRACT_NULLIFIER_ROOTS_TREE)?;

    // This will just make a snapshot to match the coins one
    msg!("[GenesisMintV1] Updating nullifiers snapshot");
    wasm::merkle::sparse_merkle_insert_batch(
        info_db,
        nullifiers_db,
        nullifier_roots_db,
        MONEY_CONTRACT_LATEST_NULLIFIER_ROOT,
        &[],
    )?;

    msg!("[GenesisMintV1] Adding new coins to the set");
    let mut new_coins = Vec::with_capacity(update.coins.len());
    for coin in &update.coins {
        wasm::db::db_set(coins_db, &serialize(coin), &[])?;
        new_coins.push(MerkleNode::from(coin.inner()));
    }

    msg!("[GenesisMintV1] Adding new coins to the Merkle tree");
    wasm::merkle::merkle_add(
        info_db,
        coin_roots_db,
        MONEY_CONTRACT_LATEST_COIN_ROOT,
        MONEY_CONTRACT_COIN_MERKLE_TREE,
        &new_coins,
    )?;

    Ok(())
}
