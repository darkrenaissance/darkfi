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

#[cfg(not(feature = "no-entrypoint"))]
use darkfi_sdk::{
    crypto::{
        pedersen::{pedersen_commitment_base, pedersen_commitment_u64},
        Coin, ContractId, MerkleNode, MerkleTree, PublicKey,
    },
    db::{db_contains_key, db_get, db_init, db_lookup, db_set, SMART_CONTRACT_ZKAS_DB_NAME},
    error::ContractResult,
    merkle::merkle_add,
    msg,
    pasta::{
        arithmetic::CurveAffine,
        group::{Curve, Group},
        pallas,
    },
    tx::ContractCall,
    util::set_return_data,
};

use darkfi_sdk::error::ContractError;

#[cfg(not(feature = "no-entrypoint"))]
use darkfi_serial::{deserialize, serialize, Encodable, WriteExt};

/// Functions we allow in this contract
#[repr(u8)]
pub enum MoneyFunction {
    Transfer = 0x00,
    OtcSwap = 0x01,
    Stake = 0x02,
    Unstake = 0x03,
    Mint = 0x04,
}

impl TryFrom<u8> for MoneyFunction {
    type Error = ContractError;

    fn try_from(b: u8) -> core::result::Result<MoneyFunction, Self::Error> {
        match b {
            0x00 => Ok(Self::Transfer),
            0x01 => Ok(Self::OtcSwap),
            0x02 => Ok(Self::Stake),
            0x03 => Ok(Self::Unstake),
            0x04 => Ok(Self::Mint),
            _ => Err(ContractError::InvalidFunction),
        }
    }
}

/// Structures and object definitions
pub mod state;

#[cfg(not(feature = "no-entrypoint"))]
use state::{MoneyTransferParams, MoneyTransferUpdate};

#[cfg(feature = "client")]
/// Transaction building API for clients interacting with this contract.
pub mod client;

#[cfg(not(feature = "no-entrypoint"))]
darkfi_sdk::define_contract!(
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata
);

// These are the different sled trees that will be created
pub const MONEY_CONTRACT_COIN_ROOTS_TREE: &str = "coin_roots";
pub const MONEY_CONTRACT_NULLIFIERS_TREE: &str = "nullifiers";
pub const MONEY_CONTRACT_FIXED_SUPPLY_TREE: &str = "fixed_supply_tokens";
pub const MONEY_CONTRACT_INFO_TREE: &str = "info";

// This is a key inside the info tree
pub const MONEY_CONTRACT_COIN_MERKLE_TREE: &str = "coin_tree";
pub const MONEY_CONTRACT_FAUCET_PUBKEYS: &str = "faucet_pubkeys";

/// zkas mint contract namespace
pub const MONEY_CONTRACT_ZKAS_MINT_NS_V1: &str = "Mint_V1";
/// zkas burn contract namespace
pub const MONEY_CONTRACT_ZKAS_BURN_NS_V1: &str = "Burn_V1";
/// zkas token mint contract namespace
pub const MONEY_CONTRACT_ZKAS_TOKEN_MINT_NS_V1: &str = "TokenMint_V1";

/// zkas lead  mint contract namespace
pub const MONEY_CONTRACT_ZKAS_LEAD_MINT_NS: &str = "Lead_Mint";
/// zkas lead burn contract namespace
pub const MONEY_CONTRACT_ZKAS_LEAD_BURN_NS: &str = "Lead_Burn";

/// This function runs when the contract is (re)deployed and initialized.
#[cfg(not(feature = "no-entrypoint"))]
fn init_contract(cid: ContractId, ix: &[u8]) -> ContractResult {
    // The payload for now contains a vector of `PublicKey` used to
    // whitelist faucets that can create clear inputs.
    let faucet_pubkeys: Vec<PublicKey> = deserialize(ix)?;

    // The zkas circuits can simply be embedded in the wasm and set up by
    // the initialization. Note that the tree should then be called "zkas".
    // The lookups can then be done by `contract_id+_zkas+namespace`.
    let zkas_db = match db_lookup(cid, SMART_CONTRACT_ZKAS_DB_NAME) {
        Ok(v) => v,
        Err(_) => db_init(cid, SMART_CONTRACT_ZKAS_DB_NAME)?,
    };
    let mint_v1_bincode = include_bytes!("../proof/mint_v1.zk.bin");
    let burn_v1_bincode = include_bytes!("../proof/burn_v1.zk.bin");

    let token_mint_v1_bincode = include_bytes!("../proof/token_mint_v1.zk.bin");


    let mint_lead_bincode = include_bytes!("../proof/lead_mint.zk.bin");
    let burn_lead_bincode = include_bytes!("../proof/lead_burn.zk.bin");

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
    db_set(zkas_db, &serialize(&MONEY_CONTRACT_ZKAS_MINT_NS_V1), &mint_v1_bincode[..])?;
    db_set(zkas_db, &serialize(&MONEY_CONTRACT_ZKAS_BURN_NS_V1), &burn_v1_bincode[..])?;
    db_set(zkas_db, &serialize(&MONEY_CONTRACT_ZKAS_TOKEN_MINT_NS_V1), &token_mint_v1_bincode[..])?;

    db_set(zkas_db, &serialize(&MONEY_CONTRACT_ZKAS_LEAD_MINT_NS), &mint_lead_bincode[..])?;
    db_set(zkas_db, &serialize(&MONEY_CONTRACT_ZKAS_LEAD_BURN_NS), &burn_lead_bincode[..])?;

    // Set up a database tree to hold Merkle roots
    let _ = match db_lookup(cid, MONEY_CONTRACT_COIN_ROOTS_TREE) {
        Ok(v) => v,
        Err(_) => db_init(cid, MONEY_CONTRACT_COIN_ROOTS_TREE)?,
    };

    // Set up a database tree to hold nullifiers
    let _ = match db_lookup(cid, MONEY_CONTRACT_NULLIFIERS_TREE) {
        Ok(v) => v,
        Err(_) => db_init(cid, MONEY_CONTRACT_NULLIFIERS_TREE)?,
    };


    // Set up a database tree to hold the set of fixed-supply tokens
    let _ = match db_lookup(cid, MONEY_CONTRACT_FIXED_SUPPLY_TREE) {
        Ok(v) => v,
        Err(_) => db_init(cid, MONEY_CONTRACT_FIXED_SUPPLY_TREE)?,

    // Set up a database tree to hold lead Merkle roots
    let _ = match db_lookup(cid, MONEY_CONTRACT_LEAD_COIN_ROOTS_TREE) {
        Ok(v) => v,
        Err(_) => db_init(cid, MONEY_CONTRACT_LEAD_COIN_ROOTS_TREE)?,
    };

    // Set up a database tree to hold nullifiers
    let _ = match db_lookup(cid, MONEY_CONTRACT_LEAD_NULLIFIERS_TREE) {
        Ok(v) => v,
        Err(_) => db_init(cid, MONEY_CONTRACT_LEAD_NULLIFIERS_TREE)?,

    };

    // Set up a database tree for arbitrary data
    let info_db = match db_lookup(cid, MONEY_CONTRACT_INFO_TREE) {
        Ok(v) => v,
        Err(_) => {
            let info_db = db_init(cid, MONEY_CONTRACT_INFO_TREE)?;
            // Add a Merkle tree to the info db:
            let coin_tree = MerkleTree::new(100);
            let mut coin_tree_data = vec![];

            coin_tree_data.write_u32(0)?;
            coin_tree.encode(&mut coin_tree_data)?;

            db_set(info_db, &serialize(&MONEY_CONTRACT_COIN_MERKLE_TREE), &coin_tree_data)?;
            info_db
        }
    };

    // Whitelisted faucets
    db_set(info_db, &serialize(&MONEY_CONTRACT_FAUCET_PUBKEYS), &serialize(&faucet_pubkeys))?;

    Ok(())
}

/// This function is used by the VM's host to fetch the necessary metadata for
/// verifying signatures and zk proofs.
#[cfg(not(feature = "no-entrypoint"))]
fn get_metadata(_cid: ContractId, ix: &[u8]) -> ContractResult {
    let (call_idx, call): (u32, Vec<ContractCall>) = deserialize(ix)?;
    assert!(call_idx < call.len() as u32);

    let self_ = &call[call_idx as usize];

    match MoneyFunction::try_from(self_.data[0])? {
        MoneyFunction::Transfer | MoneyFunction::OtcSwap => {
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
                    MONEY_CONTRACT_ZKAS_BURN_NS_V1.to_string(),
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
                    MONEY_CONTRACT_ZKAS_MINT_NS_V1.to_string(),
                    vec![
                        //output.coin.inner(),
                        output.coin,
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

        MoneyFunction::Stake => {
            let params: MoneyStakeParams = deserialize(&self_.data[1..])?;

            let mut zk_public_values: Vec<(String, Vec<pallas::Base>)> = vec![];
            let mut signature_pubkeys: Vec<PublicKey> = vec![];

            for input in &params.inputs {
                let value_coords = input.value_commit.to_affine().coordinates().unwrap();
                let token_coords = input.token_commit.to_affine().coordinates().unwrap();
                let (sig_x, sig_y) = input.signature_public.xy();

                zk_public_values.push((
                    MONEY_CONTRACT_ZKAS_BURN_NS_V1.to_string(),
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

                zk_public_values.push((
                    MONEY_CONTRACT_ZKAS_LEAD_MINT_NS.to_string(),
                    vec![
                        *value_coords.x(),
                        *value_coords.y(),
                        output.coin_pk_hash,
                        output.coin,
                    ],
                ));
            }

            let mut metadata = vec![];
            zk_public_values.encode(&mut metadata)?;
            signature_pubkeys.encode(&mut metadata)?;

            // Using this, we pass the above data to the host.
            set_return_data(&metadata)?;
        }

        MoneyFunction::Unstake => unimplemented!(),
        MoneyFunction::Mint => unimplemented!(),
    };

    Ok(())
}

/// This function verifies a state transition and produces an
/// update if everything is successful.
#[cfg(not(feature = "no-entrypoint"))]
fn process_instruction(cid: ContractId, ix: &[u8]) -> ContractResult {
    let (call_idx, call): (u32, Vec<ContractCall>) = deserialize(ix)?;
    assert!(call_idx < call.len() as u32);

    let self_ = &call[call_idx as usize];

    match MoneyFunction::try_from(self_.data[0])? {
        MoneyFunction::Transfer => {
            msg!("[Transfer] Entered match arm");
            let params: MoneyTransferParams = deserialize(&self_.data[1..])?;

            assert!(params.clear_inputs.len() + params.inputs.len() > 0);
            assert!(!params.outputs.is_empty());

            let info_db = db_lookup(cid, MONEY_CONTRACT_INFO_TREE)?;
            let nullifiers_db = db_lookup(cid, MONEY_CONTRACT_NULLIFIERS_TREE)?;
            let coin_roots_db = db_lookup(cid, MONEY_CONTRACT_COIN_ROOTS_TREE)?;

            let Some(faucet_pubkeys) = db_get(info_db, &serialize(&MONEY_CONTRACT_FAUCET_PUBKEYS))? else {
                msg!("[Transfer] Error: Missing faucet pubkeys from info db");
                return Err(ContractError::Internal);
            };
            let faucet_pubkeys: Vec<PublicKey> = deserialize(&faucet_pubkeys)?;

            // Accumulator for the value commitments
            let mut valcom_total = pallas::Point::identity();

            // State transition for payments
            msg!("[Transfer] Iterating over clear inputs");
            for (i, input) in params.clear_inputs.iter().enumerate() {
                let pk = input.signature_public;

                if !faucet_pubkeys.contains(&pk) {
                    msg!("[Transfer] Error: Clear input {} has invalid faucet pubkey", i);
                    return Err(ContractError::Custom(20))
                }

                valcom_total += pedersen_commitment_u64(input.value, input.value_blind);
            }

            let mut new_nullifiers = Vec::with_capacity(params.inputs.len());

            msg!("[Transfer] Iterating over anonymous inputs");
            for (i, input) in params.inputs.iter().enumerate() {
                // The Merkle root is used to know whether this is a coin that existed
                // in a previous state.
                if !db_contains_key(coin_roots_db, &serialize(&input.merkle_root))? {
                    msg!("[Transfer] Error: Merkle root not found in previous state (input {})", i);
                    return Err(ContractError::Custom(21))
                }

                // The nullifiers should not already exist. It is the double-spend protection.
                if new_nullifiers.contains(&input.nullifier) ||
                    db_contains_key(nullifiers_db, &serialize(&input.nullifier))?
                {
                    msg!("[Transfer] Error: Duplicate nullifier found in input {}", i);
                    return Err(ContractError::Custom(22))
                }

                new_nullifiers.push(input.nullifier);
                valcom_total += input.value_commit;
            }

            // Newly created coins for this transaction are in the outputs.
            let mut new_coins = Vec::with_capacity(params.outputs.len());
            for (i, output) in params.outputs.iter().enumerate() {
                // TODO: Should we have coins in a sled tree too to check dupes?
                if new_coins.contains(&Coin::from(output.coin)) {
                    msg!("[Transfer] Error: Duplicate coin found in output {}", i);
                    return Err(ContractError::Custom(23))
                }

                // FIXME: Needs some work on types and their place within all these libraries
                new_coins.push(Coin::from(output.coin));
                valcom_total -= output.value_commit;
            }

            // If the accumulator is not back in its initial state, there's a value mismatch.
            if valcom_total != pallas::Point::identity() {
                msg!("[Transfer] Error: Value commitments do not result in identity");
                return Err(ContractError::Custom(24))
            }

            // Verify that the token commitments are all for the same token
            let tokcom = params.outputs[0].token_commit;
            let mut failed_tokcom = params.inputs.iter().any(|input| input.token_commit != tokcom);

            failed_tokcom =
                failed_tokcom || params.outputs.iter().any(|output| output.token_commit != tokcom);

            failed_tokcom = failed_tokcom ||
                params.clear_inputs.iter().any(|input| {
                    pedersen_commitment_base(input.token_id.inner(), input.token_blind) != tokcom
                });

            if failed_tokcom {
                msg!("[Transfer] Error: Token commitments do not match");
                return Err(ContractError::Custom(25))
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

        MoneyFunction::OtcSwap => {
            msg!("[OtcSwap] Entered match arm");
            let params: MoneyTransferParams = deserialize(&self_.data[1..])?;

            let nullifiers_db = db_lookup(cid, MONEY_CONTRACT_NULLIFIERS_TREE)?;
            let coin_roots_db = db_lookup(cid, MONEY_CONTRACT_COIN_ROOTS_TREE)?;

            // State transition for OTC swaps
            // For now we enforce 2 inputs and 2 outputs, which means the coins
            // must be available beforehand. We might want to change this and
            // allow transactions including leftover change.
            assert!(params.clear_inputs.is_empty());
            assert!(params.inputs.len() == 2);
            assert!(params.outputs.len() == 2);

            let mut new_nullifiers = Vec::with_capacity(params.inputs.len());

            // inputs[0] is being swapped to outputs[1]
            // inputs[1] is being swapped to outputs[0]
            // So that's how we check the value and token commitments
            if params.inputs[0].value_commit != params.outputs[1].value_commit {
                msg!("[OtcSwap] Error: Value commitments for input 0 and output 1 do not match");
                return Err(ContractError::Custom(24))
            }

            if params.inputs[1].value_commit != params.outputs[0].value_commit {
                msg!("[OtcSwap] Error: Value commitments for input 1 and output 0 do not match");
                return Err(ContractError::Custom(24))
            }

            if params.inputs[0].token_commit != params.outputs[1].token_commit {
                msg!("[OtcSwap] Error: Token commitments for input 0 and output 1 do not match");
                return Err(ContractError::Custom(25))
            }

            if params.inputs[1].token_commit != params.outputs[0].token_commit {
                msg!("[OtcSwap] Error: Token commitments for input 1 and output 0 do not match");
                return Err(ContractError::Custom(25))
            }

            msg!("[OtcSwap] Iterating over anonymous inputs");
            for (i, input) in params.inputs.iter().enumerate() {
                // The Merkle root is used to know whether this is a coin that
                // existed in a previous state.
                if !db_contains_key(coin_roots_db, &serialize(&input.merkle_root))? {
                    msg!("[OtcSwap] Error: Merkle root not found in previous state (input {})", i);
                    return Err(ContractError::Custom(21))
                }

                // The nullifiers should not already exist. It is the double-spend protection.
                if new_nullifiers.contains(&input.nullifier) ||
                    db_contains_key(nullifiers_db, &serialize(&input.nullifier))?
                {
                    msg!("[OtcSwap] Error: Duplicate nullifier found in input {}", i);
                    return Err(ContractError::Custom(22))
                }

                new_nullifiers.push(input.nullifier);
            }

            // Newly created coins for this transaction are in the outputs.
            let mut new_coins = Vec::with_capacity(params.outputs.len());
            for (i, output) in params.outputs.iter().enumerate() {
                // TODO: Should we have coins in a sled tree too to check dupes?
                if new_coins.contains(&Coin::from(output.coin)) {
                    msg!("[OtcSwap] Error: Duplicate coin found in output {}", i);
                    return Err(ContractError::Custom(23))
                }

                // FIXME: Needs some work on types and their place within all these libraries
                new_coins.push(Coin::from(output.coin));
            }

            // Create a state update. We also use the MoneyTransferUpdate because they're
            // essentially the same thing just with a different transition ruleset.
            let update = MoneyTransferUpdate { nullifiers: new_nullifiers, coins: new_coins };
            let mut update_data = vec![];
            update_data.write_u8(MoneyFunction::OtcSwap as u8)?;
            update.encode(&mut update_data)?;
            set_return_data(&update_data)?;
            msg!("[OtcSwap] State update set!");

            Ok(())
        }

        MoneyFunction::Stake => {
            msg!("[Stake] Entered match arm");
            let params: MoneyStakeParams = deserialize(&self_.data[1..])?;

            assert!(params.inputs.len() == params.outputs.len());

            let info_db = db_lookup(cid, MONEY_CONTRACT_INFO_TREE)?;
            let nullifiers_db = db_lookup(cid, MONEY_CONTRACT_LEAD_NULLIFIERS_TREE)?;
            let coin_roots_db = db_lookup(cid, MONEY_CONTRACT_LEAD_COIN_ROOTS_TREE)?;


            // Accumulator for the value commitments
            let mut valcom_total = pallas::Point::identity();

            // State transition for payments
            let mut new_nullifiers = Vec::with_capacity(params.inputs.len());

            msg!("[Stake] Iterating over anonymous inputs");
            for (i, input) in params.inputs.iter().enumerate() {
                // The Merkle root is used to know whether this is a coin that existed
                // in a previous state.
                if !db_contains_key(coin_roots_db, &serialize(&input.merkle_root))? {
                    msg!("[Stake] Error: Merkle root not found in previous state (input {})", i);
                    return Err(ContractError::Custom(21))
                }

                // The nullifiers should not already exist. It is the double-spend protection.
                if new_nullifiers.contains(&input.nullifier) ||
                    db_contains_key(nullifiers_db, &serialize(&input.nullifier))?
                {
                    msg!("[Stake] Error: Duplicate nullifier found in input {}", i);
                    return Err(ContractError::Custom(22))
                }

                new_nullifiers.push(input.nullifier);
                valcom_total += input.value_commit;
            }

            // Newly created coins for this transaction are in the outputs.
            let mut new_coins = Vec::with_capacity(params.outputs.len());
            for (i, output) in params.outputs.iter().enumerate() {
                // TODO: Should we have coins in a sled tree too to check dupes?
                if new_coins.contains(&Coin::from(output.coin_commit_hash)) {
                    msg!("[Stake] Error: Duplicate coin found in output {}", i);
                    return Err(ContractError::Custom(23))
                }
                new_coins.push(Coin::from(output.coin));
                valcom_total -= output.value_commit;
            }

            // If the accumulator is not back in its initial state, there's a value mismatch.
            if valcom_total != pallas::Point::identity() {
                msg!("[Stake] Error: Value commitments do not result in identity");
                return Err(ContractError::Custom(24))
            }

            // Create a state update
            let update = MoneyStakeUpdate { nullifiers: new_nullifiers, coins: new_coins };
            let mut update_data = vec![];
            update_data.write_u8(MoneyFunction::Stake as u8)?;
            update.encode(&mut update_data)?;
            set_return_data(&update_data)?;
            msg!("[Stake] State update set!");

            Ok(())
        }

        MoneyFunction::Unstake => {
            msg!("[Unstake] Entered match arm");
            unimplemented!();
        }

        MoneyFunction::Mint => {
            msg!("[Mint] Entered match arm");
            unimplemented!();
        }
    }
}

#[cfg(not(feature = "no-entrypoint"))]
fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    match MoneyFunction::try_from(update_data[0])? {
        MoneyFunction::Transfer | MoneyFunction::OtcSwap => {
            let update: MoneyTransferUpdate = deserialize(&update_data[1..])?;

            let info_db = db_lookup(cid, MONEY_CONTRACT_INFO_TREE)?;
            let nullifiers_db = db_lookup(cid, MONEY_CONTRACT_NULLIFIERS_TREE)?;
            let coin_roots_db = db_lookup(cid, MONEY_CONTRACT_COIN_ROOTS_TREE)?;

            for nullifier in update.nullifiers {
                db_set(nullifiers_db, &serialize(&nullifier), &[])?;
            }

            msg!("Adding coins {:?} to Merkle tree", update.coins);
            let coins: Vec<_> = update.coins.iter().map(|x| MerkleNode::from(x.inner())).collect();
            merkle_add(
                info_db,
                coin_roots_db,
                &serialize(&MONEY_CONTRACT_COIN_MERKLE_TREE),
                &coins,
            )?;

            Ok(())
        }

        MoneyFunction::Stake => {
            let update: MoneyStakeUpdate = deserialize(&update_data[1..])?;

            let info_db = db_lookup(cid, MONEY_CONTRACT_LEAD_INFO_TREE)?;
            let nullifiers_db = db_lookup(cid, MONEY_CONTRACT_LEAD_NULLIFIERS_TREE)?;
            let coin_roots_db = db_lookup(cid, MONEY_CONTRACT_LEAD_COIN_ROOTS_TREE)?;

            for nullifier in update.nullifiers {
                db_set(nullifiers_db, &serialize(&nullifier), &[])?;
            }

            msg!("Adding coins {:?} to Merkle tree", update.coins);
            let coins: Vec<_> = update.coins.iter().map(|x| MerkleNode::from(x.inner())).collect();
            merkle_add(
                info_db,
                coin_roots_db,
                &serialize(&MONEY_CONTRACT_LEAD_COIN_MERKLE_TREE),
                &coins,
            )?;

            Ok(())
        }
        MoneyFunction::Unstake => unimplemented!(),
        MoneyFunction::Mint => unimplemented!(),
    }
}
