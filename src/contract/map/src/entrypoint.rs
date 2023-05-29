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
    error::MapError
};

use darkfi_sdk::{
    crypto::{ContractId, PublicKey, poseidon_hash},
    error::{ContractError, ContractResult},
    msg,
    pasta::pallas,
    ContractCall,
    util::set_return_data,
    db::{db_init, db_lookup, db_set, zkas_db_set, db_get},
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
            let signature_pubkeys: Vec<PublicKey> = vec![];
            let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)>
                = vec![];

            zk_public_inputs.push((
                MAP_CONTRACT_ZKAS_SET_NS.to_string(),
                params.to_vec(),
            ));
    
            let mut metadata = vec![];
            zk_public_inputs.encode(&mut metadata)?;
            signature_pubkeys.encode(&mut metadata)?;

            set_return_data(&metadata)?;
            Ok(())
        }
    }
}

/// the most imporatant things to the implementation:
/// - there is 1 map, slot (number) -> value, so set and get are gas efficient
/// - slot is function of a) namespace and b) key under the namespace
///   - slot(root_namespace, darkrenaissance) = poseidon_hash(
///                                                 0,
///                                                 darkrenaissance
///                                             )
///                                           = alice_account
///   - slot(darkrenaissance, darkfi)         = poseidon_hash(
///                                                 alice_account,
///                                                 darkfi
///                                             )
///                                           = bob_account
///   - slot(darkfi, v0_4_1)                  = poseidon_hash(
///                                                 bob_account,
///                                                 v0_4_1
///                                             )
///                                           = value
/// - 0 is the special account for the canonical root
///
///
fn process_instruction(cid: ContractId, ix: &[u8]) -> ContractResult {
    let (call_idx, calls): (u32, Vec<ContractCall>) = deserialize(ix)?;
    if call_idx >= calls.len() as u32 {
        msg!("Error: call_idx >= calls.len()");
        return Err(ContractError::Internal);
    }

    match ContractFunction::try_from(ix[0])? {
        ContractFunction::Set => {
            msg!("processing SET");
            let params: SetParamsV1 = 
                deserialize(&calls[call_idx as usize].data[1..])?;
            let slot = if params.car == pallas::Base::one() {
                poseidon_hash([pallas::Base::zero(), params.key])
            } else {
                poseidon_hash([params.account, params.key])
            };

            // Question being answered by this block of code:
            // is this slot locked?
            let db = db_lookup(cid, MAP_CONTRACT_ENTRIES_TREE)?;
            match db_get(db, &serialize(&slot))? {
                None => msg!("[SET] slot has no value"),
                Some(lock) => {
                    if deserialize(&lock)? {
                        return Err(MapError::Locked.into())
                    }
                }
            };
            msg!("[SET] slot  = {:?}", slot);
            msg!("[SET] car   = {:?}", params.car);
            msg!("[SET] lock  = {:?}", params.lock);
            msg!("[SET] value = {:?}", params.value);


            let update = SetUpdateV1 {
                slot,
                lock: params.lock,
                value: params.value,
            };
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

            msg!("[SET] serialized_slot     = {:?}",
                 &serialize(&update.slot));
            msg!("[SET] serialized_slot + 1 = {:?}",
                 &serialize(&(update.slot.add(&pallas::Base::one()))));
            msg!("[SET] serialized_lock    = {:?}",
                 &serialize(&update.lock));
            msg!("[SET] serialized_value    = {:?}",
                 &serialize(&update.value));

            // key(slot)     = lock
            // key(slot + 1) = value
            let db = db_lookup(cid, MAP_CONTRACT_ENTRIES_TREE)?;
            db_set(
                db,
                &serialize(&update.slot),
                &serialize(&update.lock),
            ).unwrap();
            db_set(
                db,
                &serialize(&(update.slot.add(&pallas::Base::one()))),
                &serialize(&update.value),
            ).unwrap();

            Ok(())
        },
    }
}

