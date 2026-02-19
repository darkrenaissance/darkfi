/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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
    error::{ContractError, ContractResult},
    msg,
    pasta::pallas,
    wasm, ContractCall,
};
use darkfi_serial::{deserialize, serialize, Encodable};

use crate::{
    error::MoneyError,
    model::{MoneyTokenMintParamsV1, MoneyTokenMintUpdateV1},
    MONEY_CONTRACT_COINS_TREE, MONEY_CONTRACT_COIN_MERKLE_TREE, MONEY_CONTRACT_COIN_ROOTS_TREE,
    MONEY_CONTRACT_INFO_TREE, MONEY_CONTRACT_LATEST_COIN_ROOT,
    MONEY_CONTRACT_LATEST_NULLIFIER_ROOT, MONEY_CONTRACT_NULLIFIERS_TREE,
    MONEY_CONTRACT_NULLIFIER_ROOTS_TREE, MONEY_CONTRACT_ZKAS_TOKEN_MINT_NS_V1,
};

/// `get_metadata` function for `Money::TokenMintV1`
pub(crate) fn money_token_mint_get_metadata_v1(
    _cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx];

    // Grab the auth call info
    if self_.children_indexes.len() != 1 {
        msg!(
            "[TokenMintV1] Error: Children indexes length is not expected(1): {}",
            self_.children_indexes.len()
        );
        return Err(MoneyError::ChildrenIndexesLengthMismatch.into())
    }
    let child_idx = self_.children_indexes[0];
    let child_call = &calls[child_idx].data;
    let child_contract_id = child_call.contract_id;
    let child_func_code = child_call.data[0];

    let params: MoneyTokenMintParamsV1 = deserialize(&self_.data.data[1..])?;

    // Public inputs for the ZK proofs we have to verify
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    // Public keys for the transaction signatures we have to verify.
    let signature_pubkeys: Vec<PublicKey> = vec![];

    let child_func_id =
        FuncRef { contract_id: child_contract_id, func_code: child_func_code }.to_func_id();

    zk_public_inputs.push((
        MONEY_CONTRACT_ZKAS_TOKEN_MINT_NS_V1.to_string(),
        vec![child_func_id.inner(), params.coin.inner()],
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
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: MoneyTokenMintParamsV1 = deserialize(&self_.data[1..])?;

    // We have to check if the token mint is frozen, and if by some chance
    // the minted coin has existed already.
    let coins_db = wasm::db::db_lookup(cid, MONEY_CONTRACT_COINS_TREE)?;

    // Check that the coin from the output hasn't existed before
    if wasm::db::db_contains_key(coins_db, &serialize(&params.coin))? ||
        wasm::tx_local::new_coins_contains(&params.coin.inner())?
    {
        msg!("[TokenMintV1] Error: Duplicate coin in output");
        return Err(MoneyError::DuplicateCoin.into())
    }

    // Create a state update. We only need the new coin.
    let coin = if params.intra_tx {
        wasm::tx_local::append_coin(&params.coin.inner())?;
        None
    } else {
        Some(params.coin)
    };

    let update = MoneyTokenMintUpdateV1 { coin };
    Ok(serialize(&update))
}

/// `process_update` function for `Money::TokenMintV1`
pub(crate) fn money_token_mint_process_update_v1(
    cid: ContractId,
    update: MoneyTokenMintUpdateV1,
) -> ContractResult {
    // Grab all db handles we want to work on
    let info_db = wasm::db::db_lookup(cid, MONEY_CONTRACT_INFO_TREE)?;
    let coins_db = wasm::db::db_lookup(cid, MONEY_CONTRACT_COINS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, MONEY_CONTRACT_NULLIFIERS_TREE)?;
    let coin_roots_db = wasm::db::db_lookup(cid, MONEY_CONTRACT_COIN_ROOTS_TREE)?;
    let nullifier_roots_db = wasm::db::db_lookup(cid, MONEY_CONTRACT_NULLIFIER_ROOTS_TREE)?;

    // This will just make a snapshot to match the coins one
    msg!("[TokenMintV1] Updating nullifiers snapshot");
    wasm::merkle::sparse_merkle_insert_batch(
        info_db,
        nullifiers_db,
        nullifier_roots_db,
        MONEY_CONTRACT_LATEST_NULLIFIER_ROOT,
        &[],
    )?;

    if let Some(coin) = update.coin {
        msg!("[TokenMintV1] Adding new coin to the set");
        wasm::db::db_set(coins_db, &serialize(&coin), &[])?;

        msg!("[TokenMintV1] Adding new coin to the Merkle tree");
        let coins = vec![MerkleNode::from(coin.inner())];
        wasm::merkle::merkle_add(
            info_db,
            coin_roots_db,
            MONEY_CONTRACT_LATEST_COIN_ROOT,
            MONEY_CONTRACT_COIN_MERKLE_TREE,
            &coins,
        )?;
    }

    Ok(())
}
