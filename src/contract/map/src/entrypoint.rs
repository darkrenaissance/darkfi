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
 * You should have received a copy of the GNU Affero General Public
 * License along with this program.
 * If not, see <https://www.gnu.org/licenses/>.
 */

use crate::{
    ContractFunction,
    MAP_CONTRACT_ENTRIES_TREE,
    MAP_CONTRACT_ZKAS_SET_NS,
};

use darkfi_sdk::{
    crypto::{ContractId, PublicKey, poseidon_hash},
    error::{ContractError, ContractResult},
    msg,
    pasta::pallas,
    ContractCall,
    util::set_return_data,
    db::{db_init, db_lookup, db_set, zkas_db_set},
};

use darkfi_serial::{
    serialize,
    deserialize,
    Encodable,
    WriteExt
};

use crate::model::{
    SetParamsV1,
    SetUpdateV1,
};

darkfi_sdk::define_contract!(
    init:     init_contract,
    exec:     process_instruction,
    apply:    process_update,
    metadata: get_metadata
);

fn init_contract(cid: ContractId, ix: &[u8]) -> ContractResult {
    let set_v1_bincode = include_bytes!("../proof/set_v1.zk.bin");
    zkas_db_set(&set_v1_bincode[..])?;

    if db_lookup(cid, MAP_CONTRACT_ENTRIES_TREE).is_err() {
        db_init(cid, MAP_CONTRACT_ENTRIES_TREE)?;
    }

    Ok(())
}

fn get_metadata(cid: ContractId, ix: &[u8]) -> ContractResult {
    let (call_idx, calls): (u32, Vec<ContractCall>) = deserialize(ix)?;
    if call_idx >= calls.len() as u32 {
        msg!("Error: call_idx >= calls.len()");
        return Err(ContractError::Internal);
    }

    let self_ = &calls[call_idx as usize];
    match ContractFunction::try_from(self_.data[0])? {
        ContractFunction::Set => {
            let params: SetParamsV1 = deserialize(&self_.data[1..])?;

            let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)>
                = vec![];

            zk_public_inputs.push((
                MAP_CONTRACT_ZKAS_SET_NS.to_string(),
                params.to_vec(),
            ));
    
            let mut metadata = vec![];
            zk_public_inputs.encode(&mut metadata)?;

            set_return_data(&metadata)?;
            Ok(())
        }
    }
}

fn process_instruction(cid: ContractId, ix: &[u8]) -> ContractResult {
    let (call_idx, calls): (u32, Vec<ContractCall>) = deserialize(ix)?;
    if call_idx >= calls.len() as u32 {
        msg!("Error: call_idx >= calls.len()");
        return Err(ContractError::Internal);
    }

    let self_ = calls[call_idx as usize];
    match ContractFunction::try_from(self_.data[0])? {
        ContractFunction::Set => {
            msg!("processing SET");

            let params: SetParamsV1 = deserialize(&self_.data[1..])?;
            let slot = poseidon_hash([params.account, params.key]);
            msg!("[SET] slot  = {:?}", slot);
            msg!("[SET] value = {:?}", params.value);


            let update = SetUpdateV1 {slot, value: params.value};
            let mut update_data = vec![];
            update_data.write_u8(ContractFunction::Set as u8)?;
            update.encode(&mut update_data);
            set_return_data(&update_data)?;
            msg!("[SET] State update set!");

            Ok(())
        }
    }
}

fn process_update(
    cid: ContractId,
    update_data: &[u8]
) -> ContractResult {
    match ContractFunction::try_from(update_data[0])? {
        ContractFunction::Set => {
            let update: SetUpdateV1 = deserialize(&update_data[1..])?;

            msg!("[SET] serialized_slot  = {:?}",
                 &serialize(&update.slot));
            msg!("[SET] serialized_value = {:?}",
                 &serialize(&update.value));

            let db = db_lookup(cid, MAP_CONTRACT_ENTRIES_TREE)?;
            db_set(
                db,
                &serialize(&update.slot),
                &serialize(&update.value),
            ).unwrap();

            Ok(())
        },
    }
}

