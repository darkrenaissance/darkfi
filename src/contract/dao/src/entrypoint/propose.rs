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

use darkfi_money_contract::{
    error::MoneyError, MONEY_CONTRACT_COIN_ROOTS_TREE, MONEY_CONTRACT_INFO_TREE,
    MONEY_CONTRACT_LATEST_COIN_ROOT, MONEY_CONTRACT_LATEST_NULLIFIER_ROOT,
    MONEY_CONTRACT_NULLIFIER_ROOTS_TREE,
};
use darkfi_sdk::{
    crypto::{contract_id::MONEY_CONTRACT_ID, pasta_prelude::*, ContractId, MerkleNode, PublicKey},
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg,
    pasta::pallas,
    tx::TransactionHash,
    wasm, ContractCall,
};
use darkfi_serial::{deserialize, serialize, Encodable};

use crate::{
    blockwindow,
    error::DaoError,
    model::{DaoBlindAggregateVote, DaoProposalMetadata, DaoProposeParams, DaoProposeUpdate},
    DAO_CONTRACT_MERKLE_ROOTS_TREE, DAO_CONTRACT_PROPOSAL_BULLAS_TREE,
    DAO_CONTRACT_ZKAS_PROPOSE_INPUT_NS, DAO_CONTRACT_ZKAS_PROPOSE_MAIN_NS,
    PROPOSAL_SNAPSHOT_CUTOFF_LIMIT,
};

/// `get_metdata` function for `Dao::Propose`
pub(crate) fn dao_propose_get_metadata(
    _cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
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
            DAO_CONTRACT_ZKAS_PROPOSE_INPUT_NS.to_string(),
            vec![
                input.smt_null_root,
                *value_coords.x(),
                *value_coords.y(),
                params.token_commit,
                input.merkle_coin_root.inner(),
                sig_x,
                sig_y,
            ],
        ));
    }

    // ANCHOR: dao-blockwindow-example-usage
    let current_blockwindow =
        blockwindow(wasm::util::get_verifying_block_height()?, wasm::util::get_block_target()?);
    // ANCHOR_END: dao-blockwindow-example-usage

    let total_funds_coords = total_funds_commit.to_affine().coordinates().unwrap();
    zk_public_inputs.push((
        DAO_CONTRACT_ZKAS_PROPOSE_MAIN_NS.to_string(),
        vec![
            params.token_commit,
            params.dao_merkle_root.inner(),
            params.proposal_bulla.inner(),
            pallas::Base::from(current_blockwindow),
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
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: DaoProposeParams = deserialize(&self_.data[1..])?;

    let coin_roots_db = wasm::db::db_lookup(*MONEY_CONTRACT_ID, MONEY_CONTRACT_COIN_ROOTS_TREE)?;
    let null_roots_db =
        wasm::db::db_lookup(*MONEY_CONTRACT_ID, MONEY_CONTRACT_NULLIFIER_ROOTS_TREE)?;

    for input in &params.inputs {
        // Check the Merkle roots for the input coins are valid
        let Some(coin_root_data) =
            wasm::db::db_get(coin_roots_db, &serialize(&input.merkle_coin_root))?
        else {
            msg!(
                "[Dao::Propose] Error: Invalid input Merkle root: {:?}",
                input.merkle_coin_root.inner()
            );
            return Err(DaoError::InvalidInputMerkleRoot.into())
        };
        if coin_root_data.len() != 32 + 1 {
            msg!(
                "[Dao::Propose] Error: Coin roots data length is not expected(32 + 1): {}",
                coin_root_data.len()
            );
            return Err(MoneyError::RootsValueDataMismatch.into())
        }

        // Check the SMT roots for the input nullifiers are valid
        let Some(null_root_data) =
            wasm::db::db_get(null_roots_db, &serialize(&input.smt_null_root))?
        else {
            msg!("[Dao::Propose] Error: Invalid input SMT root: {:?}", input.smt_null_root);
            return Err(DaoError::InvalidInputMerkleRoot.into())
        };

        // Deserialize the SMT roots set
        let null_root_data: Vec<Vec<u8>> = match deserialize(&null_root_data) {
            Ok(set) => set,
            Err(e) => {
                msg!("[Dao::Propose] Error: Failed to deserialize nulls root snapshot: {}", e);
                return Err(DaoError::SnapshotDeserializationError.into())
            }
        };

        // Nullifiers roots snapshot must include the Merkle root data
        if !null_root_data.contains(&coin_root_data) {
            msg!("[Dao::Propose] Error: coin roots snapshot for {:?} does not exist in the nulls root snapshot {:?}",
                 input.merkle_coin_root.inner(), input.smt_null_root);
            return Err(DaoError::NonMatchingSnapshotRoots.into())
        }

        // Get block_height where tx_hash was confirmed
        let tx_hash_data: [u8; 32] = coin_root_data[0..32].try_into().unwrap();
        let tx_hash = TransactionHash(tx_hash_data);
        let (tx_height, _) = wasm::util::get_tx_location(&tx_hash)?;

        // Check snapshot age againts current height
        let current_height = wasm::util::get_verifying_block_height()?;
        if current_height - tx_height > PROPOSAL_SNAPSHOT_CUTOFF_LIMIT {
            msg!("[Dao::Propose] Error: Snapshot is too old. Current height: {}, snapshot height: {}",
                 current_height, tx_height);
            return Err(DaoError::SnapshotTooOld.into())
        }
    }

    // Is the DAO bulla generated in the ZK proof valid
    let dao_roots_db = wasm::db::db_lookup(cid, DAO_CONTRACT_MERKLE_ROOTS_TREE)?;
    if !wasm::db::db_contains_key(dao_roots_db, &serialize(&params.dao_merkle_root))? {
        msg!("[Dao::Propose] Error: Invalid DAO Merkle root: {}", params.dao_merkle_root);
        return Err(DaoError::InvalidDaoMerkleRoot.into())
    }

    // Make sure the proposal doesn't already exist
    let proposal_db = wasm::db::db_lookup(cid, DAO_CONTRACT_PROPOSAL_BULLAS_TREE)?;
    if wasm::db::db_contains_key(proposal_db, &serialize(&params.proposal_bulla))? {
        msg!("[Dao::Propose] Error: Proposal already exists: {:?}", params.proposal_bulla);
        return Err(DaoError::ProposalAlreadyExists.into())
    }

    // Snapshot the latest Money merkle tree
    let money_info_db = wasm::db::db_lookup(*MONEY_CONTRACT_ID, MONEY_CONTRACT_INFO_TREE)?;
    let Some(data) = wasm::db::db_get(money_info_db, MONEY_CONTRACT_LATEST_COIN_ROOT)? else {
        msg!("[Dao::Propose] Error: Failed to fetch latest Money Merkle root");
        return Err(ContractError::Internal)
    };
    let snapshot_coins: MerkleNode = deserialize(&data)?;

    let Some(data) = wasm::db::db_get(money_info_db, MONEY_CONTRACT_LATEST_NULLIFIER_ROOT)? else {
        msg!("[Dao::Propose] Error: Failed to fetch latest Money SMT root");
        return Err(ContractError::Internal)
    };
    let snapshot_nulls: pallas::Base = deserialize(&data)?;

    msg!(
        "[Dao::Propose] Snapshotting Money at Merkle {} and SMT {:?}",
        snapshot_coins,
        snapshot_nulls
    );

    // Create state update
    let update =
        DaoProposeUpdate { proposal_bulla: params.proposal_bulla, snapshot_coins, snapshot_nulls };
    Ok(serialize(&update))
}

/// `process_update` function for `Dao::Propose`
pub(crate) fn dao_propose_process_update(
    cid: ContractId,
    update: DaoProposeUpdate,
) -> ContractResult {
    // Grab all db handles we want to work on
    let proposal_db = wasm::db::db_lookup(cid, DAO_CONTRACT_PROPOSAL_BULLAS_TREE)?;

    // Build the proposal metadata
    let proposal_metadata = DaoProposalMetadata {
        vote_aggregate: DaoBlindAggregateVote::default(),
        snapshot_coins: update.snapshot_coins,
        snapshot_nulls: update.snapshot_nulls,
    };

    // Set the new proposal in the db
    wasm::db::db_set(
        proposal_db,
        &serialize(&update.proposal_bulla),
        &serialize(&proposal_metadata),
    )?;

    Ok(())
}
