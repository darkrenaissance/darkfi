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
    error::MoneyError, CONSENSUS_CONTRACT_COINS_TREE, CONSENSUS_CONTRACT_COIN_MERKLE_TREE,
    CONSENSUS_CONTRACT_COIN_ROOTS_TREE, CONSENSUS_CONTRACT_INFO_TREE,
    CONSENSUS_CONTRACT_NULLIFIERS_TREE, CONSENSUS_CONTRACT_ZKAS_PROPOSAL_NS_V1,
};
use darkfi_sdk::{
    crypto::{pasta_prelude::*, pedersen_commitment_u64, poseidon_hash, ContractId, MerkleNode},
    db::{db_contains_key, db_lookup, db_set},
    error::{ContractError, ContractResult},
    merkle_add, msg,
    pasta::{group::ff::FromUniformBytes, pallas},
    util::get_slot_checkpoint,
    ContractCall,
};
use darkfi_serial::{deserialize, serialize, Encodable, WriteExt};

use crate::{
    error::ConsensusError,
    model::{
        ConsensusProposalParamsV1, ConsensusProposalUpdateV1, SlotCheckpoint, HEADSTART,
        MU_RHO_PREFIX, MU_Y_PREFIX,
    },
    ConsensusFunction,
};

/// `get_metadata` function for `Consensus::ProposalV1`
pub(crate) fn consensus_proposal_get_metadata_v1(
    _cid: ContractId,
    call_idx: u32,
    calls: Vec<ContractCall>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx as usize];
    let params: ConsensusProposalParamsV1 = deserialize(&self_.data[1..])?;

    // Public inputs for the ZK proofs we have to verify
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    // Public keys for the transaction signatures we have to verify
    let signature_pubkeys = vec![params.input.signature_public];

    // Grab the nullifier for the burnt coin
    let nullifier = &params.input.nullifier;

    // Grab the mint epoch pallas for the burnt coin
    let epoch_pallas = pallas::Base::from(params.input.epoch);

    // Grab the public key coordinates for the burnt coin
    let (pub_x, pub_y) = &params.input.signature_public.xy();

    // Grab the burnt coin merkle root
    let merkle_root = params.input.merkle_root.inner();

    // Grab the pedersen commitment for the burnt value
    let value_coords = &params.input.value_commit.to_affine().coordinates().unwrap();

    // Grab the pedersen commitment for the minted serial number
    let new_serial_coords = &params.new_serial_commit.to_affine().coordinates().unwrap();

    // Grab the reward pallas
    let reward_pallas = pallas::Base::from(params.reward);

    // Grab the pedersen commitment for the minted value
    let new_value_coords = &params.output.value_commit.to_affine().coordinates().unwrap();

    // Grab the new coin
    let new_coin = params.output.coin.inner();

    // Grab proposal coin y and rho for lottery
    let y = &params.y;
    let rho = &params.rho;

    // Grab the slot checkpoint to validate consensus parameters against
    let slot = &params.slot;
    let Some(slot_checkpoint) = get_slot_checkpoint(*slot)? else {
        msg!("[ConsensusProposalV1] Error: Missing slot checkpoint {} from db", slot);
        return Err(ConsensusError::ProposalMissingSlotCheckpoint.into())
    };
    let slot_checkpoint: SlotCheckpoint = deserialize(&slot_checkpoint)?;

    // Verify eta VRF proof
    let slot_pallas = pallas::Base::from(slot_checkpoint.slot);
    // NOTE: slot checkpoint eta to be renamed to previous_eta,
    //       corresponding to previous block eta.
    let mut vrf_input = [0u8; 64];
    vrf_input[..32].copy_from_slice(&slot_checkpoint.eta.to_repr());
    vrf_input[32..].copy_from_slice(&slot_pallas.to_repr());
    let vrf_proof = &params.vrf_proof;
    if !vrf_proof.verify(params.input.signature_public, &vrf_input) {
        msg!("[ConsensusProposalV1] Error: eta VRF proof couldn't be verified");
        return Err(ConsensusError::ProposalErroneousVrfProof.into())
    }
    let mut eta = [0u8; 64];
    eta[..blake3::OUT_LEN].copy_from_slice(vrf_proof.hash_output().as_bytes());
    let eta = pallas::Base::from_uniform_bytes(&eta);

    // Calculate election seeds
    let mu_y = poseidon_hash([MU_Y_PREFIX, eta, slot_pallas]);
    let mu_rho = poseidon_hash([MU_RHO_PREFIX, eta, slot_pallas]);

    // Grab sigmas from slot checkpoint
    let (sigma1, sigma2) = (slot_checkpoint.sigma1, slot_checkpoint.sigma2);

    zk_public_inputs.push((
        CONSENSUS_CONTRACT_ZKAS_PROPOSAL_NS_V1.to_string(),
        vec![
            nullifier.inner(),
            epoch_pallas,
            *pub_x,
            *pub_y,
            merkle_root,
            *value_coords.x(),
            *value_coords.y(),
            *new_serial_coords.x(),
            *new_serial_coords.y(),
            reward_pallas,
            *new_value_coords.x(),
            *new_value_coords.y(),
            new_coin,
            mu_y,
            *y,
            mu_rho,
            *rho,
            sigma1,
            sigma2,
            HEADSTART,
        ],
    ));

    // Serialize everything gathered and return it
    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    signature_pubkeys.encode(&mut metadata)?;

    Ok(metadata)
}

/// `process_instruction` function for `Consensus::ProposalV1`
pub(crate) fn consensus_proposal_process_instruction_v1(
    cid: ContractId,
    call_idx: u32,
    calls: Vec<ContractCall>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx as usize];
    let params: ConsensusProposalParamsV1 = deserialize(&self_.data[1..])?;

    // Access the necessary databases where there is information to
    // validate this state transition.
    let nullifiers_db = db_lookup(cid, CONSENSUS_CONTRACT_NULLIFIERS_TREE)?;
    let coins_db = db_lookup(cid, CONSENSUS_CONTRACT_COINS_TREE)?;
    let coin_roots_db = db_lookup(cid, CONSENSUS_CONTRACT_COIN_ROOTS_TREE)?;

    // ===================================
    // Perform the actual state transition
    // ===================================

    msg!("[ConsensusProposalV1] Validating anonymous input");
    let input = &params.input;
    let output = &params.output;

    // The Merkle root is used to know whether this is a coin that
    // existed in a previous state.
    if !db_contains_key(coin_roots_db, &serialize(&input.merkle_root))? {
        msg!("[ConsensusProposalV1] Error: Merkle root not found in previous state");
        return Err(MoneyError::TransferMerkleRootNotFound.into())
    }

    // The nullifier should not already exist. It is the double-spend protection.
    if db_contains_key(nullifiers_db, &serialize(&input.nullifier))? {
        msg!("[ConsensusProposalV1] Error: Duplicate nullifier found");
        return Err(MoneyError::DuplicateNullifier.into())
    }

    // Verify value commits match between burnt and mint inputs
    let mut valcom_total = pallas::Point::identity();
    valcom_total += input.value_commit;
    valcom_total += pedersen_commitment_u64(params.reward, params.reward_blind);
    valcom_total -= output.value_commit;
    if valcom_total != pallas::Point::identity() {
        msg!("[ConsensusProposalV1] Error: Value commitments do not result in identity");
        return Err(MoneyError::ValueMismatch.into())
    }

    // Newly created coin for this call is in the output. Here we gather it,
    // and we also check that it hasn't existed before.
    if db_contains_key(coins_db, &serialize(&output.coin))? {
        msg!("[ConsensusProposalV1] Error: Duplicate coin found in output");
        return Err(MoneyError::DuplicateCoin.into())
    }

    // At this point the state transition has passed, so we create a state update
    let update = ConsensusProposalUpdateV1 { nullifier: input.nullifier, coin: output.coin };
    let mut update_data = vec![];
    update_data.write_u8(ConsensusFunction::ProposalV1 as u8)?;
    update.encode(&mut update_data)?;

    // and return it
    Ok(update_data)
}

/// `process_update` function for `Consensus::ProposalV1`
pub(crate) fn consensus_proposal_process_update_v1(
    cid: ContractId,
    update: ConsensusProposalUpdateV1,
) -> ContractResult {
    // Grab all necessary db handles for where we want to write
    let nullifiers_db = db_lookup(cid, CONSENSUS_CONTRACT_NULLIFIERS_TREE)?;
    let info_db = db_lookup(cid, CONSENSUS_CONTRACT_INFO_TREE)?;
    let coins_db = db_lookup(cid, CONSENSUS_CONTRACT_COINS_TREE)?;
    let coin_roots_db = db_lookup(cid, CONSENSUS_CONTRACT_COIN_ROOTS_TREE)?;

    msg!("[ConsensusProposalV1] Adding new nullifier to the set");
    db_set(nullifiers_db, &serialize(&update.nullifier), &[])?;

    msg!("[ConsensusProposalV1] Adding new coin to the set");
    db_set(coins_db, &serialize(&update.coin), &[])?;

    msg!("[ConsensusProposalV1] Adding new coin to the Merkle tree");
    let coins: Vec<_> = vec![MerkleNode::from(update.coin.inner())];
    merkle_add(info_db, coin_roots_db, &serialize(&CONSENSUS_CONTRACT_COIN_MERKLE_TREE), &coins)?;

    Ok(())
}
