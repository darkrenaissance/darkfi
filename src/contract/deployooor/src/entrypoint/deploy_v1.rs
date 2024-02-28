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
    db::{db_get, db_lookup, db_set},
    deploy::DeployParamsV1,
    error::{ContractError, ContractResult},
    msg,
    pasta::pallas,
    ContractCall,
};
use darkfi_serial::{deserialize, serialize, Encodable, WriteExt};
use wasmparser::{
    ExternalKind::{Func, Memory},
    Payload::ExportSection,
};

use crate::{error::DeployError, model::DeployUpdateV1, DeployFunction, DEPLOY_CONTRACT_LOCK_TREE};

/// `get_metadata` function for `Deploy::DeployV1`
pub(crate) fn deploy_get_metadata_v1(
    _cid: ContractId,
    call_idx: u32,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx as usize];
    let params: DeployParamsV1 = deserialize(&self_.data.data[1..])?;

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

/// `process_instruction` function for `Deploy::DeployV1`
pub(crate) fn deploy_process_instruction_v1(
    cid: ContractId,
    call_idx: u32,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx as usize];
    let params: DeployParamsV1 = deserialize(&self_.data.data[1..])?;

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

    // Then validate the wasm binary
    if let Err(e) = wasmparser::validate(&params.wasm_bincode) {
        msg!("[DeployV1] Error: Failed to validate WASM binary: {}", e);
        return Err(DeployError::WasmBincodeInvalid.into())
    }

    // And find all the necessary exports/symbols
    let mut found_memory = false;
    let mut found_initialize = false;
    let mut found_entrypoint = false;
    let mut found_update = false;
    let mut found_metadata = false;

    let parser = wasmparser::Parser::new(0);
    for payload in parser.parse_all(&params.wasm_bincode) {
        let payload = match payload {
            Ok(v) => v,
            Err(e) => {
                msg!("[DeployV1] Error: Failed parsing WASM payload: {}", e);
                return Err(DeployError::WasmBincodeInvalid.into())
            }
        };

        if let ExportSection(v) = payload {
            for element in v.into_iter_with_offsets() {
                let (_, element) = match element {
                    Ok(v) => v,
                    Err(e) => {
                        msg!("[DeployV1] Error: Failed parsing WASM payload: {}", e);
                        return Err(DeployError::WasmBincodeInvalid.into())
                    }
                };

                if element.name == "memory" && element.kind == Memory {
                    found_memory = true;
                    continue
                }

                if element.name == "__initialize" && element.kind == Func {
                    found_initialize = true;
                    continue
                }

                if element.name == "__entrypoint" && element.kind == Func {
                    found_entrypoint = true;
                    continue
                }

                if element.name == "__update" && element.kind == Func {
                    found_update = true;
                    continue
                }

                if element.name == "__metadata" && element.kind == Func {
                    found_metadata = true;
                }
            }
        }
    }

    if !found_memory || !found_initialize || !found_entrypoint || !found_update || !found_metadata {
        msg!("[DeployV1] Error: Failed to find all symbols");
        return Err(DeployError::WasmBincodeInvalid.into())
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
