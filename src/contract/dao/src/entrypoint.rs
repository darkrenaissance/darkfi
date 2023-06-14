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

use std::io::Cursor;

use darkfi_sdk::{
    crypto::{ContractId, MerkleTree},
    db::{db_get, db_init, db_lookup, db_set, zkas_db_set},
    error::{ContractError, ContractResult},
    msg,
    util::set_return_data,
    ContractCall,
};
use darkfi_serial::{deserialize, serialize, Decodable, Encodable, WriteExt};

use crate::{
    model::{DaoExecUpdate, DaoMintUpdate, DaoProposeUpdate, DaoVoteUpdate},
    DaoFunction, DAO_CONTRACT_DB_DAO_BULLAS, DAO_CONTRACT_DB_DAO_MERKLE_ROOTS,
    DAO_CONTRACT_DB_INFO_TREE, DAO_CONTRACT_DB_PROPOSAL_BULLAS, DAO_CONTRACT_DB_VOTE_NULLIFIERS,
    DAO_CONTRACT_KEY_DAO_MERKLE_TREE, DAO_CONTRACT_KEY_DB_VERSION,
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
    zkas_db_set(&include_bytes!("../proof/dao-exec.zk.bin")[..])?;
    zkas_db_set(&include_bytes!("../proof/dao-mint.zk.bin")[..])?;
    zkas_db_set(&include_bytes!("../proof/dao-vote-burn.zk.bin")[..])?;
    zkas_db_set(&include_bytes!("../proof/dao-vote-main.zk.bin")[..])?;
    zkas_db_set(&include_bytes!("../proof/dao-propose-burn.zk.bin")[..])?;
    zkas_db_set(&include_bytes!("../proof/dao-propose-main.zk.bin")[..])?;

    // Set up db for general info
    let dao_info_db = match db_lookup(cid, DAO_CONTRACT_DB_INFO_TREE) {
        Ok(v) => v,
        Err(_) => db_init(cid, DAO_CONTRACT_DB_INFO_TREE)?,
    };

    // Set up the entries in the header table
    match db_get(dao_info_db, &serialize(&DAO_CONTRACT_KEY_DAO_MERKLE_TREE))? {
        Some(bytes) => {
            // We found some bytes, try to deserialize into a tree.
            // For now, if this doesn't work, we bail.
            let mut decoder = Cursor::new(&bytes);
            <u32 as Decodable>::decode(&mut decoder)?;
            <MerkleTree as Decodable>::decode(&mut decoder)?;
        }
        None => {
            // We didn't find a tree, so just make a new one.
            let tree = MerkleTree::new(100);

            let mut tree_data = vec![];
            tree_data.write_u32(0)?;
            tree.encode(&mut tree_data)?;

            db_set(dao_info_db, &serialize(&DAO_CONTRACT_KEY_DAO_MERKLE_TREE), &tree_data)?;
        }
    }

    // Set up db to avoid double creating DAOs
    let _ = match db_lookup(cid, DAO_CONTRACT_DB_DAO_BULLAS) {
        Ok(v) => v,
        Err(_) => db_init(cid, DAO_CONTRACT_DB_DAO_BULLAS)?,
    };

    // Set up db for DAO bulla Merkle roots
    let _ = match db_lookup(cid, DAO_CONTRACT_DB_DAO_MERKLE_ROOTS) {
        Ok(v) => v,
        Err(_) => db_init(cid, DAO_CONTRACT_DB_DAO_MERKLE_ROOTS)?,
    };

    // Set up db for proposal votes
    // k: ProposalBulla
    // v: (BlindAggregateVote, bool) (the bool marks if the proposal is finished)
    let _ = match db_lookup(cid, DAO_CONTRACT_DB_PROPOSAL_BULLAS) {
        Ok(v) => v,
        Err(_) => db_init(cid, DAO_CONTRACT_DB_PROPOSAL_BULLAS)?,
    };

    // TODO: These nullifiers should exist per-proposal
    let _ = match db_lookup(cid, DAO_CONTRACT_DB_VOTE_NULLIFIERS) {
        Ok(v) => v,
        Err(_) => db_init(cid, DAO_CONTRACT_DB_VOTE_NULLIFIERS)?,
    };

    // Update db version
    db_set(
        dao_info_db,
        &serialize(&DAO_CONTRACT_KEY_DB_VERSION),
        &serialize(&env!("CARGO_PKG_VERSION")),
    )?;

    Ok(())
}

/// This function is used by the wasm VM's host to fetch the necessary metadata
/// for verifying signatures and ZK proofs. The payload given here are all the
/// contract calls in the transaction.
fn get_metadata(cid: ContractId, ix: &[u8]) -> ContractResult {
    let (call_idx, calls): (u32, Vec<ContractCall>) = deserialize(ix)?;
    if call_idx >= calls.len() as u32 {
        msg!("[DAO:get_metadata()] Error: call_idx >= calls.len()");
        return Err(ContractError::Internal)
    }

    match DaoFunction::try_from(calls[call_idx as usize].data[0])? {
        DaoFunction::Mint => {
            let metadata = dao_mint_get_metadata(cid, call_idx, calls)?;
            Ok(set_return_data(&metadata)?)
        }

        DaoFunction::Propose => {
            let metadata = dao_propose_get_metadata(cid, call_idx, calls)?;
            Ok(set_return_data(&metadata)?)
        }

        DaoFunction::Vote => {
            let metadata = dao_vote_get_metadata(cid, call_idx, calls)?;
            Ok(set_return_data(&metadata)?)
        }

        DaoFunction::Exec => {
            let metadata = dao_exec_get_metadata(cid, call_idx, calls)?;
            Ok(set_return_data(&metadata)?)
        }
    }
}

/// This function verifies a state transition and produces a state update
/// if everything is successful.
fn process_instruction(cid: ContractId, ix: &[u8]) -> ContractResult {
    let (call_idx, calls): (u32, Vec<ContractCall>) = deserialize(ix)?;
    if call_idx >= calls.len() as u32 {
        msg!("[DAO::process_instruction()] Error: call_idx >= calls.len()");
        return Err(ContractError::Internal)
    }

    match DaoFunction::try_from(calls[call_idx as usize].data[0])? {
        DaoFunction::Mint => {
            let update_data = dao_mint_process_instruction(cid, call_idx, calls)?;
            Ok(set_return_data(&update_data)?)
        }

        DaoFunction::Propose => {
            let update_data = dao_propose_process_instruction(cid, call_idx, calls)?;
            Ok(set_return_data(&update_data)?)
        }

        DaoFunction::Vote => {
            let update_data = dao_vote_process_instruction(cid, call_idx, calls)?;
            Ok(set_return_data(&update_data)?)
        }

        DaoFunction::Exec => {
            let update_data = dao_exec_process_instruction(cid, call_idx, calls)?;
            Ok(set_return_data(&update_data)?)
        }
    }
}

/// This function attempts to write a given state update provided the previous
/// steps of the contract call execution were successful. The payload given to
/// the functioon is the update data retrieved from `process_instruction()`.
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
    }
}
