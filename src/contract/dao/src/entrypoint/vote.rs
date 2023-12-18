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

use darkfi_money_contract::MONEY_CONTRACT_NULLIFIERS_TREE;
use darkfi_sdk::{
    crypto::{contract_id::MONEY_CONTRACT_ID, pasta_prelude::*, ContractId, PublicKey},
    dark_tree::DarkLeaf,
    db::{db_contains_key, db_get, db_lookup, db_set},
    error::{ContractError, ContractResult},
    msg,
    pasta::pallas,
    ContractCall,
};
use darkfi_serial::{deserialize, serialize, Encodable, WriteExt};

use crate::{
    error::DaoError,
    model::{DaoProposalMetadata, DaoVoteParams, DaoVoteUpdate},
    DaoFunction, DAO_CONTRACT_DB_PROPOSAL_BULLAS, DAO_CONTRACT_DB_VOTE_NULLIFIERS,
    DAO_CONTRACT_ZKAS_DAO_VOTE_BURN_NS, DAO_CONTRACT_ZKAS_DAO_VOTE_MAIN_NS,
};

/// `get_metdata` function for `Dao::Vote`
pub(crate) fn dao_vote_get_metadata(
    _cid: ContractId,
    call_idx: u32,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx as usize].data;
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

    // Iterate through inputs
    for input in &params.inputs {
        signature_pubkeys.push(input.signature_public);
        all_vote_commit += input.vote_commit;

        let value_coords = input.vote_commit.to_affine().coordinates().unwrap();
        let (sig_x, sig_y) = input.signature_public.xy();

        // TODO: Here we "trust" the input param's merkle root. Instead we compare
        // that this root equals to the proposal's snapshotted root later in the
        // `process_instruction`. Should we just enforce it here instead/aswell?
        // The reason is because ZK proofs are verified afterwards, so by checking
        // in wasm first, we can potentially bail out more quickly.
        zk_public_inputs.push((
            DAO_CONTRACT_ZKAS_DAO_VOTE_BURN_NS.to_string(),
            vec![
                input.nullifier.inner(),
                *value_coords.x(),
                *value_coords.y(),
                params.token_commit,
                input.merkle_root.inner(),
                sig_x,
                sig_y,
            ],
        ));
    }

    let yes_vote_commit_coords = params.yes_vote_commit.to_affine().coordinates().unwrap();
    let all_vote_commit_coords = all_vote_commit.to_affine().coordinates().unwrap();

    zk_public_inputs.push((
        DAO_CONTRACT_ZKAS_DAO_VOTE_MAIN_NS.to_string(),
        vec![
            params.token_commit,
            params.proposal_bulla.inner(),
            *yes_vote_commit_coords.x(),
            *yes_vote_commit_coords.y(),
            *all_vote_commit_coords.x(),
            *all_vote_commit_coords.y(),
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
    call_idx: u32,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx as usize].data;
    let params: DaoVoteParams = deserialize(&self_.data[1..])?;

    // Check proposal bulla exists
    let proposal_votes_db = db_lookup(cid, DAO_CONTRACT_DB_PROPOSAL_BULLAS)?;
    let Some(data) = db_get(proposal_votes_db, &serialize(&params.proposal_bulla))? else {
        msg!("[Dao::Vote] Error: Proposal doesn't exist: {:?}", params.proposal_bulla);
        return Err(DaoError::ProposalNonexistent.into())
    };

    // Get the current votes, and additionally confirm proposal hasn't ended
    // TODO: Proposals should have a set length of time
    let mut proposal_metadata: DaoProposalMetadata = deserialize(&data)?;

    if proposal_metadata.ended {
        msg!("[Dao::Vote] Error: Proposal ended: {:?}", params.proposal_bulla);
        return Err(DaoError::ProposalEnded.into())
    }

    // Check the Merkle root and nullifiers for the input coins are valid
    let money_nullifier_db = db_lookup(*MONEY_CONTRACT_ID, MONEY_CONTRACT_NULLIFIERS_TREE)?;
    let dao_vote_nullifier_db = db_lookup(cid, DAO_CONTRACT_DB_VOTE_NULLIFIERS)?;
    let mut vote_nullifiers = vec![];

    for input in &params.inputs {
        if proposal_metadata.snapshot_root != input.merkle_root {
            msg!(
                "[Dao::Vote] Error: Invalid input Merkle root: {} (expected {})",
                input.merkle_root,
                proposal_metadata.snapshot_root
            );
            return Err(DaoError::InvalidInputMerkleRoot.into())
        }

        if db_contains_key(money_nullifier_db, &serialize(&input.nullifier))? {
            msg!("[Dao::Vote] Error: Coin is already spent");
            return Err(DaoError::CoinAlreadySpent.into())
        }

        // Prefix nullifier with proposal bulla so nullifiers from different proposals
        // don't interfere with each other.
        let null_key = serialize(&(params.proposal_bulla, input.nullifier));

        if vote_nullifiers.contains(&input.nullifier) ||
            db_contains_key(dao_vote_nullifier_db, &null_key)?
        {
            msg!("[Dao::Vote] Error: Attempted double vote");
            return Err(DaoError::DoubleVote.into())
        }

        proposal_metadata.vote_aggregate.all_vote_commit += input.vote_commit;
        vote_nullifiers.push(input.nullifier);
    }

    proposal_metadata.vote_aggregate.yes_vote_commit += params.yes_vote_commit;

    // Create state update
    let update =
        DaoVoteUpdate { proposal_bulla: params.proposal_bulla, proposal_metadata, vote_nullifiers };

    let mut update_data = vec![];
    update_data.write_u8(DaoFunction::Vote as u8)?;
    update.encode(&mut update_data)?;
    Ok(update_data)
}

/// `process_update` function for `Dao::Vote`
pub(crate) fn dao_vote_process_update(cid: ContractId, update: DaoVoteUpdate) -> ContractResult {
    // Grab all db handles we want to work on
    let proposal_vote_db = db_lookup(cid, DAO_CONTRACT_DB_PROPOSAL_BULLAS)?;

    // Perform this code:
    //   total_yes_vote_commit += update.yes_vote_commit
    //   total_all_vote_commit += update.all_vote_commit
    db_set(
        proposal_vote_db,
        &serialize(&update.proposal_bulla),
        &serialize(&update.proposal_metadata),
    )?;

    // We are essentially doing: vote_nulls.append(update_nulls)
    let dao_vote_nulls_db = db_lookup(cid, DAO_CONTRACT_DB_VOTE_NULLIFIERS)?;

    for nullifier in update.vote_nullifiers {
        // Uniqueness is enforced for (proposal_bulla, nullifier)
        let key = serialize(&(update.proposal_bulla, nullifier));
        db_set(dao_vote_nulls_db, &key, &[])?;
    }

    Ok(())
}
