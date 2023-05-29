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
    crypto::{ContractId, PublicKey},
    db::{db_get, db_lookup, db_set},
    error::{ContractError, ContractResult},
    msg,
    pasta::pallas,
    ContractCall,
};
use darkfi_serial::{deserialize, serialize, Encodable, WriteExt};

use crate::{
    error::DeployError,
    model::{DeployParamsV1, DeployUpdateV1},
    DeployFunction, DEPLOY_CONTRACT_LOCK_TREE, DEPLOY_CONTRACT_ZKAS_DERIVE_NS_V1,
};

/// `get_metadata` function for `Deploy::DeployV1`
pub(crate) fn deploy_get_metadata_v1(
    _cid: ContractId,
    call_idx: u32,
    calls: Vec<ContractCall>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx as usize];
    let params: DeployParamsV1 = deserialize(&self_.data[1..])?;

    // Public inputs for the ZK proofs we have to verify
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    // Public keys for the transaction signatures we have to verify
    let signature_pubkeys: Vec<PublicKey> = vec![params.public_key];

    // Derive the ContractID from the public key
    let (sig_x, sig_y) = params.public_key.xy();
    let contract_id = ContractId::derive_public(params.public_key);

    // Append the ZK public inputs
    zk_public_inputs.push((
        DEPLOY_CONTRACT_ZKAS_DERIVE_NS_V1.to_string(),
        vec![sig_x, sig_y, contract_id.inner()],
    ));

    // Serialize everything gathered and return it
    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    signature_pubkeys.encode(&mut metadata)?;

    Ok(metadata)
}

/// `process_instruction` function for `Deploy::DeployV1`
pub(crate) fn deploy_process_instruction_v1(
    cid: ContractId,
    call_idx: u32,
    calls: Vec<ContractCall>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx as usize];
    let params: DeployParamsV1 = deserialize(&self_.data[1..])?;

    // In this function, we have to check that the contract isn't locked.
    let lock_db = db_lookup(cid, DEPLOY_CONTRACT_LOCK_TREE)?;
    let contract_id = ContractId::derive_public(params.public_key);

    if let Some(v) = db_get(lock_db, &serialize(&contract_id))? {
        let locked: bool = deserialize(&v)?;
        if locked {
            msg!("[DeployV1] Error: Contract is locked. Cannot redeploy.");
            return Err(DeployError::ContractLocked.into())
        }
    }

    let update = DeployUpdateV1 { contract_id };
    let mut update_data = vec![];
    update_data.write_u8(DeployFunction::DeployV1 as u8)?;
    update.encode(&mut update_data)?;
    Ok(update_data)
}

/// `process_update` function for `Deploy::DeployV1`
pub(crate) fn deploy_process_update_v1(cid: ContractId, update: DeployUpdateV1) -> ContractResult {
    // We add the contract to the list
    msg!("[DeployV1] Adding ContractID to deployed list");
    let lock_db = db_lookup(cid, DEPLOY_CONTRACT_LOCK_TREE)?;
    db_set(lock_db, &serialize(&update.contract_id), &serialize(&false))?;

    Ok(())
}
