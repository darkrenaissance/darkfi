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
    crypto::ContractId,
    db::{db_get, db_init, db_lookup, db_set},
    define_contract,
    error::ContractResult,
    msg,
    pasta::pallas,
    tx::FuncCall,
    util::{set_return_data, put_object_bytes, get_object_bytes, get_object_size},
};
use darkfi_serial::{deserialize, serialize, Encodable, SerialDecodable, SerialEncodable, WriteExt, ReadExt};

/// Available functions for this contract.
/// We identify them with the first byte passed in through the payload.
#[repr(u8)]
pub enum Function {
    Foo = 0x00,
    Bar = 0x01,
}

impl From<u8> for Function {
    fn from(b: u8) -> Self {
        match b {
            0x00 => Self::Foo,
            0x01 => Self::Bar,
            _ => panic!("Invalid function ID: {:#04x?}", b),
        }
    }
}

// An example of deserializing the payload into a struct
#[derive(SerialEncodable, SerialDecodable)]
pub struct FooCallData {
    pub a: u64,
    pub b: u64,
}

impl FooCallData {
    //fn zk_public_values(&self) -> Vec<(String, Vec<DrkCircuitField>)>;

    //fn get_metadata(&self) {
    //}
}

#[derive(SerialEncodable, SerialDecodable)]
pub struct BarArgs {
    pub x: u32,
}

#[derive(SerialEncodable, SerialDecodable)]
pub struct FooUpdate {
    pub name: String,
    pub age: u32,
}

define_contract!(
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata
);

fn init_contract(cid: ContractId, _ix: &[u8]) -> ContractResult {
    msg!("wakeup wagies!");

    // Initialize a state tree. db_init will fail if the tree already exists.
    // Otherwise, it will return a `DbHandle` that can be used further on.
    // TODO: If the deploy execution fails, whatever is initialized with db_init
    //       should be deleted from sled afterwards. There's no way to create a
    //       tree but only apply the creation when we're done, so db_init creates
    //       it and upon failure it should delete it
    let wagies_handle = db_init(cid, "wagies")?;
    db_set(wagies_handle, &serialize(&"jason_gulag".to_string()), &serialize(&110))?;

    Ok(())
}

fn get_metadata(_cid: ContractId, ix: &[u8]) -> ContractResult {
    match Function::from(ix[0]) {
        Function::Foo => {
            let tx_data = &ix[1..];
            // ...
            let (func_call_index, func_calls): (u32, Vec<FuncCall>) = deserialize(tx_data)?;
            let _call_data: FooCallData =
                deserialize(&func_calls[func_call_index as usize].call_data)?;

            let zk_public_values = vec![
                (
                    "DaoProposeInput".to_string(),
                    vec![pallas::Base::from(110), pallas::Base::from(4)],
                ),
                ("DaoProposeInput".to_string(), vec![pallas::Base::from(7), pallas::Base::from(4)]),
                (
                    "DaoProposeMain".to_string(),
                    vec![
                        pallas::Base::from(1),
                        pallas::Base::from(3),
                        pallas::Base::from(5),
                        pallas::Base::from(7),
                    ],
                ),
            ];

            let signature_public_keys: Vec<pallas::Point> = vec![
                //pallas::Point::identity()
            ];

            let mut metadata = Vec::new();
            zk_public_values.encode(&mut metadata)?;
            signature_public_keys.encode(&mut metadata)?;
            set_return_data(&metadata)?;
            msg!("metadata returned!");

            // Convert call_data to halo2 public inputs
            // Pass this to the env
        }
        Function::Bar => {
            // ...
        }
    }
    Ok(())
}

// This is the main entrypoint function where the payload is fed.
// Through here, you can branch out into different functions inside
// this library.
fn process_instruction(cid: ContractId, ix: &[u8]) -> ContractResult {
    msg!("process_instruction():");
    msg!("    ix: {:x?}", ix);
    msg!("    cid: {:x?}", cid);

    //let bytes = [0xde, 0xad, 0xbe, 0xef];
    let bytes = [0x3a, 0x14, 0x15, 0x92, 0x63, 0x35];
    let obj = put_object_bytes(&bytes);
    let obj_size = get_object_size(obj as u32);
    msg!("    obj_size: {}", obj_size);
    let mut buf = vec![0u8; obj_size as usize];
    get_object_bytes(&mut buf, obj as u32);
    msg!("    buf (bytes): {:x?}", &buf);

    match Function::from(ix[0]) {
        Function::Foo => {
            let tx_data = &ix[1..];
            // ...
            let (func_call_index, func_calls): (u32, Vec<FuncCall>) = deserialize(tx_data)?;
            let call_data: FooCallData =
                deserialize(&func_calls[func_call_index as usize].call_data)?;
            msg!("call_data {{ a: {}, b: {} }}", call_data.a, call_data.b);
            // ...
            let update = FooUpdate { name: "john_doe".to_string(), age: 110 };

            let mut update_data = vec![Function::Foo as u8];
            update_data.extend_from_slice(&serialize(&update));
            set_return_data(&update_data)?;
            msg!("update is set!");

            // Example: try to get a value from the db
            let db_handle = db_lookup(cid, "wagies")?;

            if let Some(age_data) = db_get(db_handle, &serialize(&"jason_gulag".to_string()))? {
                let age_data: u32 = deserialize(&age_data)?;
                msg!("wagie age data: {}", age_data);
            } else {
                msg!("didn't find wagie age data");
            }
        }
        Function::Bar => {
            //let tx_data = &ix[1..];
            // ...
            //let _args: BarArgs = deserialize(tx_data)?;
        }
    }

    msg!("process_instruction() [END]");
    Ok(())
}

fn process_update(_cid: ContractId, update_data: &[u8]) -> ContractResult {
    msg!("Make 1 update!");

    match Function::from(update_data[0]) {
        Function::Foo => {
            msg!("fooupp");
            let update: FooUpdate = deserialize(&update_data[1..])?;

            // Write the wagie to the db
            //let tx_handle = db_begin_tx()?;
            //db_set(tx_handle, &serialize(&update.name), &serialize(&update.age))?;
            //let db_handle = db_lookup("wagies")?;
            //db_end_tx(db_handle, tx_handle)?;
        }
        Function::Bar => {
        }
    }

    msg!("process_update() finished");
    Ok(())
}
