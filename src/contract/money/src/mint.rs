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
    crypto::{
        pasta_prelude::*, pedersen_commitment_base, pedersen_commitment_u64, poseidon_hash, Coin,
        ContractId, MerkleNode, PublicKey, TokenId,
    },
    db::{db_contains_key, db_lookup},
    error::ContractError,
    merkle_add, msg,
    pasta::pallas,
    ContractCall,
};
use darkfi_serial::{deserialize, serialize, Encodable, WriteExt};

use crate::{
    MoneyFunction, MoneyMintParams, MoneyMintUpdate, MONEY_CONTRACT_COIN_MERKLE_TREE,
    MONEY_CONTRACT_COIN_ROOTS_TREE, MONEY_CONTRACT_INFO_TREE, MONEY_CONTRACT_TOKEN_FREEZE_TREE,
    MONEY_CONTRACT_ZKAS_TOKEN_MINT_NS_V1,
};

pub fn money_mint_get_metadata(
    _cid: ContractId,
    call_idx: u32,
    calls: Vec<ContractCall>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx as usize];
    let params: MoneyMintParams = deserialize(&self_.data[1..])?;

    let mut zk_public_values: Vec<(String, Vec<pallas::Base>)> = vec![];
    let mut signature_pubkeys: Vec<PublicKey> = vec![];

    signature_pubkeys.push(params.input.signature_public);

    let value_coords = params.output.value_commit.to_affine().coordinates().unwrap();
    let token_coords = params.output.token_commit.to_affine().coordinates().unwrap();

    let (sig_x, sig_y) = params.input.signature_public.xy();
    let token_id = poseidon_hash([sig_x, sig_y]);

    zk_public_values.push((
        MONEY_CONTRACT_ZKAS_TOKEN_MINT_NS_V1.to_string(),
        vec![
            sig_x,
            sig_y,
            token_id,
            params.output.coin,
            *value_coords.x(),
            *value_coords.y(),
            *token_coords.x(),
            *token_coords.y(),
        ],
    ));

    let mut metadata = vec![];
    zk_public_values.encode(&mut metadata)?;
    signature_pubkeys.encode(&mut metadata)?;

    Ok(metadata)
}

pub fn money_mint_process_instruction(
    cid: ContractId,
    call_idx: u32,
    calls: Vec<ContractCall>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx as usize];
    let params: MoneyMintParams = deserialize(&self_.data[1..])?;

    //let info_db = db_lookup(cid, MONEY_CONTRACT_INFO_TREE)?;
    //let coin_roots_db = db_lookup(cid, MONEY_CONTRACT_COIN_ROOTS_TREE)?;
    let token_freeze_db = db_lookup(cid, MONEY_CONTRACT_TOKEN_FREEZE_TREE)?;

    // Check that the signature public key is actually the token ID
    let (mint_x, mint_y) = params.input.signature_public.xy();
    let token_id = TokenId::from(poseidon_hash([mint_x, mint_y]));
    if token_id != params.input.token_id {
        msg!("[Mint] Token ID does not derive from mint authority");
        return Err(ContractError::Custom(18))
    }

    // Check that the mint is not frozen
    if db_contains_key(token_freeze_db, &serialize(&token_id))? {
        msg!("[Mint] Error: The mint for token {} is frozen", token_id);
        return Err(ContractError::Custom(19))
    }

    // TODO: Check that the new coin did not exist before. We should
    //       probably have a sled tree of all coins ever in order to
    //       assert against duplicates.

    // Verify that the value and token commitments match
    if pedersen_commitment_u64(params.input.value, params.input.value_blind) !=
        params.output.value_commit
    {
        msg!("[Mint] Error: Value commitments do not match");
        return Err(ContractError::Custom(10))
    }

    if pedersen_commitment_base(params.input.token_id.inner(), params.input.token_blind) !=
        params.output.token_commit
    {
        msg!("[Mint] Error: Token commitments do not match");
        return Err(ContractError::Custom(11))
    }

    // Create a state update. We only need the new coin.
    let update = MoneyMintUpdate { coin: Coin::from(params.output.coin) };
    let mut update_data = vec![];
    update_data.write_u8(MoneyFunction::Mint as u8)?;
    update.encode(&mut update_data)?;

    Ok(update_data)
}

pub fn money_mint_process_update(
    cid: ContractId,
    update: MoneyMintUpdate,
) -> Result<(), ContractError> {
    let info_db = db_lookup(cid, MONEY_CONTRACT_INFO_TREE)?;
    let coin_roots_db = db_lookup(cid, MONEY_CONTRACT_COIN_ROOTS_TREE)?;

    let coins = vec![MerkleNode::from(update.coin.inner())];

    msg!("[Mint] Adding new coin to Merkle tree");
    merkle_add(info_db, coin_roots_db, &serialize(&MONEY_CONTRACT_COIN_MERKLE_TREE), &coins)?;

    Ok(())
}
