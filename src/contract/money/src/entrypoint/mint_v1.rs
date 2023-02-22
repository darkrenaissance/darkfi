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
    db::{db_contains_key, db_lookup, db_set},
    error::{ContractError, ContractResult},
    merkle_add, msg,
    pasta::pallas,
    ContractCall,
};
use darkfi_serial::{deserialize, serialize, Encodable, WriteExt};

use crate::{
    error::MoneyError,
    model::{MoneyMintParamsV1, MoneyMintUpdateV1},
    MoneyFunction, MONEY_CONTRACT_COINS_TREE, MONEY_CONTRACT_COIN_MERKLE_TREE,
    MONEY_CONTRACT_COIN_ROOTS_TREE, MONEY_CONTRACT_INFO_TREE, MONEY_CONTRACT_TOKEN_FREEZE_TREE,
    MONEY_CONTRACT_ZKAS_TOKEN_MINT_NS_V1,
};

/// `get_metadata` function for `Money::MintV1`
pub(crate) fn money_mint_get_metadata_v1(
    _cid: ContractId,
    call_idx: u32,
    calls: Vec<ContractCall>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx as usize];
    let params: MoneyMintParamsV1 = deserialize(&self_.data[1..])?;

    // Public inputs for the ZK proofs we have to verify
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    // Public keys for the transaction signatures we have to verify
    let mut signature_pubkeys: Vec<PublicKey> = vec![];

    // The minting transaction creates 1 clear input and 1 anonymous output.
    // We check the signature from the clear input, which is supposed to be
    // signed by the mint authority.
    signature_pubkeys.push(params.input.signature_public);

    let value_coords = params.output.value_commit.to_affine().coordinates().unwrap();
    let token_coords = params.output.value_commit.to_affine().coordinates().unwrap();

    // Since we expect a signature from the mint authority, we use those coordinates
    // as public inputs for the ZK proof:
    let (sig_x, sig_y) = params.input.signature_public.xy();
    let token_id = poseidon_hash([sig_x, sig_y]);

    zk_public_inputs.push((
        MONEY_CONTRACT_ZKAS_TOKEN_MINT_NS_V1.to_string(),
        vec![
            sig_x,
            sig_y,
            token_id,
            params.output.coin.inner(),
            *value_coords.x(),
            *value_coords.y(),
            *token_coords.x(),
            *token_coords.y(),
        ],
    ));

    // Serialize everything gathered and return it
    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    signature_pubkeys.encode(&mut metadata)?;

    Ok(metadata)
}

/// `process_instruction` function for `Money::MintV1`
pub(crate) fn money_mint_process_instruction_v1(
    cid: ContractId,
    call_idx: u32,
    calls: Vec<ContractCall>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx as usize];
    let params: MoneyMintParamsV1 = deserialize(&self_.data[1..])?;

    // We have to check if the token mint is frozen, and if by some chance
    // the minted coin has existed already.
    let coins_db = db_lookup(cid, MONEY_CONTRACT_COINS_TREE)?;
    let token_freeze_db = db_lookup(cid, MONEY_CONTRACT_TOKEN_FREEZE_TREE)?;

    // Check that the signature public key is actually the token ID
    let (mint_x, mint_y) = params.input.signature_public.xy();
    let token_id = TokenId::from(poseidon_hash([mint_x, mint_y]));
    if token_id != params.input.token_id {
        msg!("[MintV1] Error: Token ID does not derive from mint authority");
        return Err(MoneyError::TokenIdDoesNotDeriveFromMint.into())
    }

    // Check that the mint is not frozen
    if db_contains_key(token_freeze_db, &serialize(&token_id))? {
        msg!("[MintV1] Error: Token mint for {} is frozen", token_id);
        return Err(MoneyError::MintFrozen.into())
    }

    // Check that the coin from the output hasn't existed before
    if db_contains_key(coins_db, &serialize(&params.output.coin))? {
        msg!("[MintV1] Error: Duplicate coin in output");
        return Err(MoneyError::DuplicateCoin.into())
    }

    // Verify that the value and token commitments match. In here we just
    // confirm that the clear input and the anon output have the same
    // commitments.
    if pedersen_commitment_u64(params.input.value, params.input.value_blind) !=
        params.output.value_commit
    {
        msg!("[MintV1] Error: Value commitment mismatch");
        return Err(MoneyError::ValueMismatch.into())
    }

    if pedersen_commitment_base(params.input.token_id.inner(), params.input.token_blind) !=
        params.output.token_commit
    {
        msg!("[MintV1] Error: Token commitment mismatch");
        return Err(MoneyError::TokenMismatch.into())
    }

    // Create a state update. We only need the new coin.
    let update = MoneyMintUpdateV1 { coin: Coin::from(params.output.coin) };
    let mut update_data = vec![];
    update_data.write_u8(MoneyFunction::MintV1 as u8)?;
    update.encode(&mut update_data)?;

    Ok(update_data)
}

/// `process_update` function for `Money::MintV1`
pub(crate) fn money_mint_process_update_v1(
    cid: ContractId,
    update: MoneyMintUpdateV1,
) -> ContractResult {
    // Grab all db handles we want to work on
    let info_db = db_lookup(cid, MONEY_CONTRACT_INFO_TREE)?;
    let coins_db = db_lookup(cid, MONEY_CONTRACT_COINS_TREE)?;
    let coin_roots_db = db_lookup(cid, MONEY_CONTRACT_COIN_ROOTS_TREE)?;

    msg!("[MintV1] Adding new coin to the set");
    db_set(coins_db, &serialize(&update.coin), &[])?;

    msg!("[MintV1] Adding new coin to the Merkle tree");
    let coins = vec![MerkleNode::from(update.coin.inner())];
    merkle_add(info_db, coin_roots_db, &serialize(&MONEY_CONTRACT_COIN_MERKLE_TREE), &coins)?;

    Ok(())
}
