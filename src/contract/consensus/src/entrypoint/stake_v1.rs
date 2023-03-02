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

use darkfi_sdk::{
    crypto::{ContractId, PublicKey},
    error::{ContractError, ContractResult},
    pasta::pallas,
    ContractCall,
};
use darkfi_serial::{deserialize, Encodable, WriteExt};

use crate::{
    model::{ConsensusStakeParamsV1, ConsensusStakeUpdateV1},
    ConsensusFunction,
};

/// `get_metadata` function for `Consensus::StakeV1`
pub(crate) fn consensus_stake_get_metadata_v1(
    _cid: ContractId,
    call_idx: u32,
    calls: Vec<ContractCall>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx as usize];
    let _params: ConsensusStakeParamsV1 = deserialize(&self_.data[1..])?;

    // Public inputs for the ZK proofs we have to verify
    let zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    // Public keys for the transaction signatures we have to verify
    let signature_pubkeys: Vec<PublicKey> = vec![];

    // TODO: implement

    // Serialize everything gathered and return it
    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    signature_pubkeys.encode(&mut metadata)?;

    Ok(metadata)
}

/// `process_instruction` function for `Consensus::StakeV1`
pub(crate) fn consensus_stake_process_instruction_v1(
    _cid: ContractId,
    _call_idx: u32,
    _calls: Vec<ContractCall>,
) -> Result<Vec<u8>, ContractError> {
    // TODO: implement

    // Create a state update.
    let update = ConsensusStakeUpdateV1 {};
    let mut update_data = vec![];
    update_data.write_u8(ConsensusFunction::StakeV1 as u8)?;
    update.encode(&mut update_data)?;

    Ok(update_data)
}

/// `process_update` function for `Consensus::StakeV1`
pub(crate) fn consensus_stake_process_update_v1(
    _cid: ContractId,
    _update: ConsensusStakeUpdateV1,
) -> ContractResult {
    // TODO: implement

    Ok(())
}
