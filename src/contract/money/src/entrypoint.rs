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
    crypto::{ContractId, MerkleTree, PublicKey},
    db::{db_init, db_lookup, db_set, set_return_data, SMART_CONTRACT_ZKAS_DB_NAME},
    error::{ContractError, ContractResult},
    msg, ContractCall,
};
use darkfi_serial::{deserialize, serialize, Encodable, WriteExt};

use crate::{
    model::{MoneyFreezeUpdateV1, MoneyMintUpdateV1, MoneyTransferUpdateV1},
    MoneyFunction, MONEY_CONTRACT_COINS_TREE, MONEY_CONTRACT_COIN_MERKLE_TREE,
    MONEY_CONTRACT_COIN_ROOTS_TREE, MONEY_CONTRACT_DB_VERSION, MONEY_CONTRACT_FAUCET_PUBKEYS,
    MONEY_CONTRACT_INFO_TREE, MONEY_CONTRACT_NULLIFIERS_TREE, MONEY_CONTRACT_TOKEN_FREEZE_TREE,
    MONEY_CONTRACT_ZKAS_BURN_NS_V1, MONEY_CONTRACT_ZKAS_MINT_NS_V1,
    MONEY_CONTRACT_ZKAS_TOKEN_FRZ_NS_V1, MONEY_CONTRACT_ZKAS_TOKEN_MINT_NS_V1,
};

/// `Money::Transfer` functions
mod transfer_v1;
use transfer_v1::{
    money_transfer_get_metadata_v1, money_transfer_process_instruction_v1,
    money_transfer_process_update_v1,
};

/// `Money::OtcSwap` functions
mod swap_v1;
use swap_v1::{
    money_otcswap_get_metadata_v1, money_otcswap_process_instruction_v1,
    money_otcswap_process_update_v1,
};

/// `Money::Mint` functions
mod mint_v1;
use mint_v1::{
    money_mint_get_metadata_v1, money_mint_process_instruction_v1, money_mint_process_update_v1,
};

/// `Money::Freeze` functions
mod freeze_v1;
use freeze_v1::{
    money_freeze_get_metadata_v1, money_freeze_process_instruction_v1,
    money_freeze_process_update_v1,
};

darkfi_sdk::define_contract!(
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata
);

/// This entrypoint function runs when the contract is (re)deployed and initialized.
/// We use this function to initialize all the necessary databases and prepare them
/// with initial data if necessary. This is also the place where we bundle the zkas
/// circuits that are to be used with functions provided by the contract.
fn init_contract(cid: ContractId, ix: &[u8]) -> ContractResult {
    // The payload for now contains a vector of `PublicKey` used to
    // whitelist faucets that can create clear inputs.
    let faucet_pubkeys: Vec<PublicKey> = deserialize(ix)?;

    // The zkas circuit can simply be embedded in the wasm and set up by
    // the initialization. Note that the tree should then be called "zkas".
    // The lookups can be done by `contract_id+_zkas+namespace`.
    // TODO: For the zkas tree, external host checks should be done to ensure
    //       that the bincode is actually valid and not arbitrary.
    let zkas_db = match db_lookup(cid, SMART_CONTRACT_ZKAS_DB_NAME) {
        Ok(v) => v,
        Err(_) => db_init(cid, SMART_CONTRACT_ZKAS_DB_NAME)?,
    };

    let mint_v1_bincode = include_bytes!("../proof/mint_v1.zk.bin");
    let burn_v1_bincode = include_bytes!("../proof/burn_v1.zk.bin");

    let token_mint_v1_bincode = include_bytes!("../proof/token_mint_v1.zk.bin");
    let token_frz_v1_bincode = include_bytes!("../proof/token_freeze_v1.zk.bin");

    db_set(zkas_db, &serialize(&MONEY_CONTRACT_ZKAS_MINT_NS_V1), &mint_v1_bincode[..])?;
    db_set(zkas_db, &serialize(&MONEY_CONTRACT_ZKAS_BURN_NS_V1), &burn_v1_bincode[..])?;
    db_set(zkas_db, &serialize(&MONEY_CONTRACT_ZKAS_TOKEN_MINT_NS_V1), &token_mint_v1_bincode[..])?;
    db_set(zkas_db, &serialize(&MONEY_CONTRACT_ZKAS_TOKEN_FRZ_NS_V1), &token_frz_v1_bincode[..])?;

    // Set up a database tree to hold Merkle roots of all coins
    // k=MerkleNode, v=[]
    if db_lookup(cid, MONEY_CONTRACT_COIN_ROOTS_TREE).is_err() {
        db_init(cid, MONEY_CONTRACT_COIN_ROOTS_TREE)?;
    }

    // Set up a database tree to hold all coins ever seen
    // k=Coin, v=[]
    if db_lookup(cid, MONEY_CONTRACT_COINS_TREE).is_err() {
        db_init(cid, MONEY_CONTRACT_COINS_TREE)?;
    }

    // Set up a database tree to hold nullifiers of all spent coins
    // k=Nullifier, v=[]
    if db_lookup(cid, MONEY_CONTRACT_NULLIFIERS_TREE).is_err() {
        db_init(cid, MONEY_CONTRACT_NULLIFIERS_TREE)?;
    }

    // Set up a database tree to hold the set of frozen token mints
    // k=TokenId, v=[]
    if db_lookup(cid, MONEY_CONTRACT_TOKEN_FREEZE_TREE).is_err() {
        db_init(cid, MONEY_CONTRACT_TOKEN_FREEZE_TREE)?;
    }

    // Set up a database tree for arbitrary data
    let info_db = match db_lookup(cid, MONEY_CONTRACT_INFO_TREE) {
        Ok(v) => v,
        Err(_) => {
            let info_db = db_init(cid, MONEY_CONTRACT_INFO_TREE)?;

            // Create the incrementalmerkletree for seen coins
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

    // Update db version
    db_set(
        info_db,
        &serialize(&MONEY_CONTRACT_DB_VERSION),
        &serialize(&env!("CARGO_PKG_VERSION")),
    )?;

    Ok(())
}

/// This function is used by the wasm VM's host to fetch the necessary metadata
/// for verifying signatures and zk proofs. The payload given here are all the
/// contract calls in the transaction.
fn get_metadata(cid: ContractId, ix: &[u8]) -> ContractResult {
    let (call_idx, calls): (u32, Vec<ContractCall>) = deserialize(ix)?;
    if call_idx >= calls.len() as u32 {
        msg!("Error: call_idx >= calls.len()");
        return Err(ContractError::Internal)
    }

    match MoneyFunction::try_from(calls[call_idx as usize].data[0])? {
        MoneyFunction::TransferV1 => {
            // We pass everything into the correct function, and it will return
            // the metadata for us, which we can then copy into the host with
            // the `set_return_data` function. On the host, this metadata will
            // be used to do external verification (zk proofs, and signatures).
            let metadata = money_transfer_get_metadata_v1(cid, call_idx, calls)?;
            Ok(set_return_data(&metadata)?)
        }

        MoneyFunction::OtcSwapV1 => {
            let metadata = money_otcswap_get_metadata_v1(cid, call_idx, calls)?;
            Ok(set_return_data(&metadata)?)
        }

        MoneyFunction::MintV1 => {
            let metadata = money_mint_get_metadata_v1(cid, call_idx, calls)?;
            Ok(set_return_data(&metadata)?)
        }

        MoneyFunction::FreezeV1 => {
            let metadata = money_freeze_get_metadata_v1(cid, call_idx, calls)?;
            Ok(set_return_data(&metadata)?)
        }
    }
}

/// This function verifies a state transition and produces a state update
/// if everything is successful. This step should happen **after** the host
/// has successfully verified the metadata from `get_metadata()`.
fn process_instruction(cid: ContractId, ix: &[u8]) -> ContractResult {
    let (call_idx, calls): (u32, Vec<ContractCall>) = deserialize(ix)?;
    if call_idx >= calls.len() as u32 {
        msg!("Error: call_idx >= calls.len()");
        return Err(ContractError::Internal)
    }

    match MoneyFunction::try_from(calls[call_idx as usize].data[0])? {
        MoneyFunction::TransferV1 => {
            // Again, we pass everything into the correct function.
            // If it executes successfully, we'll get a state update
            // which we can copy into the host using `set_return_data`.
            // This update can then be written with `process_update()`
            // if everything is in order.
            let update_data = money_transfer_process_instruction_v1(cid, call_idx, calls)?;
            Ok(set_return_data(&update_data)?)
        }

        MoneyFunction::OtcSwapV1 => {
            let update_data = money_otcswap_process_instruction_v1(cid, call_idx, calls)?;
            Ok(set_return_data(&update_data)?)
        }

        MoneyFunction::MintV1 => {
            let update_data = money_mint_process_instruction_v1(cid, call_idx, calls)?;
            Ok(set_return_data(&update_data)?)
        }

        MoneyFunction::FreezeV1 => {
            let update_data = money_freeze_process_instruction_v1(cid, call_idx, calls)?;
            Ok(set_return_data(&update_data)?)
        }
    }
}

/// This function attempts to write a given state update provided the previous steps
/// of the contract call execution all were successful. It's the last in line, and
/// assumes that the transaction/call was successful. The payload given to the function
/// is the update data retrieved from `process_instruction()`.
fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    match MoneyFunction::try_from(update_data[0])? {
        MoneyFunction::TransferV1 => {
            let update: MoneyTransferUpdateV1 = deserialize(&update_data[1..])?;
            Ok(money_transfer_process_update_v1(cid, update)?)
        }

        MoneyFunction::OtcSwapV1 => {
            // For the atomic swaps, we use the same state update like we would
            // use for `Money::Transfer`.
            let update: MoneyTransferUpdateV1 = deserialize(&update_data[1..])?;
            Ok(money_otcswap_process_update_v1(cid, update)?)
        }

        MoneyFunction::MintV1 => {
            let update: MoneyMintUpdateV1 = deserialize(&update_data[1..])?;
            Ok(money_mint_process_update_v1(cid, update)?)
        }

        MoneyFunction::FreezeV1 => {
            let update: MoneyFreezeUpdateV1 = deserialize(&update_data[1..])?;
            Ok(money_freeze_process_update_v1(cid, update)?)
        }
    }
}
