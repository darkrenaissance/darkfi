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
    crypto::{ContractId, FuncRef, MerkleNode, PublicKey},
    dark_tree::DarkLeaf,
    db::{db_contains_key, db_lookup, db_set},
    error::{ContractError, ContractResult},
    merkle_add, msg,
    pasta::pallas,
    ContractCall,
};
use darkfi_serial::{deserialize, serialize, Encodable, WriteExt};

use crate::{
    error::MoneyError,
    model::{MoneyTokenMintParamsV1, MoneyTokenMintUpdateV1},
    MoneyFunction, MONEY_CONTRACT_COINS_TREE, MONEY_CONTRACT_COIN_MERKLE_TREE,
    MONEY_CONTRACT_COIN_ROOTS_TREE, MONEY_CONTRACT_INFO_TREE, MONEY_CONTRACT_LATEST_COIN_ROOT,
    MONEY_CONTRACT_ZKAS_TOKEN_MINT_NS_V1,
};

/// `get_metadata` function for `Money::TokenMintV1`
pub(crate) fn money_token_mint_get_metadata_v1(
    _cid: ContractId,
    call_idx: u32,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx as usize].data;
    let params: MoneyTokenMintParamsV1 = deserialize(&self_.data[1..])?;

    let parent_idx = calls[call_idx as usize].parent_index.unwrap();
    let parent_call = &calls[parent_idx].data;
    let parent_contract_id = parent_call.contract_id;
    let parent_func_code = parent_call.data[0];

    // Public inputs for the ZK proofs we have to verify
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    // Public keys for the transaction signatures we have to verify.
    let signature_pubkeys: Vec<PublicKey> = vec![];

    let parent_func_id =
        FuncRef { contract_id: parent_contract_id, func_code: parent_func_code }.to_func_id();

    zk_public_inputs.push((
        MONEY_CONTRACT_ZKAS_TOKEN_MINT_NS_V1.to_string(),
        vec![parent_func_id.inner(), params.coin.inner()],
    ));

    // Serialize everything gathered and return it
    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    signature_pubkeys.encode(&mut metadata)?;

    Ok(metadata)
}

/// `process_instruction` function for `Money::TokenMintV1`
pub(crate) fn money_token_mint_process_instruction_v1(
    cid: ContractId,
    call_idx: u32,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx as usize].data;
    let params: MoneyTokenMintParamsV1 = deserialize(&self_.data[1..])?;

    // We have to check if the token mint is frozen, and if by some chance
    // the minted coin has existed already.
    let coins_db = db_lookup(cid, MONEY_CONTRACT_COINS_TREE)?;

    // Check that the coin from the output hasn't existed before
    if db_contains_key(coins_db, &serialize(&params.coin))? {
        msg!("[MintV1] Error: Duplicate coin in output");
        return Err(MoneyError::DuplicateCoin.into())
    }

    // Create a state update. We only need the new coin.
    let update = MoneyTokenMintUpdateV1 { coin: params.coin };
    let mut update_data = vec![];
    update_data.write_u8(MoneyFunction::TokenMintV1 as u8)?;
    update.encode(&mut update_data)?;

    Ok(update_data)
}

/// `process_update` function for `Money::TokenMintV1`
pub(crate) fn money_token_mint_process_update_v1(
    cid: ContractId,
    update: MoneyTokenMintUpdateV1,
) -> ContractResult {
    // Grab all db handles we want to work on
    let info_db = db_lookup(cid, MONEY_CONTRACT_INFO_TREE)?;
    let coins_db = db_lookup(cid, MONEY_CONTRACT_COINS_TREE)?;
    let coin_roots_db = db_lookup(cid, MONEY_CONTRACT_COIN_ROOTS_TREE)?;

    msg!("[MintV1] Adding new coin to the set");
    db_set(coins_db, &serialize(&update.coin), &[])?;

    msg!("[MintV1] Adding new coin to the Merkle tree");
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
