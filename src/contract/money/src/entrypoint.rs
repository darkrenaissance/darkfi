
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
    crypto::{Coin, ContractId, MerkleNode, MerkleTree, PublicKey},
    db::{db_contains_key, db_get, db_init, db_lookup, db_set},
    error::{ContractError, ContractResult},
    tx::ContractCall,
};
use darkfi_serial::{deserialize, serialize, Encodable, WriteExt};

use crate::{
    processor::{transfer_get_metadata,transfer_process_instruction, transfer_process_update},
    instruction::MoneyFunction,
    state::{MoneyTransferParams, MoneyTransferUpdate},
    constants::{ZKAS_TREE, COIN_ROOTS_TREE,NULLIFIERS_TREE, INFO_TREE, COIN_MERKLE_TREE,FAUCET_PUBKEYS, ZKAS_MINT_NS, ZKAS_BURN_NS },   
};


// this macro defines the structure of the contract , which will be useful for contract deployemnt, proof verfication and fetching metadata 
darkfi_sdk::define_contract!(
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata
);


/// This function runs when the contract is (re)deployed and initialized.
fn init_contract(cid: ContractId, ix: &[u8]) -> ContractResult {
    // The payload for now contains a vector of `PublicKey` used to
    // whitelist faucets that can create clear inputs.
    let faucet_pubkeys: Vec<PublicKey> = deserialize(ix)?;

    // The zkas circuits can simply be embedded in the wasm and set up by
    // the initialization. Note that the tree should then be called "zkas".
    // The lookups can then be done by `contract_id+zkas+namespace`.
    let zkas_db = match db_lookup(cid, ZKAS_TREE) {
        Ok(v) => v,
        Err(_) => db_init(cid, ZKAS_TREE)?,
    };
    let mint_bincode = include_bytes!("../proof/mint.zk.bin");
    let burn_bincode = include_bytes!("../proof/burn.zk.bin");

    /* TODO: Do I really want to make zkas a dependency? Yeah, in the future.
       For now we take anything.
    let zkbin = ZkBinary::decode(mint_bincode)?;
    let mint_namespace = zkbin.namespace.clone();
    assert_eq!(&mint_namespace, ZKAS_MINT_NS);
    let zkbin = ZkBinary::decode(burn_bincode)?;
    let burn_namespace = zkbin.namespace.clone();
    assert_eq!(&burn_namespace, ZKAS_BURN_NS);
    db_set(zkas_db, &serialize(&mint_namespace), &mint_bincode[..])?;
    db_set(zkas_db, &serialize(&burn_namespace), &burn_bincode[..])?;
    */
    db_set(zkas_db, &serialize(&ZKAS_MINT_NS.to_string()), &mint_bincode[..])?;
    db_set(zkas_db, &serialize(&ZKAS_BURN_NS.to_string()), &burn_bincode[..])?;

    // Set up a database tree to hold Merkle roots
    let _ = match db_lookup(cid, COIN_ROOTS_TREE) {
        Ok(v) => v,
        Err(_) => db_init(cid, COIN_ROOTS_TREE)?,
    };

    // Set up a database tree to hold nullifiers
    let _ = match db_lookup(cid, NULLIFIERS_TREE) {
        Ok(v) => v,
        Err(_) => db_init(cid, NULLIFIERS_TREE)?,
    };

    // Set up a database tree for arbitrary data
    let info_db = match db_lookup(cid, INFO_TREE) {
        Ok(v) => v,
        Err(_) => {
            let info_db = db_init(cid, INFO_TREE)?;
            // Add a Merkle tree to the info db:
            let coin_tree = MerkleTree::new(100);
            let mut coin_tree_data = vec![];
            // TODO: FIXME: What is this write_u32 doing here?
            coin_tree_data.write_u32(0)?;
            coin_tree.encode(&mut coin_tree_data)?;

            db_set(info_db, &serialize(&COIN_MERKLE_TREE.to_string()), &coin_tree_data)?;
            info_db
        }
    };

    // Whitelisted faucets
    db_set(info_db, &serialize(&FAUCET_PUBKEYS.to_string()), &serialize(&faucet_pubkeys))?;

    Ok(())
}

/// This function is used by the VM's host to fetch the necessary metadata for
/// verifying signatures and zk proofs(public_inputs).
fn get_metadata(_cid: ContractId, ix: &[u8]) -> ContractResult {
    let (call_idx, call): (u32, Vec<ContractCall>) = deserialize(ix)?;
    assert!(call_idx < call.len() as u32);

    let self_ = &call[call_idx as usize];

    // TODO : maybe we can ADD unpacking first byte logic in instruction.rs 
    // first byte i.e. self_.data[0] is function-id
    match MoneyFunction::from(self_.data[0]) {
        MoneyFunction::Transfer => {
            let params: MoneyTransferParams = deserialize(&self_.data[1..])?;
            transfer_get_metadata(params)
        },    
        // MoneyFunction::Transfer2 => {
        //     let params: MoneyTransferParams = deserialize(&self_.data[1..])?;
        //     transfer2_get_metadata(params)
        // } 
    }

    // Ok(())
}

/// This function verifies a state transition and produces an
/// update if everything is successful.
fn process_instruction(cid: ContractId, ix: &[u8]) -> ContractResult {
    let (call_idx, call): (u32, Vec<ContractCall>) = deserialize(ix)?;
    assert!(call_idx < call.len() as u32);

    let self_ = &call[call_idx as usize];

    // first byte of data i.e self_.data[0] is the function-id
    match MoneyFunction::from(self_.data[0]) {
        MoneyFunction::Transfer => {
            let params: MoneyTransferParams = deserialize(&self_.data[1..])?;
            transfer_process_instruction(cid,params)
        }
        // MoneyFunction::Transfer2 => {
        //     let params: MoneyTransferParams = deserialize(&self_.data[1..])?;
        //     transfer_process_instruction(params)
        // }
    }
}

/// This function takes in the updatedata produced from the state_transition & applies it to the current state
/// i.e add new_coins to the merkle tree and store the new_nullifiers in db
fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    match MoneyFunction::from(update_data[0]) {
        MoneyFunction::Transfer => {
            let update: MoneyTransferUpdate = deserialize(&update_data[1..])?;
            transfer_process_update(cid,update)
        }
        // MoneyFunction::Transfer2 => {
        //     let update: MoneyTransferUpdate = deserialize(&update_data[1..])?;
        //     transfer_process_update(cid,update)
        // }
    }
}
