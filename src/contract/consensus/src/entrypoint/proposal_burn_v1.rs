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
    error::MoneyError,
    model::{ConsensusUnstakeParamsV1, ConsensusUnstakeUpdateV1},
    CONSENSUS_CONTRACT_COIN_ROOTS_TREE, CONSENSUS_CONTRACT_NULLIFIERS_TREE,
    MONEY_CONTRACT_ZKAS_BURN_NS_V1,
};
use darkfi_sdk::{
    crypto::{
        pasta_prelude::*, pedersen_commitment_base, ContractId, PublicKey, CONSENSUS_CONTRACT_ID,
        DARK_TOKEN_ID,
    },
    db::{db_contains_key, db_lookup, db_set},
    error::{ContractError, ContractResult},
    msg,
    pasta::pallas,
    ContractCall,
};
use darkfi_serial::{deserialize, serialize, Encodable, WriteExt};

use crate::{
    model::{ConsensusRewardParamsV1, ZERO},
    ConsensusFunction,
};

/// `get_metadata` function for `Consensus::ProposalBurnV1`
pub(crate) fn consensus_proposal_burn_get_metadata_v1(
    _cid: ContractId,
    call_idx: u32,
    calls: Vec<ContractCall>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx as usize];
    let params: ConsensusUnstakeParamsV1 = deserialize(&self_.data[1..])?;

    // Public inputs for the ZK proofs we have to verify
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    // Public keys for the transaction signatures we have to verify
    let mut signature_pubkeys: Vec<PublicKey> = vec![];

    // Grab the pedersen commitments and signature pubkeys from the
    // anonymous input
    let input = &params.input;
    let value_coords = input.value_commit.to_affine().coordinates().unwrap();
    let token_coords = input.token_commit.to_affine().coordinates().unwrap();
    let (sig_x, sig_y) = input.signature_public.xy();

    // It is very important that these are in the same order as the
    // `constrain_instance` calls in the zkas code.
    // Otherwise verification will fail.
    zk_public_inputs.push((
        MONEY_CONTRACT_ZKAS_BURN_NS_V1.to_string(),
        vec![
            input.nullifier.inner(),
            *value_coords.x(),
            *value_coords.y(),
            *token_coords.x(),
            *token_coords.y(),
            input.merkle_root.inner(),
            input.user_data_enc,
            sig_x,
            sig_y,
        ],
    ));

    signature_pubkeys.push(input.signature_public);

    // Serialize everything gathered and return it
    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    signature_pubkeys.encode(&mut metadata)?;

    Ok(metadata)
}

/// `process_instruction` function for `Consensus::ProposalBurnV1`
pub(crate) fn consensus_proposal_burn_process_instruction_v1(
    cid: ContractId,
    call_idx: u32,
    calls: Vec<ContractCall>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx as usize];
    let params: ConsensusUnstakeParamsV1 = deserialize(&self_.data[1..])?;

    // Access the necessary databases where there is information to
    // validate this state transition.
    let nullifiers_db = db_lookup(cid, CONSENSUS_CONTRACT_NULLIFIERS_TREE)?;
    let coin_roots_db = db_lookup(cid, CONSENSUS_CONTRACT_COIN_ROOTS_TREE)?;

    // ===================================
    // Perform the actual state transition
    // ===================================

    msg!("[ConsensusProposalBurnV1] Validating anonymous input");
    let input = &params.input;

    // Only native token can be burned in a proposal
    if input.token_commit != pedersen_commitment_base(DARK_TOKEN_ID.inner(), params.token_blind) {
        msg!("[ConsensusProposalBurnV1] Error: Input used non-native token");
        return Err(MoneyError::StakeInputNonNativeToken.into())
    }

    // The Merkle root is used to know whether this is a coin that
    // existed in a previous state.
    if !db_contains_key(coin_roots_db, &serialize(&input.merkle_root))? {
        msg!("[ConsensusProposalBurnV1] Error: Merkle root not found in previous state");
        return Err(MoneyError::TransferMerkleRootNotFound.into())
    }

    // The nullifiers should not already exist. It is the double-spend protection.
    if db_contains_key(nullifiers_db, &serialize(&input.nullifier))? {
        msg!("[ConsensusProposalBurnV1] Error: Duplicate nullifier found");
        return Err(MoneyError::DuplicateNullifier.into())
    }

    // Check next call is consensus contract
    let next_call_idx = call_idx + 1;
    if next_call_idx >= calls.len() as u32 {
        msg!("[ConsensusProposalBurnV1] Error: next_call_idx out of bounds");
        return Err(MoneyError::SpendHookOutOfBounds.into())
    }

    let next = &calls[next_call_idx as usize];
    if next.contract_id.inner() != CONSENSUS_CONTRACT_ID.inner() {
        msg!("[ConsensusProposalBurnV1] Error: Next contract call is not consensus contract");
        return Err(MoneyError::StakeNextCallNotConsensusContract.into())
    }

    // Check if spend hook is set and its correctness
    if input.spend_hook == ZERO {
        msg!("[ConsensusProposalBurnV1] Error: Missing spend hook");
        return Err(MoneyError::StakeMissingSpendHook.into())
    }

    if input.spend_hook != CONSENSUS_CONTRACT_ID.inner() {
        msg!("[ConsensusProposalBurnV1] Error: Spend hook is not consensus contract");
        return Err(MoneyError::UnstakeSpendHookNotConsensusContract.into())
    }

    // Verify next call corresponds to Consensus::ProposalRewardV1 (0x02)
    if next.data[0] != 0x02 {
        msg!("[ConsensusProposalBurnV1] Error: Next call function mismatch");
        return Err(MoneyError::NextCallFunctionMissmatch.into())
    }

    // Verify next call StakeInput is the same as this calls input
    let next_params: ConsensusRewardParamsV1 = deserialize(&next.data[1..])?;
    if input != &next_params.unstake_input {
        msg!("[ConsensusProposalBurnV1] Error: Next call input mismatch");
        return Err(MoneyError::NextCallInputMissmatch.into())
    }

    // At this point the state transition has passed, so we create a state update
    let update = ConsensusUnstakeUpdateV1 { nullifier: input.nullifier };
    let mut update_data = vec![];
    update_data.write_u8(ConsensusFunction::UnstakeV1 as u8)?;
    update.encode(&mut update_data)?;

    // and return it
    Ok(update_data)
}

/// `process_update` function for `Consensus::ProposalBurnV1`
pub(crate) fn consensus_proposal_burn_process_update_v1(
    cid: ContractId,
    update: ConsensusUnstakeUpdateV1,
) -> ContractResult {
    // Grab all necessary db handles for where we want to write
    let nullifiers_db = db_lookup(cid, CONSENSUS_CONTRACT_NULLIFIERS_TREE)?;

    msg!("[ConsensusProposalBurnV1] Adding new nullifier to the set");
    db_set(nullifiers_db, &serialize(&update.nullifier), &[])?;

    Ok(())
}
