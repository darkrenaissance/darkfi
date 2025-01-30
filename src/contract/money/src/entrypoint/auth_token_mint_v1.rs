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
    crypto::{ContractId, PublicKey},
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg,
    pasta::pallas,
    wasm, ContractCall,
};
use darkfi_serial::{deserialize, serialize, Encodable, WriteExt};

use crate::{
    error::MoneyError,
    model::{MoneyAuthTokenMintParamsV1, MoneyAuthTokenMintUpdateV1},
    MoneyFunction, MONEY_CONTRACT_TOKEN_FREEZE_TREE, MONEY_CONTRACT_ZKAS_AUTH_TOKEN_MINT_NS_V1,
};

/// `get_metadata` function for `Money::AuthTokenMintV1`
pub(crate) fn money_auth_token_mint_get_metadata_v1(
    _cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let params: MoneyAuthTokenMintParamsV1 = deserialize(&calls[call_idx].data.data[1..])?;

    // Public inputs for the ZK proofs we have to verify
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    // Public keys for the transaction signatures we have to verify.
    let signature_pubkeys: Vec<PublicKey> = vec![params.mint_pubkey];

    zk_public_inputs.push((
        MONEY_CONTRACT_ZKAS_AUTH_TOKEN_MINT_NS_V1.to_string(),
        vec![params.mint_pubkey.x(), params.mint_pubkey.y(), params.token_id.inner()],
    ));

    // Serialize everything gathered and return it
    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    signature_pubkeys.encode(&mut metadata)?;

    Ok(metadata)
}

/// `process_instruction` function for `Money::AuthTokenMintV1`
pub(crate) fn money_auth_token_mint_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: MoneyAuthTokenMintParamsV1 = deserialize(&self_.data[1..])?;

    // We have to check if the token mint is frozen.
    let token_freeze_db = wasm::db::db_lookup(cid, MONEY_CONTRACT_TOKEN_FREEZE_TREE)?;

    // Check that the mint is not frozen
    if wasm::db::db_contains_key(token_freeze_db, &serialize(&params.token_id))? {
        msg!("[AuthTokenMintV1] Error: Token mint for {} is frozen", params.token_id);
        return Err(MoneyError::TokenMintFrozen.into())
    }

    // Create a state update.
    let update = MoneyAuthTokenMintUpdateV1 {};
    let mut update_data = vec![];
    update_data.write_u8(MoneyFunction::AuthTokenMintV1 as u8)?;
    update.encode(&mut update_data)?;

    Ok(update_data)
}

/// `process_update` function for `Money::AuthTokenMintV1`
pub(crate) fn money_auth_token_mint_process_update_v1(
    _cid: ContractId,
    _update: MoneyAuthTokenMintUpdateV1,
) -> ContractResult {
    // Do nothing... Coin is added with token_mint() call instead.
    Ok(())
}
