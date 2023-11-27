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
    crypto::ContractId,
    db::{db_init, db_lookup, db_set, zkas_db_set},
    error::{ContractError, ContractResult},
    msg,
    util::set_return_data,
    ContractCall,
};
use darkfi_serial::{deserialize, serialize};

use crate::{
    model::{DeployUpdateV1, LockUpdateV1},
    DeployFunction, DEPLOY_CONTRACT_DB_VERSION, DEPLOY_CONTRACT_INFO_TREE,
    DEPLOY_CONTRACT_LOCK_TREE,
};

/// `Deployooor::Deploy` functions
mod deploy_v1;
use deploy_v1::{deploy_get_metadata_v1, deploy_process_instruction_v1, deploy_process_update_v1};

/// `Deployooor::Lock` functions
mod lock_v1;
use lock_v1::{lock_get_metadata_v1, lock_process_instruction_v1, lock_process_update_v1};

darkfi_sdk::define_contract!(
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata
);

/// This entrypoint function runs when the contract is (re)deployed and initialized.
/// We use this function to initialize all the necessary databases and prepare them
/// with initial data if necessary.
fn init_contract(cid: ContractId, _ix: &[u8]) -> ContractResult {
    // Set up the zkas circuit tree
    let derive_cid_bincode = include_bytes!("../proof/derive_contract_id.zk");
    zkas_db_set(&derive_cid_bincode[..])?;

    // Set up a database tree for arbitrary data
    let info_db = match db_lookup(cid, DEPLOY_CONTRACT_INFO_TREE) {
        Ok(v) => v,
        Err(_) => db_init(cid, DEPLOY_CONTRACT_INFO_TREE)?,
    };

    // Set up a database to hold the set of locked contracts
    // k=ContractId, v=bool
    if db_lookup(cid, DEPLOY_CONTRACT_LOCK_TREE).is_err() {
        db_init(cid, DEPLOY_CONTRACT_LOCK_TREE)?;
    }

    // Update db version
    db_set(
        info_db,
        &serialize(&DEPLOY_CONTRACT_DB_VERSION),
        &serialize(&env!("CARGO_PKG_VERSION")),
    )?;

    Ok(())
}

/// This function is used by the wasm VM's host to fetch the necessary metadata
/// for verifying signatures and zk proofs. The payload given here are all the
/// contract calls in the transaction.
fn get_metadata(cid: ContractId, ix: &[u8]) -> ContractResult {
    let (call_idx, calls): (u32, Vec<ContractCall>) = deserialize(ix)?;
    if call_idx >= calls.len() as u32 {
        msg!("Error: call_idx >= calls.len()");
        return Err(ContractError::Internal)
    }

    match DeployFunction::try_from(calls[call_idx as usize].data[0])? {
        DeployFunction::DeployV1 => {
            let metadata = deploy_get_metadata_v1(cid, call_idx, calls)?;
            Ok(set_return_data(&metadata)?)
        }

        DeployFunction::LockV1 => {
            let metadata = lock_get_metadata_v1(cid, call_idx, calls)?;
            Ok(set_return_data(&metadata)?)
        }
    }
}

/// This function verifies a state transition and produces a state update
/// if everything is successful.
fn process_instruction(cid: ContractId, ix: &[u8]) -> ContractResult {
    let (call_idx, calls): (u32, Vec<ContractCall>) = deserialize(ix)?;
    if call_idx >= calls.len() as u32 {
        msg!("Error: call_idx >= calls.len()");
        return Err(ContractError::Internal)
    }

    match DeployFunction::try_from(calls[call_idx as usize].data[0])? {
        DeployFunction::DeployV1 => {
            let update_data = deploy_process_instruction_v1(cid, call_idx, calls)?;
            Ok(set_return_data(&update_data)?)
        }

        DeployFunction::LockV1 => {
            let update_data = lock_process_instruction_v1(cid, call_idx, calls)?;
            Ok(set_return_data(&update_data)?)
        }
    }
}

/// This function attempts to write a given state update provided the previous
/// steps of the contract call execution were all successful. It's the last in
/// line, and assumes that the transaction/call was successful. The payload
/// given to the function is the update data retrieved from `process_instruction()`.
fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    match DeployFunction::try_from(update_data[0])? {
        DeployFunction::DeployV1 => {
            let update: DeployUpdateV1 = deserialize(&update_data[1..])?;
            Ok(deploy_process_update_v1(cid, update)?)
        }

        DeployFunction::LockV1 => {
            let update: LockUpdateV1 = deserialize(&update_data[1..])?;
            Ok(lock_process_update_v1(cid, update)?)
        }
    }
}
