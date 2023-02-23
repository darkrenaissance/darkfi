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
    crypto::{poseidon_hash, ContractId, PublicKey, TokenId},
    db::{db_contains_key, db_lookup, db_set},
    error::{ContractError, ContractResult},
    msg,
    pasta::pallas,
    ContractCall,
};
use darkfi_serial::{deserialize, serialize, Encodable, WriteExt};

use crate::{
    error::MoneyError,
    model::{MoneyFreezeParamsV1, MoneyFreezeUpdateV1},
    MoneyFunction, MONEY_CONTRACT_TOKEN_FREEZE_TREE, MONEY_CONTRACT_ZKAS_TOKEN_FRZ_NS_V1,
};

/// `get_metadata` function for `Money::FreezeV1`
pub(crate) fn money_freeze_get_metadata_v1(
    _cid: ContractId,
    call_idx: u32,
    calls: Vec<ContractCall>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx as usize];
    let params: MoneyFreezeParamsV1 = deserialize(&self_.data[1..])?;

    // Public inputs for the ZK proofs we have to verify
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    // Public keys for the transaction signatures we have to verify
    let signature_pubkeys: Vec<PublicKey> = vec![params.signature_public];

    let (mint_x, mint_y) = params.signature_public.xy();
    let token_id = poseidon_hash([mint_x, mint_y]);

    // In ZK we just verify that the token ID is properly derived from the authority.
    zk_public_inputs.push((MONEY_CONTRACT_ZKAS_TOKEN_FRZ_NS_V1.to_string(), vec![token_id]));

    // Serialize everything gathered and return it
    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    signature_pubkeys.encode(&mut metadata)?;

    Ok(metadata)
}

/// `process_instruction` function for `Money::FreezeV1`
pub(crate) fn money_freeze_process_instruction_v1(
    cid: ContractId,
    call_idx: u32,
    calls: Vec<ContractCall>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx as usize];
    let params: MoneyFreezeParamsV1 = deserialize(&self_.data[1..])?;

    // We just check if the mint was already frozen beforehand
    let token_freeze_db = db_lookup(cid, MONEY_CONTRACT_TOKEN_FREEZE_TREE)?;

    let (mint_x, mint_y) = params.signature_public.xy();
    let token_id = TokenId::from(poseidon_hash([mint_x, mint_y]));

    // Check that the mint is not frozen
    if db_contains_key(token_freeze_db, &serialize(&token_id))? {
        msg!("[MintV1] Error: Token mint for {} is frozen", token_id);
        return Err(MoneyError::MintFrozen.into())
    }

    // Create a state update. We only need the new coin.
    let update = MoneyFreezeUpdateV1 { signature_public: params.signature_public };
    let mut update_data = vec![];
    update_data.write_u8(MoneyFunction::FreezeV1 as u8)?;
    update.encode(&mut update_data)?;

    Ok(update_data)
}

/// `process_update` function for `Money::FreezeV1`
pub(crate) fn money_freeze_process_update_v1(
    cid: ContractId,
    update: MoneyFreezeUpdateV1,
) -> ContractResult {
    let token_freeze_db = db_lookup(cid, MONEY_CONTRACT_TOKEN_FREEZE_TREE)?;

    let (mint_x, mint_y) = update.signature_public.xy();
    let token_id = TokenId::from(poseidon_hash([mint_x, mint_y]));

    msg!("[MintV1] Freezing mint for token {}", token_id);
    db_set(token_freeze_db, &serialize(&token_id), &[])?;

    Ok(())
}
