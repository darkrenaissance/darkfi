/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
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
    crypto::{pasta_prelude::Field, smt::EMPTY_NODES_FP, ContractId, MerkleNode, MerkleTree},
    dark_tree::DarkLeaf,
    error::ContractResult,
    msg,
    pasta::pallas,
    wasm, ContractCall,
};
use darkfi_serial::{deserialize, serialize, Encodable, WriteExt};

use crate::{
    error::MoneyError,
    model::{
        MoneyAuthTokenFreezeUpdateV1, MoneyAuthTokenMintUpdateV1, MoneyFeeUpdateV1,
        MoneyGenesisMintUpdateV1, MoneyPoWRewardUpdateV1, MoneyTokenMintUpdateV1,
        MoneyTransferUpdateV1,
    },
    MoneyFunction, EMPTY_COINS_TREE_ROOT, MONEY_CONTRACT_COINS_TREE,
    MONEY_CONTRACT_COIN_MERKLE_TREE, MONEY_CONTRACT_COIN_ROOTS_TREE, MONEY_CONTRACT_DB_VERSION,
    MONEY_CONTRACT_FEES_TREE, MONEY_CONTRACT_INFO_TREE, MONEY_CONTRACT_LATEST_COIN_ROOT,
    MONEY_CONTRACT_LATEST_NULLIFIER_ROOT, MONEY_CONTRACT_NULLIFIERS_TREE,
    MONEY_CONTRACT_NULLIFIER_ROOTS_TREE, MONEY_CONTRACT_TOKEN_FREEZE_TREE,
};

/// `Money::Fee` functions
mod fee_v1;
use fee_v1::{
    money_fee_get_metadata_v1, money_fee_process_instruction_v1, money_fee_process_update_v1,
};

/// `Money::GenesisMint` functions
mod genesis_mint_v1;
use genesis_mint_v1::{
    money_genesis_mint_get_metadata_v1, money_genesis_mint_process_instruction_v1,
    money_genesis_mint_process_update_v1,
};

/// `Money::PoWReward` functions
mod pow_reward_v1;
use pow_reward_v1::{
    money_pow_reward_get_metadata_v1, money_pow_reward_process_instruction_v1,
    money_pow_reward_process_update_v1,
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

/// `Money::AuthTokenMint` functions
mod auth_token_mint_v1;
use auth_token_mint_v1::{
    money_auth_token_mint_get_metadata_v1, money_auth_token_mint_process_instruction_v1,
    money_auth_token_mint_process_update_v1,
};

/// `Money::AuthTokenFreeze` functions
mod auth_token_freeze_v1;
use auth_token_freeze_v1::{
    money_auth_token_freeze_get_metadata_v1, money_auth_token_freeze_process_instruction_v1,
    money_auth_token_freeze_process_update_v1,
};

/// `Money::TokenMint` functions
mod token_mint_v1;
use token_mint_v1::{
    money_token_mint_get_metadata_v1, money_token_mint_process_instruction_v1,
    money_token_mint_process_update_v1,
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
fn init_contract(cid: ContractId, _ix: &[u8]) -> ContractResult {
    // zkas circuits can simply be embedded in the wasm and set up by using
    // respective db functions. The special `zkas db` operations exist in
    // order to be able to verify the circuits being bundled and enforcing
    // a specific tree inside sled, and also creation of VerifyingKey.
    let fee_v1_bincode = include_bytes!("../proof/fee_v1.zk.bin");
    let mint_v1_bincode = include_bytes!("../proof/mint_v1.zk.bin");
    let burn_v1_bincode = include_bytes!("../proof/burn_v1.zk.bin");
    let token_mint_v1_bincode = include_bytes!("../proof/token_mint_v1.zk.bin");
    let auth_token_mint_v1_bincode = include_bytes!("../proof/auth_token_mint_v1.zk.bin");

    // For that, we use `wasm::db::zkas_wasm::db::db_set` and pass in the bincode.
    wasm::db::zkas_db_set(&fee_v1_bincode[..])?;
    wasm::db::zkas_db_set(&mint_v1_bincode[..])?;
    wasm::db::zkas_db_set(&burn_v1_bincode[..])?;
    wasm::db::zkas_db_set(&token_mint_v1_bincode[..])?;
    wasm::db::zkas_db_set(&auth_token_mint_v1_bincode[..])?;

    let tx_hash = wasm::util::get_tx_hash()?;
    // The max outputs for a tx in BTC is 2501
    let call_idx = wasm::util::get_call_index()?;
    let mut roots_value_data = Vec::with_capacity(32 + 1);
    tx_hash.encode(&mut roots_value_data)?;
    call_idx.encode(&mut roots_value_data)?;
    if roots_value_data.len() != 32 + 1 {
        msg!(
            "[money::init_contract] Error: Roots value data length is not expected(32 + 1): {}",
            roots_value_data.len()
        );
        return Err(MoneyError::RootsValueDataMismatch.into())
    }

    // Set up a database tree to hold Merkle roots of all coin trees
    // k=root_hash:32, v=(tx_hash:32, call_idx: 1)
    if wasm::db::db_lookup(cid, MONEY_CONTRACT_COIN_ROOTS_TREE).is_err() {
        let db_coin_roots = wasm::db::db_init(cid, MONEY_CONTRACT_COIN_ROOTS_TREE)?;
        wasm::db::db_set(db_coin_roots, &serialize(&EMPTY_COINS_TREE_ROOT), &roots_value_data)?;
    }

    // Set up a database tree to hold Merkle roots of all nullifier trees
    // k=root_hash:32, v=[(tx_hash:32, call_idx: 1)]
    if wasm::db::db_lookup(cid, MONEY_CONTRACT_NULLIFIER_ROOTS_TREE).is_err() {
        let db_null_roots = wasm::db::db_init(cid, MONEY_CONTRACT_NULLIFIER_ROOTS_TREE)?;
        wasm::db::db_set(
            db_null_roots,
            &serialize(&EMPTY_NODES_FP[0]),
            &serialize(&vec![roots_value_data]),
        )?;
    }

    // Set up a database tree to hold all coins ever seen
    // k=Coin, v=[]
    if wasm::db::db_lookup(cid, MONEY_CONTRACT_COINS_TREE).is_err() {
        wasm::db::db_init(cid, MONEY_CONTRACT_COINS_TREE)?;
    }

    // Set up a database tree to hold nullifiers of all spent coins
    // k=Nullifier, v=[]
    if wasm::db::db_lookup(cid, MONEY_CONTRACT_NULLIFIERS_TREE).is_err() {
        wasm::db::db_init(cid, MONEY_CONTRACT_NULLIFIERS_TREE)?;
    }

    // Set up a database tree to hold the set of frozen token mints
    // k=TokenId, v=[]
    if wasm::db::db_lookup(cid, MONEY_CONTRACT_TOKEN_FREEZE_TREE).is_err() {
        wasm::db::db_init(cid, MONEY_CONTRACT_TOKEN_FREEZE_TREE)?;
    }

    // Set up a database tree to hold the fees paid for each block
    // k=height_bytes, v=fees_paid_bytes
    if wasm::db::db_lookup(cid, MONEY_CONTRACT_FEES_TREE).is_err() {
        let fees_db = wasm::db::db_init(cid, MONEY_CONTRACT_FEES_TREE)?;
        // Initialize the first two accumulators
        wasm::db::db_set(fees_db, &serialize(&0_u32), &serialize(&0_u64))?;
        wasm::db::db_set(fees_db, &serialize(&1_u32), &serialize(&0_u64))?;
    }

    // Set up a database tree for arbitrary data
    let info_db = match wasm::db::db_lookup(cid, MONEY_CONTRACT_INFO_TREE) {
        Ok(v) => v,
        Err(_) => {
            let info_db = wasm::db::db_init(cid, MONEY_CONTRACT_INFO_TREE)?;

            // Create the incrementalmerkletree for seen coins and initialize
            // it with a "fake" coin that can be used for dummy inputs.
            let mut coin_tree = MerkleTree::new(1);
            coin_tree.append(MerkleNode::from(pallas::Base::ZERO));
            let mut coin_tree_data = vec![];
            coin_tree_data.write_u32(0)?;
            coin_tree.encode(&mut coin_tree_data)?;
            wasm::db::db_set(info_db, MONEY_CONTRACT_COIN_MERKLE_TREE, &coin_tree_data)?;

            // Initialize coins and nulls latest root field
            // This will result in exhausted gas so we use a precalculated value:
            //let root = coin_tree.root(0).unwrap();
            wasm::db::db_set(
                info_db,
                MONEY_CONTRACT_LATEST_COIN_ROOT,
                &serialize(&EMPTY_COINS_TREE_ROOT),
            )?;
            wasm::db::db_set(
                info_db,
                MONEY_CONTRACT_LATEST_NULLIFIER_ROOT,
                &serialize(&EMPTY_NODES_FP[0]),
            )?;

            info_db
        }
    };

    // Update db version
    wasm::db::db_set(info_db, MONEY_CONTRACT_DB_VERSION, &serialize(&env!("CARGO_PKG_VERSION")))?;

    Ok(())
}

/// This function is used by the wasm VM's host to fetch the necessary metadata
/// for verifying signatures and zk proofs. The payload given here are all the
/// contract calls in the transaction.
fn get_metadata(cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = MoneyFunction::try_from(self_.data[0])?;

    let metadata = match func {
        MoneyFunction::FeeV1 => {
            // We pass everything into the correct function, and it will return
            // the metadata for us, which we can then copy into the host with
            // the `wasm::util::set_return_data` function. On the host, this metadata will
            // be used to do external verification (zk proofs, and signatures).
            money_fee_get_metadata_v1(cid, call_idx, calls)?
        }
        MoneyFunction::GenesisMintV1 => money_genesis_mint_get_metadata_v1(cid, call_idx, calls)?,
        MoneyFunction::PoWRewardV1 => money_pow_reward_get_metadata_v1(cid, call_idx, calls)?,
        MoneyFunction::TransferV1 => money_transfer_get_metadata_v1(cid, call_idx, calls)?,
        MoneyFunction::OtcSwapV1 => money_otcswap_get_metadata_v1(cid, call_idx, calls)?,
        MoneyFunction::AuthTokenMintV1 => {
            money_auth_token_mint_get_metadata_v1(cid, call_idx, calls)?
        }
        MoneyFunction::AuthTokenFreezeV1 => {
            money_auth_token_freeze_get_metadata_v1(cid, call_idx, calls)?
        }
        MoneyFunction::TokenMintV1 => money_token_mint_get_metadata_v1(cid, call_idx, calls)?,
    };

    wasm::util::set_return_data(&metadata)
}

/// This function verifies a state transition and produces a state update
/// if everything is successful. This step should happen **after** the host
/// has successfully verified the metadata from `get_metadata()`.
fn process_instruction(cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = MoneyFunction::try_from(self_.data[0])?;

    let update_data = match func {
        MoneyFunction::FeeV1 => {
            // Again, we pass everything into the correct function.
            // If it executes successfully, we'll get a state update
            // which we can copy into the host using `wasm::util::set_return_data`.
            // This update can then be written with `process_update()`
            // if everything is in order.
            money_fee_process_instruction_v1(cid, call_idx, calls)?
        }
        MoneyFunction::GenesisMintV1 => {
            money_genesis_mint_process_instruction_v1(cid, call_idx, calls)?
        }
        MoneyFunction::PoWRewardV1 => {
            money_pow_reward_process_instruction_v1(cid, call_idx, calls)?
        }
        MoneyFunction::TransferV1 => money_transfer_process_instruction_v1(cid, call_idx, calls)?,
        MoneyFunction::OtcSwapV1 => money_otcswap_process_instruction_v1(cid, call_idx, calls)?,
        MoneyFunction::AuthTokenMintV1 => {
            money_auth_token_mint_process_instruction_v1(cid, call_idx, calls)?
        }
        MoneyFunction::AuthTokenFreezeV1 => {
            money_auth_token_freeze_process_instruction_v1(cid, call_idx, calls)?
        }
        MoneyFunction::TokenMintV1 => {
            money_token_mint_process_instruction_v1(cid, call_idx, calls)?
        }
    };

    wasm::util::set_return_data(&update_data)
}

/// This function attempts to write a given state update provided the previous steps
/// of the contract call execution all were successful. It's the last in line, and
/// assumes that the transaction/call was successful. The payload given to the function
/// is the update data retrieved from `process_instruction()`, prefixed with the
/// contract function.
fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    match MoneyFunction::try_from(update_data[0])? {
        MoneyFunction::FeeV1 => {
            let update: MoneyFeeUpdateV1 = deserialize(&update_data[1..])?;
            Ok(money_fee_process_update_v1(cid, update)?)
        }

        MoneyFunction::GenesisMintV1 => {
            let update: MoneyGenesisMintUpdateV1 = deserialize(&update_data[1..])?;
            Ok(money_genesis_mint_process_update_v1(cid, update)?)
        }

        MoneyFunction::PoWRewardV1 => {
            let update: MoneyPoWRewardUpdateV1 = deserialize(&update_data[1..])?;
            Ok(money_pow_reward_process_update_v1(cid, update)?)
        }

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

        MoneyFunction::AuthTokenMintV1 => {
            let update: MoneyAuthTokenMintUpdateV1 = deserialize(&update_data[1..])?;
            Ok(money_auth_token_mint_process_update_v1(cid, update)?)
        }

        MoneyFunction::AuthTokenFreezeV1 => {
            let update: MoneyAuthTokenFreezeUpdateV1 = deserialize(&update_data[1..])?;
            Ok(money_auth_token_freeze_process_update_v1(cid, update)?)
        }

        MoneyFunction::TokenMintV1 => {
            let update: MoneyTokenMintUpdateV1 = deserialize(&update_data[1..])?;
            Ok(money_token_mint_process_update_v1(cid, update)?)
        }
    }
}
