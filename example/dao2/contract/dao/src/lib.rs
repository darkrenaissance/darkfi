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
    crypto::{ContractId, MerkleNode, MerkleTree},
    db::{db_init, db_lookup, db_set},
    define_contract,
    error::ContractResult,
    merkle::merkle_add,
    msg,
    pasta::pallas,
    tx::ContractCall,
    util::set_return_data,
};
use darkfi_serial::{
    deserialize, serialize, Encodable, SerialDecodable, SerialEncodable, WriteExt,
};

#[derive(Clone, SerialEncodable, SerialDecodable)]
pub struct DaoBulla(pub pallas::Base);

#[repr(u8)]
pub enum DaoFunction {
    Foo = 0x00,
    Mint = 0x01,
}

impl From<u8> for DaoFunction {
    fn from(b: u8) -> Self {
        match b {
            0x00 => Self::Foo,
            0x01 => Self::Mint,
            _ => panic!("Invalid function ID: {:#04x?}", b),
        }
    }
}

#[derive(SerialEncodable, SerialDecodable)]
pub struct DaoMintParams {
    pub dao_bulla: DaoBulla,
}
#[derive(SerialEncodable, SerialDecodable)]
pub struct DaoMintUpdate {
    pub dao_bulla: DaoBulla,
}

define_contract!(
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata
);

fn init_contract(cid: ContractId, _ix: &[u8]) -> ContractResult {
    let info_db = db_init(cid, "info")?;
    let _ = db_init(cid, "dao_roots")?;

    let dao_tree = MerkleTree::new(100);
    let mut dao_tree_data = Vec::new();
    dao_tree_data.write_u32(0)?;
    dao_tree.encode(&mut dao_tree_data)?;
    db_set(info_db, &serialize(&"dao_tree".to_string()), &dao_tree_data)?;

    Ok(())
}
fn get_metadata(_cid: ContractId, ix: &[u8]) -> ContractResult {
    let (call_idx, call): (u32, Vec<ContractCall>) = deserialize(ix)?;

    assert!(call_idx < call.len() as u32);
    let self_ = &call[call_idx as usize];

    match DaoFunction::from(self_.data[0]) {
        DaoFunction::Mint => {
            let data = &self_.data[1..];
            let params: DaoMintParams = deserialize(data)?;

            let zk_public_values: Vec<(String, Vec<pallas::Base>)> =
                vec![("dao-mint".to_string(), vec![params.dao_bulla.0])];
            let signature_public_keys: Vec<pallas::Point> = Vec::new();

            let mut metadata = Vec::new();
            zk_public_values.encode(&mut metadata)?;
            signature_public_keys.encode(&mut metadata)?;
            set_return_data(&metadata)?;
        }
        DaoFunction::Foo => {
            unimplemented!();
        }
    }

    Ok(())
}
fn process_instruction(cid: ContractId, ix: &[u8]) -> ContractResult {
    let (call_idx, call): (u32, Vec<ContractCall>) = deserialize(ix)?;

    assert!(call_idx < call.len() as u32);
    let self_ = &call[call_idx as usize];

    match DaoFunction::from(self_.data[0]) {
        DaoFunction::Mint => {
            let data = &self_.data[1..];
            let params: DaoMintParams = deserialize(data)?;

            // No checks in Mint. Just return the update.

            let update = DaoMintUpdate { dao_bulla: params.dao_bulla };

            let mut update_data = Vec::new();
            update_data.write_u8(DaoFunction::Mint as u8)?;
            update.encode(&mut update_data)?;
            set_return_data(&update_data)?;
            msg!("update is set!");
        }
        DaoFunction::Foo => {
            unimplemented!();
        }
    }

    Ok(())
}
fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    match DaoFunction::from(update_data[0]) {
        DaoFunction::Mint => {
            let data = &update_data[1..];
            let update: DaoMintUpdate = deserialize(data)?;

            let db_info = db_lookup(cid, "info")?;
            let db_roots = db_lookup(cid, "dao_roots")?;
            let node = MerkleNode::new(update.dao_bulla.0);
            merkle_add(db_info, db_roots, &serialize(&"dao_tree".to_string()), &node)?;
        }
        DaoFunction::Foo => {
            unimplemented!();
        }
    }

    Ok(())
}
