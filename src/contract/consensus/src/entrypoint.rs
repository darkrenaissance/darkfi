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

use darkfi_money_contract::{
    model::{ConsensusStakeUpdateV1, ConsensusUnstakeUpdateV1},
    CONSENSUS_CONTRACT_COINS_TREE, CONSENSUS_CONTRACT_COIN_MERKLE_TREE,
    CONSENSUS_CONTRACT_COIN_ROOTS_TREE, CONSENSUS_CONTRACT_DB_VERSION,
    CONSENSUS_CONTRACT_INFO_TREE, CONSENSUS_CONTRACT_NULLIFIERS_TREE,
};
use darkfi_sdk::{
    crypto::{ContractId, MerkleTree},
    db::{db_init, db_lookup, db_set, zkas_db_set},
    error::{ContractError, ContractResult},
    msg,
    util::set_return_data,
    ContractCall,
};
use darkfi_serial::{deserialize, serialize, Encodable, WriteExt};

use crate::{
    model::{ConsensusProposalRewardUpdateV1, ConsensusProposalUpdateV1},
    ConsensusFunction,
};

/// `Consensus::GenesisStake` functions
mod genesis_stake_v1;
use genesis_stake_v1::{
    consensus_genesis_stake_get_metadata_v1, consensus_genesis_stake_process_instruction_v1,
};

/// `Consensus::Stake` functions
mod stake_v1;
use stake_v1::{
    consensus_stake_get_metadata_v1, consensus_stake_process_instruction_v1,
    consensus_stake_process_update_v1,
};

/// `Consensus::ProposalBurn` functions
mod proposal_burn_v1;
use proposal_burn_v1::{
    consensus_proposal_burn_get_metadata_v1, consensus_proposal_burn_process_instruction_v1,
    consensus_proposal_burn_process_update_v1,
};

/// `Consensus::ProposalRewardV1` functions
mod proposal_reward_v1;
use proposal_reward_v1::{
    consensus_proposal_reward_get_metadata_v1, consensus_proposal_reward_process_instruction_v1,
    consensus_proposal_reward_process_update_v1,
};

/// `Consensus::ProposalMintV1` functions
mod proposal_mint_v1;
use proposal_mint_v1::{
    consensus_proposal_mint_get_metadata_v1, consensus_proposal_mint_process_instruction_v1,
    consensus_proposal_mint_process_update_v1,
};

/// `Consensus::ProposalV1` functions
mod proposal_v1;
use proposal_v1::{
    consensus_proposal_get_metadata_v1, consensus_proposal_process_instruction_v1,
    consensus_proposal_process_update_v1,
};

/// `Consensus::Unstake` functions
mod unstake_v1;
use unstake_v1::{
    consensus_unstake_get_metadata_v1, consensus_unstake_process_instruction_v1,
    consensus_unstake_process_update_v1,
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
    let money_mint_v1_bincode = include_bytes!("../../money/proof/mint_v1.zk.bin");
    let money_burn_v1_bincode = include_bytes!("../../money/proof/burn_v1.zk.bin");
    let consensus_mint_v1_bincode = include_bytes!("../proof/consensus_mint_v1.zk.bin");
    let consensus_burn_v1_bincode = include_bytes!("../proof/consensus_burn_v1.zk.bin");
    let proposal_reward_v1_bincode = include_bytes!("../proof/proposal_reward_v1.zk.bin");
    let proposal_mint_v1_bincode = include_bytes!("../proof/proposal_mint_v1.zk.bin");
    let consensus_proposal_v1_bincode = include_bytes!("../proof/consensus_proposal_v1.zk.bin");

    // For that, we use `zkas_db_set` and pass in the bincode.
    zkas_db_set(&money_mint_v1_bincode[..])?;
    zkas_db_set(&money_burn_v1_bincode[..])?;
    zkas_db_set(&consensus_mint_v1_bincode[..])?;
    zkas_db_set(&consensus_burn_v1_bincode[..])?;
    zkas_db_set(&proposal_reward_v1_bincode[..])?;
    zkas_db_set(&proposal_mint_v1_bincode[..])?;
    zkas_db_set(&consensus_proposal_v1_bincode[..])?;

    // Set up a database tree to hold Merkle roots of all coins
    // k=MerkleNode, v=[]
    if db_lookup(cid, CONSENSUS_CONTRACT_COIN_ROOTS_TREE).is_err() {
        db_init(cid, CONSENSUS_CONTRACT_COIN_ROOTS_TREE)?;
    }

    // Set up a database tree to hold all coins ever seen
    // k=Coin, v=[]
    if db_lookup(cid, CONSENSUS_CONTRACT_COINS_TREE).is_err() {
        db_init(cid, CONSENSUS_CONTRACT_COINS_TREE)?;
    }

    // Set up a database tree to hold nullifiers of all spent coins
    // k=Nullifier, v=[]
    if db_lookup(cid, CONSENSUS_CONTRACT_NULLIFIERS_TREE).is_err() {
        db_init(cid, CONSENSUS_CONTRACT_NULLIFIERS_TREE)?;
    }

    // Set up a database tree for arbitrary data
    let info_db = match db_lookup(cid, CONSENSUS_CONTRACT_INFO_TREE) {
        Ok(v) => v,
        Err(_) => {
            let info_db = db_init(cid, CONSENSUS_CONTRACT_INFO_TREE)?;

            // Create the incrementalmerkletree for seen coins
            let coin_tree = MerkleTree::new(100);
            let mut coin_tree_data = vec![];

            coin_tree_data.write_u32(0)?;
            coin_tree.encode(&mut coin_tree_data)?;

            db_set(info_db, &serialize(&CONSENSUS_CONTRACT_COIN_MERKLE_TREE), &coin_tree_data)?;
            info_db
        }
    };

    // Update db version
    db_set(
        info_db,
        &serialize(&CONSENSUS_CONTRACT_DB_VERSION),
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

    match ConsensusFunction::try_from(calls[call_idx as usize].data[0])? {
        ConsensusFunction::GenesisStakeV1 => {
            // We pass everything into the correct function, and it will return
            // the metadata for us, which we can then copy into the host with
            // the `set_return_data` function. On the host, this metadata will
            // be used to do external verification (zk proofs, and signatures).
            let metadata = consensus_genesis_stake_get_metadata_v1(cid, call_idx, calls)?;
            Ok(set_return_data(&metadata)?)
        }
        ConsensusFunction::StakeV1 => {
            let metadata = consensus_stake_get_metadata_v1(cid, call_idx, calls)?;
            Ok(set_return_data(&metadata)?)
        }
        ConsensusFunction::ProposalBurnV1 => {
            let metadata = consensus_proposal_burn_get_metadata_v1(cid, call_idx, calls)?;
            Ok(set_return_data(&metadata)?)
        }
        ConsensusFunction::ProposalRewardV1 => {
            let metadata = consensus_proposal_reward_get_metadata_v1(cid, call_idx, calls)?;
            Ok(set_return_data(&metadata)?)
        }
        ConsensusFunction::ProposalMintV1 => {
            let metadata = consensus_proposal_mint_get_metadata_v1(cid, call_idx, calls)?;
            Ok(set_return_data(&metadata)?)
        }
        ConsensusFunction::ProposalV1 => {
            let metadata = consensus_proposal_get_metadata_v1(cid, call_idx, calls)?;
            Ok(set_return_data(&metadata)?)
        }
        ConsensusFunction::UnstakeV1 => {
            let metadata = consensus_unstake_get_metadata_v1(cid, call_idx, calls)?;
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

    match ConsensusFunction::try_from(calls[call_idx as usize].data[0])? {
        ConsensusFunction::GenesisStakeV1 => {
            // Again, we pass everything into the correct function.
            // If it executes successfully, we'll get a state update
            // which we can copy into the host using `set_return_data`.
            // This update can then be written with `process_update()`
            // if everything is in order.
            let update_data = consensus_genesis_stake_process_instruction_v1(cid, call_idx, calls)?;
            Ok(set_return_data(&update_data)?)
        }
        ConsensusFunction::StakeV1 => {
            let update_data = consensus_stake_process_instruction_v1(cid, call_idx, calls)?;
            Ok(set_return_data(&update_data)?)
        }
        ConsensusFunction::ProposalBurnV1 => {
            let update_data = consensus_proposal_burn_process_instruction_v1(cid, call_idx, calls)?;
            Ok(set_return_data(&update_data)?)
        }
        ConsensusFunction::ProposalRewardV1 => {
            let update_data =
                consensus_proposal_reward_process_instruction_v1(cid, call_idx, calls)?;
            Ok(set_return_data(&update_data)?)
        }
        ConsensusFunction::ProposalMintV1 => {
            let update_data = consensus_proposal_mint_process_instruction_v1(cid, call_idx, calls)?;
            Ok(set_return_data(&update_data)?)
        }
        ConsensusFunction::ProposalV1 => {
            let update_data = consensus_proposal_process_instruction_v1(cid, call_idx, calls)?;
            Ok(set_return_data(&update_data)?)
        }
        ConsensusFunction::UnstakeV1 => {
            let update_data = consensus_unstake_process_instruction_v1(cid, call_idx, calls)?;
            Ok(set_return_data(&update_data)?)
        }
    }
}

/// This function attempts to write a given state update provided the previous steps
/// of the contract call execution all were successful. It's the last in line, and
/// assumes that the transaction/call was successful. The payload given to the function
/// is the update data retrieved from `process_instruction()`.
fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    match ConsensusFunction::try_from(update_data[0])? {
        ConsensusFunction::GenesisStakeV1 => {
            // GenesisStake uses the same update as normal Stake
            let update: ConsensusStakeUpdateV1 = deserialize(&update_data[1..])?;
            Ok(consensus_stake_process_update_v1(cid, update)?)
        }
        ConsensusFunction::StakeV1 => {
            let update: ConsensusStakeUpdateV1 = deserialize(&update_data[1..])?;
            Ok(consensus_stake_process_update_v1(cid, update)?)
        }
        ConsensusFunction::ProposalBurnV1 => {
            let update: ConsensusUnstakeUpdateV1 = deserialize(&update_data[1..])?;
            Ok(consensus_proposal_burn_process_update_v1(cid, update)?)
        }
        ConsensusFunction::ProposalRewardV1 => {
            let update: ConsensusProposalRewardUpdateV1 = deserialize(&update_data[1..])?;
            Ok(consensus_proposal_reward_process_update_v1(cid, update)?)
        }
        ConsensusFunction::ProposalMintV1 => {
            let update: ConsensusStakeUpdateV1 = deserialize(&update_data[1..])?;
            Ok(consensus_proposal_mint_process_update_v1(cid, update)?)
        }
        ConsensusFunction::ProposalV1 => {
            let update: ConsensusProposalUpdateV1 = deserialize(&update_data[1..])?;
            Ok(consensus_proposal_process_update_v1(cid, update)?)
        }
        ConsensusFunction::UnstakeV1 => {
            let update: ConsensusUnstakeUpdateV1 = deserialize(&update_data[1..])?;
            Ok(consensus_unstake_process_update_v1(cid, update)?)
        }
    }
}
