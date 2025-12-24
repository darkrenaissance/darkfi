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
    model::{DaoProposalMetadata, DaoVoteParams, DaoVoteUpdate},
    DAO_CONTRACT_PROPOSAL_BULLAS_TREE, DAO_CONTRACT_VOTE_NULLIFIERS_TREE,
    DAO_CONTRACT_ZKAS_VOTE_INPUT_NS, DAO_CONTRACT_ZKAS_VOTE_MAIN_NS,
};

/// `get_metdata` function for `Dao::Vote`
pub(crate) fn dao_vote_get_metadata(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: DaoVoteParams = deserialize(&self_.data[1..])?;

    if params.inputs.is_empty() {
        msg!("[Dao::Vote] Error: Vote inputs are empty");
        return Err(DaoError::VoteInputsEmpty.into())
    }

    // Public inputs for the ZK proofs we have to verify
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    // Public keys for the transaction signatures we have to verify
    let mut signature_pubkeys: Vec<PublicKey> = vec![];

    // Commitment calculation for all votes
    let mut all_vote_commit = pallas::Point::identity();

    let proposal_db = wasm::db::db_lookup(cid, DAO_CONTRACT_PROPOSAL_BULLAS_TREE)?;
    let Some(data) = wasm::db::db_get(proposal_db, &serialize(&params.proposal_bulla))? else {
        msg!("[Dao::Vote] Error: Proposal doesn't exist: {:?}", params.proposal_bulla);
        return Err(DaoError::ProposalNonexistent.into())
    };
    // Get the current votes
    let proposal_metadata: DaoProposalMetadata = deserialize(&data)?;

    // Iterate through inputs
    for input in &params.inputs {
        signature_pubkeys.push(input.signature_public);
        all_vote_commit += input.vote_commit;

        let value_coords = input.vote_commit.to_affine().coordinates().unwrap();
        let (sig_x, sig_y) = input.signature_public.xy();

        zk_public_inputs.push((
            DAO_CONTRACT_ZKAS_VOTE_INPUT_NS.to_string(),
            vec![
                proposal_metadata.snapshot_nulls,
                params.proposal_bulla.inner(),
                input.vote_nullifier.inner(),
                *value_coords.x(),
                *value_coords.y(),
                params.token_commit,
                proposal_metadata.snapshot_coins.inner(),
                sig_x,
                sig_y,
            ],
        ));
    }

    let current_blockwindow =
        blockwindow(wasm::util::get_verifying_block_height()?, wasm::util::get_block_target()?);

    let yes_vote_commit_coords = params.yes_vote_commit.to_affine().coordinates().unwrap();
    let all_vote_commit_coords = all_vote_commit.to_affine().coordinates().unwrap();

    let (ephem_x, ephem_y) = params.note.ephem_public.xy();
    zk_public_inputs.push((
        DAO_CONTRACT_ZKAS_VOTE_MAIN_NS.to_string(),
        vec![
            params.token_commit,
            params.proposal_bulla.inner(),
            *yes_vote_commit_coords.x(),
            *yes_vote_commit_coords.y(),
            *all_vote_commit_coords.x(),
            *all_vote_commit_coords.y(),
            pallas::Base::from(current_blockwindow),
            ephem_x,
            ephem_y,
            params.note.encrypted_values[0],
            params.note.encrypted_values[1],
            params.note.encrypted_values[2],
            params.note.encrypted_values[3],
        ],
    ));

    // Serialize everything gathered and return it
    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    signature_pubkeys.encode(&mut metadata)?;

    Ok(metadata)
}

/// `process_instruction` function for `Dao::Vote`
pub(crate) fn dao_vote_process_instruction(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: DaoVoteParams = deserialize(&self_.data[1..])?;

    // Check proposal bulla exists
    let proposal_db = wasm::db::db_lookup(cid, DAO_CONTRACT_PROPOSAL_BULLAS_TREE)?;
    let Some(data) = wasm::db::db_get(proposal_db, &serialize(&params.proposal_bulla))? else {
        msg!("[Dao::Vote] Error: Proposal doesn't exist: {:?}", params.proposal_bulla);
        return Err(DaoError::ProposalNonexistent.into())
    };

    // Get the current votes
    let mut proposal_metadata: DaoProposalMetadata = deserialize(&data)?;

    // Check the Merkle root and nullifiers for the input coins are valid
    let dao_vote_nullifier_db = wasm::db::db_lookup(cid, DAO_CONTRACT_VOTE_NULLIFIERS_TREE)?;
    let mut vote_nullifiers = vec![];

    for input in &params.inputs {
        // Prefix nullifier with proposal bulla so nullifiers from different proposals
        // don't interfere with each other.
        let null_key = serialize(&(params.proposal_bulla, input.vote_nullifier));

        if vote_nullifiers.contains(&input.vote_nullifier) ||
            wasm::db::db_contains_key(dao_vote_nullifier_db, &null_key)?
        {
            msg!("[Dao::Vote] Error: Attempted double vote");
            return Err(DaoError::DoubleVote.into())
        }

        proposal_metadata.vote_aggregate.all_vote_commit += input.vote_commit;
        vote_nullifiers.push(input.vote_nullifier);
    }

    proposal_metadata.vote_aggregate.yes_vote_commit += params.yes_vote_commit;

    // Create state update
    let update =
        DaoVoteUpdate { proposal_bulla: params.proposal_bulla, proposal_metadata, vote_nullifiers };
    Ok(serialize(&update))
}

/// `process_update` function for `Dao::Vote`
pub(crate) fn dao_vote_process_update(cid: ContractId, update: DaoVoteUpdate) -> ContractResult {
    // Grab all db handles we want to work on
    let proposal_db = wasm::db::db_lookup(cid, DAO_CONTRACT_PROPOSAL_BULLAS_TREE)?;

    // Perform this code:
    //   total_yes_vote_commit += update.yes_vote_commit
    //   total_all_vote_commit += update.all_vote_commit
    wasm::db::db_set(
        proposal_db,
        &serialize(&update.proposal_bulla),
        &serialize(&update.proposal_metadata),
    )?;

    // We are essentially doing: vote_nulls.append(update_nulls)
    let dao_vote_nulls_db = wasm::db::db_lookup(cid, DAO_CONTRACT_VOTE_NULLIFIERS_TREE)?;

    for nullifier in update.vote_nullifiers {
        // Uniqueness is enforced for (proposal_bulla, nullifier)
        let key = serialize(&(update.proposal_bulla, nullifier));
        wasm::db::db_set(dao_vote_nulls_db, &key, &[])?;
    }

    Ok(())
}
