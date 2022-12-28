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
    crypto::{ContractId, MerkleNode, MerkleTree, PublicKey},
    db::{db_init, db_lookup, db_set},
    define_contract,
    error::ContractResult,
    merkle::merkle_add,
    msg,
    pasta::{arithmetic::CurveAffine, group::Curve, pallas},
    tx::ContractCall,
    util::set_return_data,
};
use darkfi_serial::{
    deserialize, serialize, Encodable, SerialDecodable, SerialEncodable, WriteExt,
};

#[repr(u8)]
pub enum MoneyFunction {
    Transfer = 0x00,
}

impl From<u8> for MoneyFunction {
    fn from(b: u8) -> Self {
        match b {
            0x00 => Self::Transfer,
            _ => panic!("Invalid function ID: {:#04x?}", b),
        }
    }
}

#[derive(SerialEncodable, SerialDecodable)]
pub struct MoneyTransferParams {
    /// Clear inputs
    pub clear_inputs: Vec<ClearInput>,
    /// Anonymous inputs
    pub inputs: Vec<Input>,
    /// Anonymous outputs
    pub outputs: Vec<Output>,
}
#[derive(SerialEncodable, SerialDecodable)]
pub struct MoneyTransferUpdate {
    /// Nullifiers
    pub nullifiers: Vec<pallas::Base>,
    /// Coins
    pub coins: Vec<pallas::Base>,
}

/// A transaction's clear input
#[derive(SerialEncodable, SerialDecodable)]
pub struct ClearInput {
    /// Input's value (amount)
    pub value: u64,
    /// Input's token ID
    pub token_id: pallas::Base,
    /// Blinding factor for `value`
    pub value_blind: pallas::Scalar,
    /// Blinding factor for `token_id`
    pub token_blind: pallas::Scalar,
    /// Public key for the signature
    pub signature_public: PublicKey,
}

/// A transaction's anonymous input
#[derive(SerialEncodable, SerialDecodable)]
pub struct Input {
    // Public inputs for the zero-knowledge proof
    pub value_commit: pallas::Point,
    pub token_commit: pallas::Point,
    pub nullifier: pallas::Base,
    pub merkle_root: pallas::Base,
    pub spend_hook: pallas::Base,
    pub user_data_enc: pallas::Base,
    pub signature_public: PublicKey,
}

/// A transaction's anonymous output
#[derive(SerialEncodable, SerialDecodable)]
pub struct Output {
    // Public inputs for the zero-knowledge proof
    pub value_commit: pallas::Point,
    pub token_commit: pallas::Point,
    pub coin: pallas::Base,
    /// The encrypted note ciphertext
    pub ciphertext: Vec<u8>,
    pub ephem_public: PublicKey,
}

define_contract!(
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata
);

fn init_contract(cid: ContractId, _ix: &[u8]) -> ContractResult {
    let info_db = db_init(cid, "info")?;
    let _ = db_init(cid, "coin_roots")?;

    let coin_tree = MerkleTree::new(100);
    let mut coin_tree_data = Vec::new();
    coin_tree_data.write_u32(0)?;
    coin_tree.encode(&mut coin_tree_data)?;
    db_set(info_db, &serialize(&"coin_tree".to_string()), &coin_tree_data)?;

    let _ = db_init(cid, "nulls")?;

    Ok(())
}
fn get_metadata(_cid: ContractId, ix: &[u8]) -> ContractResult {
    let (call_idx, call): (u32, Vec<ContractCall>) = deserialize(ix)?;

    assert!(call_idx < call.len() as u32);
    let self_ = &call[call_idx as usize];

    match MoneyFunction::from(self_.data[0]) {
        MoneyFunction::Transfer => {
            let data = &self_.data[1..];
            let params: MoneyTransferParams = deserialize(data)?;

            let mut zk_public_values: Vec<(String, Vec<pallas::Base>)> = Vec::new();
            let mut signature_public_keys: Vec<pallas::Point> = Vec::new();

            for input in &params.clear_inputs {
                signature_public_keys.push(input.signature_public.inner());
            }
            for input in &params.inputs {
                let value_coords = input.value_commit.to_affine().coordinates().unwrap();
                let token_coords = input.token_commit.to_affine().coordinates().unwrap();
                let (sig_x, sig_y) = input.signature_public.xy();

                zk_public_values.push((
                    "money-transfer-burn".to_string(),
                    vec![
                        input.nullifier,
                        *value_coords.x(),
                        *value_coords.y(),
                        *token_coords.x(),
                        *token_coords.y(),
                        input.merkle_root,
                        input.user_data_enc,
                        sig_x,
                        sig_y,
                    ],
                ));

                signature_public_keys.push(input.signature_public.inner());
            }
            for output in &params.outputs {
                let value_coords = output.value_commit.to_affine().coordinates().unwrap();
                let token_coords = output.token_commit.to_affine().coordinates().unwrap();

                zk_public_values.push((
                    "money-transfer-mint".to_string(),
                    vec![
                        output.coin,
                        *value_coords.x(),
                        *value_coords.y(),
                        *token_coords.x(),
                        *token_coords.y(),
                    ],
                ));
            }

            let mut metadata = Vec::new();
            zk_public_values.encode(&mut metadata)?;
            signature_public_keys.encode(&mut metadata)?;
            set_return_data(&metadata)?;
        }
    }
    Ok(())
}
fn process_instruction(cid: ContractId, ix: &[u8]) -> ContractResult {
    let (call_idx, call): (u32, Vec<ContractCall>) = deserialize(ix)?;

    assert!(call_idx < call.len() as u32);
    let self_ = &call[call_idx as usize];

    match MoneyFunction::from(self_.data[0]) {
        MoneyFunction::Transfer => {
            let data = &self_.data[1..];
            let params: MoneyTransferParams = deserialize(data)?;

            // TODO: implement state_transition() checks here

            let update = MoneyTransferUpdate {
                nullifiers: params.inputs.iter().map(|input| input.nullifier).collect(),
                coins: params.outputs.iter().map(|output| output.coin).collect(),
            };

            let mut update_data = Vec::new();
            update_data.write_u8(MoneyFunction::Transfer as u8)?;
            update.encode(&mut update_data)?;
            set_return_data(&update_data)?;
            msg!("update is set!");
        }
    }
    Ok(())
}
fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    match MoneyFunction::from(update_data[0]) {
        MoneyFunction::Transfer => {
            let data = &update_data[1..];
            let update: MoneyTransferUpdate = deserialize(data)?;

            let db_info = db_lookup(cid, "info")?;
            let db_nulls = db_lookup(cid, "nulls")?;
            for nullifier in update.nullifiers {
                db_set(db_nulls, &serialize(&nullifier), &[])?;
            }
            let db_roots = db_lookup(cid, "coin_roots")?;
            for coin in update.coins {
                let node = MerkleNode::new(coin);
                // TODO: merkle_add() should take a list of coins and batch add them
                // for efficiency
                merkle_add(db_info, db_roots, &serialize(&"coin_tree".to_string()), &node)?;
            }
        }
    }

    Ok(())
}
