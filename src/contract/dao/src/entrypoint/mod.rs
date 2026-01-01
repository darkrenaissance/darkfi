/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

use std::io::Cursor;

use darkfi_sdk::{
    crypto::{ContractId, MerkleTree},
    dark_tree::DarkLeaf,
    error::ContractResult,
    wasm, ContractCall,
};
use darkfi_serial::{deserialize, serialize, Decodable, Encodable, WriteExt};

use crate::{
    model::{DaoExecUpdate, DaoMintUpdate, DaoProposeUpdate, DaoVoteUpdate},
    DaoFunction, DAO_CONTRACT_BULLAS_TREE, DAO_CONTRACT_DB_VERSION, DAO_CONTRACT_INFO_TREE,
    DAO_CONTRACT_MERKLE_ROOTS_TREE, DAO_CONTRACT_MERKLE_TREE, DAO_CONTRACT_PROPOSAL_BULLAS_TREE,
    DAO_CONTRACT_VOTE_NULLIFIERS_TREE,
};

/// `Dao::Mint` functions
mod mint;
use mint::{dao_mint_get_metadata, dao_mint_process_instruction, dao_mint_process_update};

/// `Dao::Propose` functions
mod propose;
use propose::{
    dao_propose_get_metadata, dao_propose_process_instruction, dao_propose_process_update,
};

/// `Dao::Vote` functions
mod vote;
use vote::{dao_vote_get_metadata, dao_vote_process_instruction, dao_vote_process_update};

/// `Dao::Exec` functions
mod exec;
use exec::{dao_exec_get_metadata, dao_exec_process_instruction, dao_exec_process_update};

mod auth_xfer;
use auth_xfer::{dao_authxfer_get_metadata, dao_authxfer_process_instruction};

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
    // The zkas circuits can simply be embedded in the wasm and set up by
    // the initialization.
    wasm::db::zkas_db_set(&include_bytes!("../../proof/mint.zk.bin")[..])?;
    wasm::db::zkas_db_set(&include_bytes!("../../proof/propose-input.zk.bin")[..])?;
    wasm::db::zkas_db_set(&include_bytes!("../../proof/propose-main.zk.bin")[..])?;
    wasm::db::zkas_db_set(&include_bytes!("../../proof/vote-input.zk.bin")[..])?;
    wasm::db::zkas_db_set(&include_bytes!("../../proof/vote-main.zk.bin")[..])?;
    wasm::db::zkas_db_set(&include_bytes!("../../proof/exec.zk.bin")[..])?;
    wasm::db::zkas_db_set(&include_bytes!("../../proof/early-exec.zk.bin")[..])?;
    wasm::db::zkas_db_set(&include_bytes!("../../proof/auth-money-transfer.zk.bin")[..])?;
    wasm::db::zkas_db_set(&include_bytes!("../../proof/auth-money-transfer-enc-coin.zk.bin")[..])?;

    // Set up db for general info
    let dao_info_db = match wasm::db::db_lookup(cid, DAO_CONTRACT_INFO_TREE) {
        Ok(v) => v,
        Err(_) => wasm::db::db_init(cid, DAO_CONTRACT_INFO_TREE)?,
    };

    // Set up the entries in the header table
    match wasm::db::db_get(dao_info_db, DAO_CONTRACT_MERKLE_TREE)? {
        Some(bytes) => {
            // We found some bytes, try to deserialize into a tree.
            // For now, if this doesn't work, we bail.
            let mut decoder = Cursor::new(&bytes);
            <u32 as Decodable>::decode(&mut decoder)?;
            <MerkleTree as Decodable>::decode(&mut decoder)?;
        }
        None => {
            // We didn't find a tree, so just make a new one.
            let tree = MerkleTree::new(1);

            let mut tree_data = vec![];
            tree_data.write_u32(0)?;
            tree.encode(&mut tree_data)?;

            wasm::db::db_set(dao_info_db, DAO_CONTRACT_MERKLE_TREE, &tree_data)?;
        }
    }

    // Set up db to avoid double creating DAOs
    let _ = match wasm::db::db_lookup(cid, DAO_CONTRACT_BULLAS_TREE) {
        Ok(v) => v,
        Err(_) => wasm::db::db_init(cid, DAO_CONTRACT_BULLAS_TREE)?,
    };

    // Set up db for DAO bulla Merkle roots
    let _ = match wasm::db::db_lookup(cid, DAO_CONTRACT_MERKLE_ROOTS_TREE) {
        Ok(v) => v,
        Err(_) => wasm::db::db_init(cid, DAO_CONTRACT_MERKLE_ROOTS_TREE)?,
    };

    // Set up db for proposal votes
    // k: ProposalBulla
    // v: (BlindAggregateVote, bool) (the bool marks if the proposal is finished)
    let _ = match wasm::db::db_lookup(cid, DAO_CONTRACT_PROPOSAL_BULLAS_TREE) {
        Ok(v) => v,
        Err(_) => wasm::db::db_init(cid, DAO_CONTRACT_PROPOSAL_BULLAS_TREE)?,
    };

    // TODO: These nullifiers should exist per-proposal
    let _ = match wasm::db::db_lookup(cid, DAO_CONTRACT_VOTE_NULLIFIERS_TREE) {
        Ok(v) => v,
        Err(_) => wasm::db::db_init(cid, DAO_CONTRACT_VOTE_NULLIFIERS_TREE)?,
    };

    // Update db version
    wasm::db::db_set(dao_info_db, DAO_CONTRACT_DB_VERSION, &serialize(&env!("CARGO_PKG_VERSION")))?;

    Ok(())
}

/// This function is used by the wasm VM's host to fetch the necessary metadata
/// for verifying signatures and ZK proofs. The payload given here are all the
/// contract calls in the transaction.
fn get_metadata(cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = DaoFunction::try_from(self_.data[0])?;

    let metadata = match func {
        DaoFunction::Mint => dao_mint_get_metadata(cid, call_idx, calls)?,
        DaoFunction::Propose => dao_propose_get_metadata(cid, call_idx, calls)?,
        DaoFunction::Vote => dao_vote_get_metadata(cid, call_idx, calls)?,
        DaoFunction::Exec => dao_exec_get_metadata(cid, call_idx, calls)?,
        DaoFunction::AuthMoneyTransfer => dao_authxfer_get_metadata(cid, call_idx, calls)?,
    };

    wasm::util::set_return_data(&metadata)
}

/// This function verifies a state transition and produces a state update
/// if everything is successful.
fn process_instruction(cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = DaoFunction::try_from(self_.data[0])?;

    let update_data = match func {
        DaoFunction::Mint => dao_mint_process_instruction(cid, call_idx, calls)?,
        DaoFunction::Propose => dao_propose_process_instruction(cid, call_idx, calls)?,
        DaoFunction::Vote => dao_vote_process_instruction(cid, call_idx, calls)?,
        DaoFunction::Exec => dao_exec_process_instruction(cid, call_idx, calls)?,
        DaoFunction::AuthMoneyTransfer => dao_authxfer_process_instruction(cid, call_idx, calls)?,
    };

    wasm::util::set_return_data(&update_data)
}

/// This function attempts to write a given state update provided the previous
/// steps of the contract call execution were successful. The payload given to
/// the function is the update data retrieved from `process_instruction()`,
/// prefixed with the contract function.
fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    match DaoFunction::try_from(update_data[0])? {
        DaoFunction::Mint => {
            let update: DaoMintUpdate = deserialize(&update_data[1..])?;
            Ok(dao_mint_process_update(cid, update)?)
        }

        DaoFunction::Propose => {
            let update: DaoProposeUpdate = deserialize(&update_data[1..])?;
            Ok(dao_propose_process_update(cid, update)?)
        }

        DaoFunction::Vote => {
            let update: DaoVoteUpdate = deserialize(&update_data[1..])?;
            Ok(dao_vote_process_update(cid, update)?)
        }

        DaoFunction::Exec => {
            let update: DaoExecUpdate = deserialize(&update_data[1..])?;
            Ok(dao_exec_process_update(cid, update)?)
        }

        DaoFunction::AuthMoneyTransfer => {
            // Does nothing, just verifies the other calls are correct
            Ok(())
        }
    }
}
