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
    crypto::{pasta_prelude::*, ContractId, PublicKey},
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg,
    pasta::pallas,
    wasm, ContractCall,
};
use darkfi_serial::{deserialize, serialize, Encodable};

use crate::{
    blockwindow,
    error::DaoError,
    model::{DaoExecParams, DaoExecUpdate, DaoProposalMetadata, VecAuthCallCommit},
    DAO_CONTRACT_DB_PROPOSAL_BULLAS, DAO_CONTRACT_ZKAS_DAO_EARLY_EXEC_NS,
    DAO_CONTRACT_ZKAS_DAO_EXEC_NS,
};

/// `get_metdata` function for `Dao::Exec`
pub(crate) fn dao_exec_get_metadata(
    _cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx];
    let params: DaoExecParams = deserialize(&self_.data.data[1..])?;

    // Public inputs for the ZK proofs we have to verify
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    // Public keys for the transaction signatures we have to verify
    let signature_pubkeys: Vec<PublicKey> = vec![params.signature_public];

    let current_blockwindow =
        blockwindow(wasm::util::get_verifying_block_height()?, wasm::util::get_block_target()?);

    let blind_vote = params.blind_total_vote;
    let yes_vote_coords = blind_vote.yes_vote_commit.to_affine().coordinates().unwrap();
    let all_vote_coords = blind_vote.all_vote_commit.to_affine().coordinates().unwrap();

    // Grab proof namespace to use, based on early execution flag
    let proof_namespace = match params.early_exec {
        true => DAO_CONTRACT_ZKAS_DAO_EARLY_EXEC_NS.to_string(),
        false => DAO_CONTRACT_ZKAS_DAO_EXEC_NS.to_string(),
    };

    zk_public_inputs.push((
        proof_namespace,
        vec![
            params.proposal_bulla.inner(),
            params.proposal_auth_calls.commit(),
            pallas::Base::from(current_blockwindow),
            *yes_vote_coords.x(),
            *yes_vote_coords.y(),
            *all_vote_coords.x(),
            *all_vote_coords.y(),
            params.signature_public.x(),
            params.signature_public.y(),
        ],
    ));

    // Serialize everything gathered and return it
    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    signature_pubkeys.encode(&mut metadata)?;

    Ok(metadata)
}

/// `process_instruction` function for `Dao::Exec`
pub(crate) fn dao_exec_process_instruction(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx];
    let params: DaoExecParams = deserialize(&self_.data.data[1..])?;

    ///////////////////////////////////////////////////
    // 1. Verify the correct calling formats match the proposal
    ///////////////////////////////////////////////////

    // Check children of DAO exec match the specified calls
    if params.proposal_auth_calls.len() != self_.children_indexes.len() {
        return Err(DaoError::ExecCallWrongChildCallsLen.into())
    }
    for (auth_call, child_idx) in
        params.proposal_auth_calls.iter().zip(self_.children_indexes.iter())
    {
        let child_call = &calls[*child_idx].data;

        // We are allowing 2nd tier child calls here since it
        // should be allowed to make recursive calls.
        // Auth modules should check the direct parent is DAO::exec().
        // Doing anything else is potentially risky.

        let contract_id = child_call.contract_id;
        let function_code = child_call.data[0];

        // Check they match the auth call spec
        if contract_id != auth_call.contract_id || function_code != auth_call.function_code {
            msg!("[Dao::Exec] Error: wrong child call");
            return Err(DaoError::ExecCallWrongChildCall.into())
        }
    }

    ///////////////////////////////////////////////////
    // 2. Verify the correct voting
    ///////////////////////////////////////////////////

    // Get the ProposalVote from DAO state
    let proposal_db = wasm::db::db_lookup(cid, DAO_CONTRACT_DB_PROPOSAL_BULLAS)?;
    let Some(data) = wasm::db::db_get(proposal_db, &serialize(&params.proposal_bulla))? else {
        msg!("[Dao::Exec] Error: Proposal {:?} not found", params.proposal_bulla);
        return Err(DaoError::ProposalNonexistent.into())
    };
    let proposal: DaoProposalMetadata = deserialize(&data)?;

    // Check yes_vote commit and all_vote_commit are the same as in BlindAggregateVote
    if proposal.vote_aggregate.yes_vote_commit != params.blind_total_vote.yes_vote_commit ||
        proposal.vote_aggregate.all_vote_commit != params.blind_total_vote.all_vote_commit
    {
        return Err(DaoError::VoteCommitMismatch.into())
    }

    // Create state update
    let update = DaoExecUpdate { proposal_bulla: params.proposal_bulla };
    Ok(serialize(&update))
}

/// `process_update` function for `Dao::Exec`
pub(crate) fn dao_exec_process_update(cid: ContractId, update: DaoExecUpdate) -> ContractResult {
    // Remove proposal from db
    let proposal_vote_db = wasm::db::db_lookup(cid, DAO_CONTRACT_DB_PROPOSAL_BULLAS)?;
    wasm::db::db_del(proposal_vote_db, &serialize(&update.proposal_bulla))?;

    Ok(())
}
