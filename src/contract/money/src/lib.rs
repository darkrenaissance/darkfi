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
    crypto::{ContractId, MerkleNode, MerkleTree, PublicKey},
    db::{db_contains_key, db_get, db_init, db_lookup, db_set},
    define_contract,
    error::{ContractError, ContractResult},
    merkle::merkle_add,
    msg,
    pasta::{arithmetic::CurveAffine, group::Curve, pallas},
    tx::ContractCall,
    util::set_return_data,
};
use darkfi_serial::{deserialize, serialize, Encodable, WriteExt};

/// Functions we allow in this contract
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

/// Structures and object definitions
pub mod state;
use state::{MoneyTransferParams, MoneyTransferUpdate};

define_contract!(
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata
);

// These are the different sled trees that will be created
pub const ZKAS_TREE: &str = "zkas"; // <-- This should be a constant in darkfi-sdk
pub const COIN_ROOTS_TREE: &str = "coin_roots";
pub const NULLIFIERS_TREE: &str = "nullifiers";
pub const INFO_TREE: &str = "info";

// This is a key inside the info tree
pub const COIN_MERKLE_TREE: &str = "coin_tree";
pub const FAUCET_PUBKEYS: &str = "faucet_pubkeys";

/// zkas mint contract namespace
pub const ZKAS_MINT_NS: &str = "Mint";
/// zkas burn contract namespace
pub const ZKAS_BURN_NS: &str = "Burn";

/// This function runs when the contract is (re)deployed and initialized.
fn init_contract(cid: ContractId, _ix: &[u8]) -> ContractResult {
    // The zkas circuits can simply be embedded in the wasm and set up by
    // the initialization. Note that the tree should then be called "zkas".
    // The lookups can then be done by `contract_id+zkas+namespace`.
    let zkas_db = match db_lookup(cid, ZKAS_TREE) {
        Ok(v) => v,
        Err(_) => db_init(cid, ZKAS_TREE)?,
    };
    let mint_bincode = include_bytes!("../proof/mint.zk.bin");
    let burn_bincode = include_bytes!("../proof/burn.zk.bin");

    /* TODO: Do I really want to make zkas a dependency? Yeah, in the future.
       For now we take anything.
    let zkbin = ZkBinary::decode(mint_bincode)?;
    let mint_namespace = zkbin.namespace.clone();
    assert_eq!(&mint_namespace, ZKAS_MINT_NS);
    let zkbin = ZkBinary::decode(burn_bincode)?;
    let burn_namespace = zkbin.namespace.clone();
    assert_eq!(&burn_namespace, ZKAS_BURN_NS);
    db_set(zkas_db, &serialize(&mint_namespace), &mint_bincode[..])?;
    db_set(zkas_db, &serialize(&burn_namespace), &burn_bincode[..])?;
    */
    db_set(zkas_db, &serialize(&ZKAS_MINT_NS.to_string()), &mint_bincode[..])?;
    db_set(zkas_db, &serialize(&ZKAS_BURN_NS.to_string()), &burn_bincode[..])?;

    // Set up a database tree to hold Merkle roots
    let _ = match db_lookup(cid, COIN_ROOTS_TREE) {
        Ok(v) => v,
        Err(_) => db_init(cid, COIN_ROOTS_TREE)?,
    };

    // Set up a database tree to hold nullifiers
    let _ = match db_lookup(cid, NULLIFIERS_TREE) {
        Ok(v) => v,
        Err(_) => db_init(cid, NULLIFIERS_TREE)?,
    };

    // Set up a database tree for arbitrary data
    let info_db = match db_lookup(cid, INFO_TREE) {
        Ok(v) => v,
        Err(_) => {
            let info_db = db_init(cid, INFO_TREE)?;
            // Add a Merkle tree to the info db:
            let coin_tree = MerkleTree::new(100);
            let mut coin_tree_data = vec![];
            // TODO: FIXME: What is this write_u32 doing here?
            coin_tree_data.write_u32(0)?;
            coin_tree.encode(&mut coin_tree_data)?;

            db_set(info_db, &serialize(&COIN_MERKLE_TREE.to_string()), &coin_tree_data)?;
            info_db
        }
    };

    // Whitelisted faucets
    let faucet_pubkeys: Vec<PublicKey> = vec![];
    db_set(info_db, &serialize(&FAUCET_PUBKEYS.to_string()), &serialize(&faucet_pubkeys))?;

    Ok(())
}

/// This function is used by the VM's host to fetch the necessary metadata for
/// verifying signatures and zk proofs.
fn get_metadata(_cid: ContractId, ix: &[u8]) -> ContractResult {
    let (call_idx, call): (u32, Vec<ContractCall>) = deserialize(ix)?;
    assert!(call_idx < call.len() as u32);

    let self_ = &call[call_idx as usize];

    match MoneyFunction::from(self_.data[0]) {
        MoneyFunction::Transfer => {
            let params: MoneyTransferParams = deserialize(&self_.data[1..])?;

            let mut zk_public_values: Vec<(String, Vec<pallas::Base>)> = vec![];
            let mut signature_pubkeys: Vec<PublicKey> = vec![];

            for input in &params.clear_inputs {
                signature_pubkeys.push(input.signature_public);
            }

            for input in &params.inputs {
                let value_coords = input.value_commit.to_affine().coordinates().unwrap();
                let token_coords = input.token_commit.to_affine().coordinates().unwrap();
                let (sig_x, sig_y) = input.signature_public.xy();

                zk_public_values.push((
                    ZKAS_BURN_NS.to_string(),
                    vec![
                        input.nullifier.inner(),
                        *value_coords.x(),
                        *value_coords.y(),
                        *token_coords.x(),
                        *token_coords.y(),
                        input.merkle_root.inner(),
                        input.user_data_enc,
                        sig_x,
                        sig_y,
                    ],
                ));

                signature_pubkeys.push(input.signature_public);
            }

            for output in &params.outputs {
                let value_coords = output.value_commit.to_affine().coordinates().unwrap();
                let token_coords = output.token_commit.to_affine().coordinates().unwrap();

                zk_public_values.push((
                    ZKAS_MINT_NS.to_string(),
                    vec![
                        output.coin.inner(),
                        *value_coords.x(),
                        *value_coords.y(),
                        *token_coords.x(),
                        *token_coords.y(),
                    ],
                ));
            }

            let mut metadata = vec![];
            zk_public_values.encode(&mut metadata)?;
            signature_pubkeys.encode(&mut metadata)?;

            // Using this, we pass the above data to the host.
            set_return_data(&metadata)?;
        }
    };

    Ok(())
}

/// This function verifies a state transition and produces an
/// update if everything is successful.
fn process_instruction(cid: ContractId, ix: &[u8]) -> ContractResult {
    let (call_idx, call): (u32, Vec<ContractCall>) = deserialize(ix)?;
    assert!(call_idx < call.len() as u32);

    let self_ = &call[call_idx as usize];

    match MoneyFunction::from(self_.data[0]) {
        MoneyFunction::Transfer => {
            let params: MoneyTransferParams = deserialize(&self_.data[1..])?;

            let info_db = db_lookup(cid, INFO_TREE)?;
            let nullifier_db = db_lookup(cid, NULLIFIERS_TREE)?;
            let coin_roots_db = db_lookup(cid, COIN_ROOTS_TREE)?;

            let Some(faucet_pubkeys) = db_get(info_db, &serialize(&FAUCET_PUBKEYS.to_string()))? else {
                msg!("[Transfer] Error: Missing faucet pubkeys from info db");
                return Err(ContractError::Internal);
            };
            let faucet_pubkeys: Vec<PublicKey> = deserialize(&faucet_pubkeys)?;

            // State transition for payments
            msg!("[Transfer] Iterating over clear inputs");
            for (i, input) in params.clear_inputs.iter().enumerate() {
                let pk = input.signature_public;

                if !faucet_pubkeys.contains(&pk) {
                    msg!("[Transfer] Error: Clear input {} has invalid faucet pubkey", i);
                    return Err(ContractError::Custom(20))
                }
            }

            let mut new_coin_roots = vec![];
            let mut new_nullifiers = vec![];

            msg!("[Transfer] Iterating over anonymous inputs");
            for (i, input) in params.inputs.iter().enumerate() {
                // The Merkle root is used to know whether this is a coin that existed
                // in a previous state.
                if new_coin_roots.contains(&input.merkle_root) ||
                    db_contains_key(coin_roots_db, &serialize(&input.merkle_root))?
                {
                    msg!("[Transfer] Error: Duplicate Merkle root found in input {}", i);
                    return Err(ContractError::Custom(21))
                }

                // The nullifiers should not already exist. It is the double-spend protection.
                if new_nullifiers.contains(&input.nullifier) ||
                    db_contains_key(nullifier_db, &serialize(&input.nullifier))?
                {
                    msg!("[Transfer] Error: Duplicate nullifier found in input {}", i);
                    return Err(ContractError::Custom(22))
                }

                new_coin_roots.push(input.merkle_root);
                new_nullifiers.push(input.nullifier);
            }

            // Newly created coins for this transaction are in the outputs.
            let new_coins = Vec::with_capacity(params.outputs.len());
            for (i, output) in params.outputs.iter().enumerate() {
                // TODO: Should we have coins in a sled tree too to check dupes?
                if new_coins.contains(&output.coin) {
                    msg!("[Transfer] Error: Duplicate coin found in output {}", i);
                    return Err(ContractError::Custom(23))
                }
            }

            // Create a state update
            let update = MoneyTransferUpdate { nullifiers: new_nullifiers, coins: new_coins };
            let mut update_data = vec![];
            update_data.write_u8(MoneyFunction::Transfer as u8)?;
            update.encode(&mut update_data)?;
            set_return_data(&update_data)?;
            msg!("[Transfer] State update set!");

            Ok(())
        }
    }
}

fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    match MoneyFunction::from(update_data[0]) {
        MoneyFunction::Transfer => {
            let update: MoneyTransferUpdate = deserialize(&update_data[1..])?;

            let info_db = db_lookup(cid, INFO_TREE)?;
            let nullifiers_db = db_lookup(cid, NULLIFIERS_TREE)?;
            let coin_roots_db = db_lookup(cid, COIN_ROOTS_TREE)?;

            for nullifier in update.nullifiers {
                db_set(nullifiers_db, &serialize(&nullifier), &[])?;
            }

            for coin in update.coins {
                // TODO: merkle_add() should take a list of coins and batch add them for efficiency
                merkle_add(
                    info_db,
                    coin_roots_db,
                    &serialize(&COIN_MERKLE_TREE.to_string()),
                    &MerkleNode::from(coin.inner()),
                )?;
            }

            Ok(())
        }
    }
}
