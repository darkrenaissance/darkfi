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

use darkfi_money_contract::{error::MoneyError, CONSENSUS_CONTRACT_ZKAS_PROPOSAL_REWARD_NS_V1};
use darkfi_sdk::{
    crypto::{
        pasta_prelude::*, pedersen_commitment_base, pedersen_commitment_u64, poseidon_hash,
        ContractId, CONSENSUS_CONTRACT_ID, DARK_TOKEN_ID,
    },
    error::{ContractError, ContractResult},
    msg,
    pasta::{group::ff::FromUniformBytes, pallas},
    util::get_slot_checkpoint,
    ContractCall,
};
use darkfi_serial::{deserialize, Encodable, WriteExt};

use crate::{
    error::ConsensusError,
    model::{
        ConsensusProposalBurnParamsV1, ConsensusProposalMintParamsV1,
        ConsensusProposalRewardParamsV1, ConsensusProposalRewardUpdateV1, SlotCheckpoint,
        HEADSTART, MU_RHO_PREFIX, MU_Y_PREFIX, REWARD, ZERO,
    },
    ConsensusFunction,
};

/// `get_metadata` function for `Consensus::ProposalRewardV1`
pub(crate) fn consensus_proposal_reward_get_metadata_v1(
    _cid: ContractId,
    call_idx: u32,
    calls: Vec<ContractCall>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx as usize];
    let params: ConsensusProposalRewardParamsV1 = deserialize(&self_.data[1..])?;

    // Public inputs for the ZK proofs we have to verify
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    // Public keys for the transaction signatures we have to verify
    let signature_pubkeys = vec![params.mint_input.signature_public];

    // Grab the nullifier for the burnt coin
    let nullifier = &params.burnt_input.nullifier;

    // Grab the public key coordinates for the burnt coin
    let (pub_x, pub_y) = &params.burnt_public_key.xy();

    // Grab the pedersen commitment for the burnt value
    let value_coords = &params.burnt_input.value_commit.to_affine().coordinates().unwrap();

    // Grab the pedersen commitment for the minted serial number
    let new_serial_coords = &params.new_serial_commit.to_affine().coordinates().unwrap();

    // Grab the pedersen commitment for the minted value
    let new_value_coords = &params.mint_input.value_commit.to_affine().coordinates().unwrap();

    // Grab proposal coin y and rho for lottery
    let y = &params.y;
    let rho = &params.rho;

    // Grab the slot checkpoint to validate consensus parameters against
    let slot = &params.slot;
    let Some(slot_checkpoint) = get_slot_checkpoint(*slot)? else {
        msg!("[ConsensusProposalRewardV1] Error: Missing slot checkpoint {} from db", slot);
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
    if !vrf_proof.verify(params.burnt_public_key, &vrf_input) {
        msg!("[ConsensusProposalRewardV1] Error: eta VRF proof couldn't be verified");
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
        CONSENSUS_CONTRACT_ZKAS_PROPOSAL_REWARD_NS_V1.to_string(),
        vec![
            nullifier.inner(),
            *pub_x,
            *pub_y,
            *value_coords.x(),
            *value_coords.y(),
            *new_serial_coords.x(),
            *new_serial_coords.y(),
            *new_value_coords.x(),
            *new_value_coords.y(),
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

/// `process_instruction` function for `Consensus::ProposalRewardV1`
pub(crate) fn consensus_proposal_reward_process_instruction_v1(
    _cid: ContractId,
    call_idx: u32,
    calls: Vec<ContractCall>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx as usize];
    let params: ConsensusProposalRewardParamsV1 = deserialize(&self_.data[1..])?;

    // ===================================
    // Perform the actual state transition
    // ===================================

    msg!("[ConsensusProposalRewardV1] Validating anonymous inputs");
    let burnt_input = &params.burnt_input;
    let mint_input = &params.mint_input;
    let output = &params.output;

    // Only native token can be rewarded in a proposal
    let dark_token_commit = pedersen_commitment_base(DARK_TOKEN_ID.inner(), mint_input.token_blind);
    if burnt_input.token_commit != dark_token_commit || output.token_commit != dark_token_commit {
        msg!("[ConsensusProposalRewardV1] Error: Input used non-native token");
        return Err(MoneyError::StakeInputNonNativeToken.into())
    }

    // Verify value commits match
    let mut valcom_total = pallas::Point::identity();
    valcom_total += burnt_input.value_commit;
    valcom_total += pedersen_commitment_u64(REWARD, pallas::Scalar::zero());
    valcom_total -= mint_input.value_commit;
    if valcom_total != pallas::Point::identity() {
        msg!("[ConsensusProposalRewardV1] Error: Value commitments do not result in identity");
        return Err(MoneyError::ValueMismatch.into())
    }

    // Check previous call is consensus contract
    if call_idx == 0 {
        msg!("[ConsensusProposalRewardV1] Error: previous_call_idx will be out of bounds");
        return Err(MoneyError::SpendHookOutOfBounds.into())
    }

    let previous_call_idx = call_idx - 1;
    let previous = &calls[previous_call_idx as usize];
    if previous.contract_id.inner() != CONSENSUS_CONTRACT_ID.inner() {
        msg!("[ConsensusProposalRewardV1] Error: Previous contract call is not consensus contract");
        return Err(MoneyError::UnstakePreviousCallNotConsensusContract.into())
    }

    // Verify previous call corresponds to Consensus::ProposalBurnV1 (0x02)
    if previous.data[0] != 0x02 {
        msg!("[ConsensusProposalRewardV1] Error: Previous call function mismatch");
        return Err(MoneyError::PreviousCallFunctionMissmatch.into())
    }

    // Verify previous call input is the same as this calls StakeInput
    let previous_params: ConsensusProposalBurnParamsV1 = deserialize(&previous.data[1..])?;
    let previous_input = &previous_params.input;
    if previous_input != burnt_input || previous_params.public_key != params.burnt_public_key {
        msg!("[ConsensusProposalRewardV1] Error: Previous call input mismatch");
        return Err(MoneyError::PreviousCallInputMissmatch.into())
    }

    // If spend hook is set, check its correctness
    if previous_input.spend_hook != ZERO &&
        previous_input.spend_hook != CONSENSUS_CONTRACT_ID.inner()
    {
        msg!("[ConsensusProposalRewardV1] Error: Invoking contract call does not match spend hook in input");
        return Err(MoneyError::SpendHookMismatch.into())
    }

    // Check next call is consensus contract
    let next_call_idx = call_idx + 1;
    if next_call_idx >= calls.len() as u32 {
        msg!("[ConsensusProposalRewardV1] Error: next_call_idx out of bounds");
        return Err(MoneyError::SpendHookOutOfBounds.into())
    }

    let next = &calls[next_call_idx as usize];
    if next.contract_id.inner() != CONSENSUS_CONTRACT_ID.inner() {
        msg!("[ConsensusProposalRewardV1] Error: Next contract call is not consensus contract");
        return Err(MoneyError::StakeNextCallNotConsensusContract.into())
    }

    // Verify next call corresponds to Consensus::ProposalMintV1 (0x04)
    if next.data[0] != 0x04 {
        msg!("[ConsensusProposalRewardV1] Error: Next call function mismatch");
        return Err(MoneyError::NextCallFunctionMissmatch.into())
    }

    // Verify next call StakeInput is the same as this calls input
    let next_params: ConsensusProposalMintParamsV1 = deserialize(&next.data[1..])?;
    if mint_input != &next_params.input ||
        output != &next_params.output ||
        params.new_serial_commit != next_params.serial_commit
    {
        msg!("[ConsensusProposalRewardV1] Error: Next call input mismatch");
        return Err(MoneyError::NextCallInputMissmatch.into())
    }

    // Create a state update.
    let update = ConsensusProposalRewardUpdateV1 {};
    let mut update_data = vec![];
    update_data.write_u8(ConsensusFunction::ProposalRewardV1 as u8)?;
    update.encode(&mut update_data)?;

    Ok(update_data)
}

/// `process_update` function for `Consensus::ProposalRewardV1`
pub(crate) fn consensus_proposal_reward_process_update_v1(
    _cid: ContractId,
    _update: ConsensusProposalRewardUpdateV1,
) -> ContractResult {
    // This contract call doesn't produce any updates
    Ok(())
}
