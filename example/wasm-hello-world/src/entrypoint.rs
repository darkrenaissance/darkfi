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
    crypto::{poseidon_hash, ContractId, PublicKey},
    dark_tree::DarkLeaf,
    error::ContractResult,
    msg,
    pasta::pallas,
    wasm::{
        self,
        db::{db_contains_key, db_del, db_init, db_lookup, db_set, zkas_db_set},
    },
    ContractCall, ContractError,
};
use darkfi_serial::{deserialize, serialize, Encodable};

use crate::{
    ContractFunction, HelloParams, HELLO_CONTRACT_MEMBER_TREE, HELLO_CONTRACT_ZKAS_SECRETCOMMIT_NS,
};

darkfi_sdk::define_contract!(
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata
);

/// This entrypoint function runs when the contract is (re)deployed and initialized.
/// We use this function to init all the necessary databases and prepare them with
/// initial data if necessary.
/// This is also the place where we bundle the zkas circuits that are to be used
/// with functions provided by the contract.
fn init_contract(cid: ContractId, _ix: &[u8]) -> ContractResult {
    // zkas circuits can simply be embedded in the wasm and set up by using
    // respective db functions.
    // The special `zkas db` operations exist in order to be able to verify
    // the circuits being bundled and enforcing a specific tree inside sled,
    // and dlso creation of VerifyingKey.
    let circuit_bincode = include_bytes!("../proof/secret_commitment.zk.bin");

    // For that, we use `zkas_db_set` and pass in the bincode.
    zkas_db_set(&circuit_bincode[..])?;

    // Now we also want to create our own database to hold things.
    // This `lookup || init` method is a redeployment guard.
    if db_lookup(cid, HELLO_CONTRACT_MEMBER_TREE).is_err() {
        db_init(cid, HELLO_CONTRACT_MEMBER_TREE)?;
    }

    Ok(())
}

fn get_metadata(_cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let _func = ContractFunction::try_from(self_.data[0])?;

    // Deserialize the call parameters
    let params: HelloParams = deserialize(&self_.data[1..])?;

    // Public inputs for the ZK proofs we have to verify
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    // Public keys for the transaction signatures we have to verify
    let signature_pubkeys: Vec<PublicKey> = vec![];

    zk_public_inputs
        .push((HELLO_CONTRACT_ZKAS_SECRETCOMMIT_NS.to_string(), vec![params.x, params.y]));

    // Serialize everything gathered and return it
    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    signature_pubkeys.encode(&mut metadata)?;

    wasm::util::set_return_data(&metadata)
}

fn process_instruction(cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = ContractFunction::try_from(self_.data[0])?;

    // Deserialize the call parameters
    let params: HelloParams = deserialize(&self_.data[1..])?;

    // Open the db
    let db_members = db_lookup(cid, HELLO_CONTRACT_MEMBER_TREE)?;

    // Pubkey commitment
    let commitment = poseidon_hash([params.x, params.y]);

    match func {
        ContractFunction::Register => {
            if db_contains_key(db_members, &serialize(&commitment))? {
                msg!("Error: Member already in database");
                return Err(ContractError::Custom(1))
            }
        }

        ContractFunction::Deregister => {
            if !db_contains_key(db_members, &serialize(&commitment))? {
                msg!("Error: Member not in database");
                return Err(ContractError::Custom(2))
            }
        }
    }

    wasm::util::set_return_data(&serialize(&commitment))
}

fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    let func = ContractFunction::try_from(update_data[0])?;
    let db_members = db_lookup(cid, HELLO_CONTRACT_MEMBER_TREE)?;

    let commitment = &update_data[1..];

    match func {
        ContractFunction::Register => db_set(db_members, commitment, &[])?,
        ContractFunction::Deregister => db_del(db_members, commitment)?,
    }

    Ok(())
}
