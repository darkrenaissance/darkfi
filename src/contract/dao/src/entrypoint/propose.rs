/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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
    MONEY_CONTRACT_COIN_ROOTS_TREE, MONEY_CONTRACT_INFO_TREE, MONEY_CONTRACT_LATEST_COIN_ROOT,
    MONEY_CONTRACT_NULLIFIERS_TREE,
};
use darkfi_sdk::{
    crypto::{contract_id::MONEY_CONTRACT_ID, pasta_prelude::*, ContractId, MerkleNode, PublicKey},
    dark_tree::DarkLeaf,
    db::{db_contains_key, db_get, db_lookup, db_set},
    error::{ContractError, ContractResult},
    msg,
    pasta::pallas,
    util::get_verifying_block_height,
    ContractCall,
};
use darkfi_serial::{deserialize, serialize, Encodable, WriteExt};

use crate::{
    blockwindow,
    error::DaoError,
    model::{DaoBlindAggregateVote, DaoProposalMetadata, DaoProposeParams, DaoProposeUpdate},
    DaoFunction, DAO_CONTRACT_DB_DAO_MERKLE_ROOTS, DAO_CONTRACT_DB_PROPOSAL_BULLAS,
    DAO_CONTRACT_ZKAS_DAO_PROPOSE_INPUT_NS, DAO_CONTRACT_ZKAS_DAO_PROPOSE_MAIN_NS,
};

/// `get_metdata` function for `Dao::Propose`
pub(crate) fn dao_propose_get_metadata(
    _cid: ContractId,
    call_idx: u32,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx as usize].data;
    let params: DaoProposeParams = deserialize(&self_.data[1..])?;

    if params.inputs.is_empty() {
        msg!("[DAO::Propose] Error: Proposal inputs are empty");
        return Err(DaoError::ProposalInputsEmpty.into())
    }

    // Public inputs for the ZK proofs we have to verify
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    // Public keys for the transaction signatures we have to verify
    let mut signature_pubkeys: Vec<PublicKey> = vec![];

    // Commitment calculation for all inputs
    let mut total_funds_commit = pallas::Point::identity();

    // Iterate through inputs
    for input in &params.inputs {
        signature_pubkeys.push(input.signature_public);
        total_funds_commit += input.value_commit;

        let value_coords = input.value_commit.to_affine().coordinates().unwrap();
        let (sig_x, sig_y) = input.signature_public.xy();

        zk_public_inputs.push((
            DAO_CONTRACT_ZKAS_DAO_PROPOSE_INPUT_NS.to_string(),
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

    // ANCHOR: dao-blockwindow-example-usage
    let current_day = blockwindow(get_verifying_block_height());
    // ANCHOR_END: dao-blockwindow-example-usage

    let total_funds_coords = total_funds_commit.to_affine().coordinates().unwrap();
    zk_public_inputs.push((
        DAO_CONTRACT_ZKAS_DAO_PROPOSE_MAIN_NS.to_string(),
        vec![
            params.token_commit,
            params.dao_merkle_root.inner(),
            params.proposal_bulla.inner(),
            pallas::Base::from(current_day),
            *total_funds_coords.x(),
            *total_funds_coords.y(),
        ],
    ));

    // Serialize everything gathered and return it
    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    signature_pubkeys.encode(&mut metadata)?;

    Ok(metadata)
}

/// `process_instruction` function for `Dao::Propose`
pub(crate) fn dao_propose_process_instruction(
    cid: ContractId,
    call_idx: u32,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx as usize].data;
    let params: DaoProposeParams = deserialize(&self_.data[1..])?;

    let coin_roots_db = db_lookup(*MONEY_CONTRACT_ID, MONEY_CONTRACT_COIN_ROOTS_TREE)?;
    let money_nullifier_db = db_lookup(*MONEY_CONTRACT_ID, MONEY_CONTRACT_NULLIFIERS_TREE)?;
    let mut propose_nullifiers = Vec::with_capacity(params.inputs.len());

    for input in &params.inputs {
        // Check the Merkle roots for the input coins are valid
        if !db_contains_key(coin_roots_db, &serialize(&input.merkle_root))? {
            msg!("[Dao::Propose] Error: Invalid input Merkle root: {}", input.merkle_root);
            return Err(DaoError::InvalidInputMerkleRoot.into())
        }

        // Check the coins weren't already spent
        // The nullifiers should not already exist. It is the double-spend protection.
        if propose_nullifiers.contains(&input.nullifier) ||
            db_contains_key(money_nullifier_db, &serialize(&input.nullifier))?
        {
            msg!("[Dao::Vote] Error: Coin is already spent");
            return Err(DaoError::CoinAlreadySpent.into())
        }

        propose_nullifiers.push(input.nullifier);
    }

    // Is the DAO bulla generated in the ZK proof valid
    let dao_roots_db = db_lookup(cid, DAO_CONTRACT_DB_DAO_MERKLE_ROOTS)?;
    if !db_contains_key(dao_roots_db, &serialize(&params.dao_merkle_root))? {
        msg!("[Dao::Propose] Error: Invalid DAO Merkle root: {}", params.dao_merkle_root);
        return Err(DaoError::InvalidDaoMerkleRoot.into())
    }

    // Make sure the proposal doesn't already exist
    let proposal_db = db_lookup(cid, DAO_CONTRACT_DB_PROPOSAL_BULLAS)?;
    if db_contains_key(proposal_db, &serialize(&params.proposal_bulla))? {
        msg!("[Dao::Propose] Error: Proposal already exists: {:?}", params.proposal_bulla);
        return Err(DaoError::ProposalAlreadyExists.into())
    }

    // Snapshot the latest Money merkle tree
    let money_info_db = db_lookup(*MONEY_CONTRACT_ID, MONEY_CONTRACT_INFO_TREE)?;
    let Some(data) = db_get(money_info_db, MONEY_CONTRACT_LATEST_COIN_ROOT)? else {
        msg!("[Dao::Propose] Error: Failed to fetch latest Money Merkle root");
        return Err(ContractError::Internal)
    };
    let snapshot_root: MerkleNode = deserialize(&data)?;
    msg!("[Dao::Propose] Snapshotting Money at Merkle root {}", snapshot_root);

    // Create state update
    let update = DaoProposeUpdate { proposal_bulla: params.proposal_bulla, snapshot_root };
    let mut update_data = vec![];
    update_data.write_u8(DaoFunction::Propose as u8)?;
    update.encode(&mut update_data)?;
    Ok(update_data)
}

/// `process_update` function for `Dao::Propose`
pub(crate) fn dao_propose_process_update(
    cid: ContractId,
    update: DaoProposeUpdate,
) -> ContractResult {
    // Grab all db handles we want to work on
    let proposal_vote_db = db_lookup(cid, DAO_CONTRACT_DB_PROPOSAL_BULLAS)?;

    // Build the proposal metadata
    let proposal_metadata = DaoProposalMetadata {
        vote_aggregate: DaoBlindAggregateVote::default(),
        snapshot_root: update.snapshot_root,
    };

    // Set the new proposal in the db
    db_set(proposal_vote_db, &serialize(&update.proposal_bulla), &serialize(&proposal_metadata))?;

    Ok(())
}
