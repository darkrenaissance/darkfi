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
        pasta_prelude::*, pedersen_commitment_u64, poseidon_hash, ContractId, MerkleNode,
        DARK_TOKEN_ID,
    },
    db::{db_contains_key, db_lookup, db_set},
    error::{ContractError, ContractResult},
    merkle_add, msg,
    pasta::pallas,
    util::get_verifying_slot,
    ContractCall,
};
use darkfi_serial::{deserialize, serialize, Encodable, WriteExt};

use crate::{
    error::MoneyError,
    model::{MoneyGenesisMintParamsV1, MoneyGenesisMintUpdateV1},
    MoneyFunction, MONEY_CONTRACT_COINS_TREE, MONEY_CONTRACT_COIN_MERKLE_TREE,
    MONEY_CONTRACT_COIN_ROOTS_TREE, MONEY_CONTRACT_INFO_TREE, MONEY_CONTRACT_LATEST_COIN_ROOT,
    MONEY_CONTRACT_ZKAS_MINT_NS_V1,
};

/// `get_metadata` function for `Money::GenesisMintV1`
pub(crate) fn money_genesis_mint_get_metadata_v1(
    _cid: ContractId,
    call_idx: u32,
    calls: Vec<ContractCall>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx as usize];
    let params: MoneyGenesisMintParamsV1 = deserialize(&self_.data[1..])?;

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

/// `process_instruction` function for `Money::GenesisMintV1`
pub(crate) fn money_genesis_mint_process_instruction_v1(
    cid: ContractId,
    call_idx: u32,
    calls: Vec<ContractCall>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx as usize];
    let params: MoneyGenesisMintParamsV1 = deserialize(&self_.data[1..])?;

    // Verify this contract call is verified against on genesis slot(0).
    let verifying_slot = get_verifying_slot();
    if verifying_slot != 0 {
        msg!("[GenesisMintV1] Error: Call is executed for slot {}, not genesis", verifying_slot);
        return Err(MoneyError::GenesisCallNonGenesisSlot.into())
    }

    // Only DARK_TOKEN_ID can be minted on genesis slot.
    if params.input.token_id != *DARK_TOKEN_ID {
        msg!("[GenesisMintV1] Error: Clear input used non-native token");
        return Err(MoneyError::TransferClearInputNonNativeToken.into())
    }

    // Access the necessary databases where there is information to
    // validate this state transition.
    let coins_db = db_lookup(cid, MONEY_CONTRACT_COINS_TREE)?;

    // Check that the coin from the output hasn't existed before.
    if db_contains_key(coins_db, &serialize(&params.output.coin))? {
        msg!("[GenesisMintV1] Error: Duplicate coin in output");
        return Err(MoneyError::DuplicateCoin.into())
    }

    // Verify that the value and token commitments match. In here we just
    // confirm that the clear input and the anon output have the same
    // commitments.
    if pedersen_commitment_u64(params.input.value, params.input.value_blind) !=
        params.output.value_commit
    {
        msg!("[GenesisMintV1] Error: Value commitment mismatch");
        return Err(MoneyError::ValueMismatch.into())
    }

    if poseidon_hash([params.input.token_id.inner(), params.input.token_blind]) !=
        params.output.token_commit
    {
        msg!("[GenesisMintV1] Error: Token commitment mismatch");
        return Err(MoneyError::TokenMismatch.into())
    }

    // Create a state update. We only need the new coin.
    let update = MoneyGenesisMintUpdateV1 { coin: params.output.coin };
    let mut update_data = vec![];
    update_data.write_u8(MoneyFunction::GenesisMintV1 as u8)?;
    update.encode(&mut update_data)?;

    Ok(update_data)
}

/// `process_update` function for `Money::GenesisMintV1`
pub(crate) fn money_genesis_mint_process_update_v1(
    cid: ContractId,
    update: MoneyGenesisMintUpdateV1,
) -> ContractResult {
    // Grab all db handles we want to work on
    let info_db = db_lookup(cid, MONEY_CONTRACT_INFO_TREE)?;
    let coins_db = db_lookup(cid, MONEY_CONTRACT_COINS_TREE)?;
    let coin_roots_db = db_lookup(cid, MONEY_CONTRACT_COIN_ROOTS_TREE)?;

    msg!("[GenesisMintV1] Adding new coin to the set");
    db_set(coins_db, &serialize(&update.coin), &[])?;

    msg!("[GenesisMintV1] Adding new coin to the Merkle tree");
    let coins = vec![MerkleNode::from(update.coin.inner())];
    merkle_add(
        info_db,
        coin_roots_db,
        &serialize(&MONEY_CONTRACT_LATEST_COIN_ROOT),
        &serialize(&MONEY_CONTRACT_COIN_MERKLE_TREE),
        &coins,
    )?;

    Ok(())
}
