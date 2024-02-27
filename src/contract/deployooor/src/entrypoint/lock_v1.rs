/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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
    db::{db_contains_key, db_get, db_lookup, db_set},
    error::{ContractError, ContractResult},
    msg,
    pasta::pallas,
    ContractCall,
};
use darkfi_serial::{deserialize, serialize, Encodable, WriteExt};

use crate::{
    error::DeployError,
    model::{LockParamsV1, LockUpdateV1},
    DeployFunction, DEPLOY_CONTRACT_LOCK_TREE,
};

/// `get_metadata` function for `Deploy::LockV1`
pub(crate) fn lock_get_metadata_v1(
    _cid: ContractId,
    call_idx: u32,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx as usize];
    let params: LockParamsV1 = deserialize(&self_.data.data[1..])?;

    // Public inputs for the ZK proofs we have to verify
    let zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    // Public keys for the transaction signatures we have to verify
    let signature_pubkeys: Vec<PublicKey> = vec![params.public_key];

    // Serialize everything gathered and return it
    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    signature_pubkeys.encode(&mut metadata)?;

    Ok(metadata)
}

/// `process_instruction` function for `Deploy::LockV1`
pub(crate) fn lock_process_instruction_v1(
    cid: ContractId,
    call_idx: u32,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx as usize];
    let params: LockParamsV1 = deserialize(&self_.data.data[1..])?;

    // In this function, we check that the contract exists, and that it isn't
    // already locked.
    let lock_db = db_lookup(cid, DEPLOY_CONTRACT_LOCK_TREE)?;
    let contract_id = ContractId::derive_public(params.public_key);

    if !db_contains_key(lock_db, &serialize(&contract_id))? {
        msg!("[LockV1] Error: Contract ID doesn't exist.");
        return Err(DeployError::ContractNonExistent.into())
    }

    let v = db_get(lock_db, &serialize(&contract_id))?.unwrap();
    let locked: bool = deserialize(&v)?;
    if locked {
        msg!("[LockV1] Error: Contract already locked.");
        return Err(DeployError::ContractLocked.into())
    }

    let update = LockUpdateV1 { contract_id };
    let mut update_data = vec![];
    update_data.write_u8(DeployFunction::LockV1 as u8)?;
    update.encode(&mut update_data)?;

    Ok(update_data)
}

/// `process_update` function for `Deploy::LockV1`
pub(crate) fn lock_process_update_v1(cid: ContractId, update: LockUpdateV1) -> ContractResult {
    // We make the contract immutable
    msg!("[LockV1] Making ContractID immutable");
    let lock_db = db_lookup(cid, DEPLOY_CONTRACT_LOCK_TREE)?;
    db_set(lock_db, &serialize(&update.contract_id), &serialize(&true))?;

    Ok(())
}
