
/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2022 Dyne.org foundation
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
    crypto::{Coin, ContractId, MerkleNode, PublicKey},
    db::{db_contains_key, db_get, db_lookup, db_set},
    error::{ContractError, ContractResult},
    merkle::merkle_add,
    msg,
    pasta::{arithmetic::CurveAffine, group::Curve, pallas},
    util::set_return_data,
};
use darkfi_serial::{deserialize, serialize, Encodable, WriteExt};

use crate::{
    instruction::MoneyFunction,
    state::{MoneyTransferParams, MoneyTransferUpdate},
    constants::{ COIN_ROOTS_TREE,NULLIFIERS_TREE, INFO_TREE, COIN_MERKLE_TREE,FAUCET_PUBKEYS, ZKAS_MINT_NS, ZKAS_BURN_NS },   
    // error::MoneyError,
};


pub fn transfer_get_metadata(params: MoneyTransferParams) -> ContractResult {

    let mut zk_public_values: Vec<(String, Vec<pallas::Base>)> = vec![];
    let mut signature_pubkeys: Vec<PublicKey> = vec![];

    for input in &params.clear_inputs {
        signature_pubkeys.push(input.signature_public);
    }

    for input in &params.inputs {
        let value_coords = input.value_commit.to_affine().coordinates().unwrap();
        let token_coords = input.token_commit.to_affine().coordinates().unwrap();
        let (sig_x, sig_y) = input.signature_public.xy();

        zk_public_values.push((
            ZKAS_BURN_NS.to_string(),
            vec![
                input.nullifier.inner(),
                *value_coords.x(),
                *value_coords.y(),
                *token_coords.x(),
                *token_coords.y(),
                input.merkle_root.inner(),
                input.user_data_enc,
                sig_x,
                sig_y,
            ],
        ));

        signature_pubkeys.push(input.signature_public);
    }

    for output in &params.outputs {
        let value_coords = output.value_commit.to_affine().coordinates().unwrap();
        let token_coords = output.token_commit.to_affine().coordinates().unwrap();

        zk_public_values.push((
            ZKAS_MINT_NS.to_string(),
            vec![
                //output.coin.inner(),
                output.coin,
                *value_coords.x(),
                *value_coords.y(),
                *token_coords.x(),
                *token_coords.y(),
            ],
        ));
    }

    // encoding the zk_public_values and signature_pubkeys into metadata
    let mut metadata = vec![];
    zk_public_values.encode(&mut metadata)?;
    signature_pubkeys.encode(&mut metadata)?;

    // Using this, we pass the above data to the host.
    set_return_data(&metadata)
    // Ok(())
}

pub fn transfer_process_instruction(cid: ContractId, params: MoneyTransferParams) -> ContractResult {
    let info_db = db_lookup(cid, INFO_TREE)?;
    let nullifier_db = db_lookup(cid, NULLIFIERS_TREE)?;
    let coin_roots_db = db_lookup(cid, COIN_ROOTS_TREE)?;

    let Some(faucet_pubkeys) = db_get(info_db, &serialize(&FAUCET_PUBKEYS.to_string()))? else {
        msg!("[Transfer] Error: Missing faucet pubkeys from info db");
        return Err(ContractError::Internal);
    };
    let faucet_pubkeys: Vec<PublicKey> = deserialize(&faucet_pubkeys)?;

    // State transition for payments
    msg!("[Transfer] Iterating over clear inputs");
    for (i, input) in params.clear_inputs.iter().enumerate() {
        let pk = input.signature_public;

        if !faucet_pubkeys.contains(&pk) {
            msg!("[Transfer] Error: Clear input {} has invalid faucet pubkey", i);
            return Err(ContractError::Custom(20))
        }
    }

    let mut new_coin_roots = vec![];
    let mut new_nullifiers = vec![];

    msg!("[Transfer] Iterating over anonymous inputs");
    for (i, input) in params.inputs.iter().enumerate() {
        // The Merkle root is used to know whether this is a coin that existed
        // in a previous state.
        if new_coin_roots.contains(&input.merkle_root) ||
            db_contains_key(coin_roots_db, &serialize(&input.merkle_root))?
        {
            msg!("[Transfer] Error: Duplicate Merkle root found in input {}", i);
            return Err(ContractError::Custom(21))
        }

        // The nullifiers should not already exist. It is the double-spend protection.
        if new_nullifiers.contains(&input.nullifier) ||
            db_contains_key(nullifier_db, &serialize(&input.nullifier))?
        {
            msg!("[Transfer] Error: Duplicate nullifier found in input {}", i);
            return Err(ContractError::Custom(22))
        }

        new_coin_roots.push(input.merkle_root);
        new_nullifiers.push(input.nullifier);
    }

    // Newly created coins for this transaction are in the outputs.
    let mut new_coins = Vec::with_capacity(params.outputs.len());
    for (i, output) in params.outputs.iter().enumerate() {
        // TODO: Should we have coins in a sled tree too to check dupes?
        if new_coins.contains(&Coin::from(output.coin)) {
            msg!("[Transfer] Error: Duplicate coin found in output {}", i);
            return Err(ContractError::Custom(23))
        }

        // FIXME: Needs some work on types and their place within all these libraries
        new_coins.push(Coin::from(output.coin))
    }

    // Create a state update
    let update = MoneyTransferUpdate { nullifiers: new_nullifiers, coins: new_coins };
    let mut update_data = vec![];
    // writing the first byte of data as the function-id
    update_data.write_u8(MoneyFunction::Transfer as u8)?;
    update.encode(&mut update_data)?;
    set_return_data(&update_data)?;
    msg!("[Transfer] State update set!");

    Ok(())

}

pub fn transfer_process_update(cid: ContractId,update: MoneyTransferUpdate) -> ContractResult {
    let info_db = db_lookup(cid, INFO_TREE)?;
    let nullifiers_db = db_lookup(cid, NULLIFIERS_TREE)?;
    let coin_roots_db = db_lookup(cid, COIN_ROOTS_TREE)?;

    for nullifier in update.nullifiers {
        db_set(nullifiers_db, &serialize(&nullifier), &[])?;
    }

    for coin in update.coins {
        // TODO: merkle_add() should take a list of coins and batch add them for efficiency
        merkle_add(
            info_db,
            coin_roots_db,
            &serialize(&COIN_MERKLE_TREE.to_string()),
            &MerkleNode::from(coin.inner()),
        )?;
    }

    Ok(())
}




