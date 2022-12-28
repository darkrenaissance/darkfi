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

use std::any::{Any, TypeId};

use darkfi_sdk::{
    crypto::PublicKey,
    pasta::{
        arithmetic::CurveAffine,
        group::{ff::PrimeField, Curve, Group},
        pallas,
    },
};
use darkfi_serial::{Encodable, SerialDecodable, SerialEncodable};

use darkfi::{
    crypto::{coin::Coin, types::DrkCircuitField},
    Error as DarkFiError,
};

use crate::{
    contract::{dao, dao::CONTRACT_ID, money},
    util::{CallDataBase, StateRegistry, Transaction, UpdateBase},
};

type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, thiserror::Error)]
pub enum Error {
    #[error("DarkFi error: {0}")]
    DarkFiError(String),

    #[error("InvalidNumberOfFuncCalls")]
    InvalidNumberOfFuncCalls,

    #[error("InvalidIndex")]
    InvalidIndex,

    #[error("InvalidCallData")]
    InvalidCallData,

    #[error("InvalidNumberOfOutputs")]
    InvalidNumberOfOutputs,

    #[error("InvalidOutput")]
    InvalidOutput,

    #[error("InvalidValueCommit")]
    InvalidValueCommit,

    #[error("InvalidVoteCommit")]
    InvalidVoteCommit,
}

impl From<DarkFiError> for Error {
    fn from(err: DarkFiError) -> Self {
        Self::DarkFiError(err.to_string())
    }
}

#[derive(Clone, SerialEncodable, SerialDecodable)]
pub struct CallData {
    pub proposal: pallas::Base,
    pub coin_0: pallas::Base,
    pub coin_1: pallas::Base,
    pub yes_votes_commit: pallas::Point,
    pub all_votes_commit: pallas::Point,
    pub input_value_commit: pallas::Point,
}

impl CallDataBase for CallData {
    fn zk_public_values(&self) -> Vec<(String, Vec<DrkCircuitField>)> {
        let yes_votes_commit_coords = self.yes_votes_commit.to_affine().coordinates().unwrap();

        let all_votes_commit_coords = self.all_votes_commit.to_affine().coordinates().unwrap();

        let input_value_commit_coords = self.input_value_commit.to_affine().coordinates().unwrap();

        vec![(
            "dao-exec".to_string(),
            vec![
                self.proposal,
                self.coin_0,
                self.coin_1,
                *yes_votes_commit_coords.x(),
                *yes_votes_commit_coords.y(),
                *all_votes_commit_coords.x(),
                *all_votes_commit_coords.y(),
                *input_value_commit_coords.x(),
                *input_value_commit_coords.y(),
                *super::FUNC_ID,
                pallas::Base::from(0),
                pallas::Base::from(0),
            ],
        )]
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn signature_public_keys(&self) -> Vec<PublicKey> {
        vec![]
    }

    fn encode_bytes(
        &self,
        mut writer: &mut dyn std::io::Write,
    ) -> std::result::Result<usize, std::io::Error> {
        self.encode(&mut writer)
    }
}

pub fn state_transition(
    states: &StateRegistry,
    func_call_index: usize,
    parent_tx: &Transaction,
) -> Result<Box<dyn UpdateBase + Send>> {
    let func_call = &parent_tx.func_calls[func_call_index];
    let call_data = func_call.call_data.as_any();

    assert_eq!((*call_data).type_id(), TypeId::of::<CallData>());
    let call_data = call_data.downcast_ref::<CallData>();

    // This will be inside wasm so unwrap is fine.
    let call_data = call_data.unwrap();

    // Enforce tx has correct format:
    // 1. There should only be 2 func_call's
    if parent_tx.func_calls.len() != 2 {
        return Err(Error::InvalidNumberOfFuncCalls)
    }

    // 2. func_call_index == 1
    if func_call_index != 1 {
        return Err(Error::InvalidIndex)
    }

    // 3. First item should be a Money::transfer() calldata
    if parent_tx.func_calls[0].func_id != *money::transfer::FUNC_ID {
        return Err(Error::InvalidCallData)
    }

    let money_transfer_call_data = parent_tx.func_calls[0].call_data.as_any();
    let money_transfer_call_data =
        money_transfer_call_data.downcast_ref::<money::transfer::validate::CallData>();
    let money_transfer_call_data = money_transfer_call_data.unwrap();
    assert_eq!(
        money_transfer_call_data.type_id(),
        TypeId::of::<money::transfer::validate::CallData>()
    );

    // 4. Money::transfer() has exactly 2 outputs
    if money_transfer_call_data.outputs.len() != 2 {
        return Err(Error::InvalidNumberOfOutputs)
    }

    // Checks:
    // 1. Check both coins in Money::transfer() are equal to our coin_0, coin_1
    if money_transfer_call_data.outputs[0].revealed.coin != Coin(call_data.coin_0) {
        return Err(Error::InvalidOutput)
    }
    if money_transfer_call_data.outputs[1].revealed.coin != Coin(call_data.coin_1) {
        return Err(Error::InvalidOutput)
    }

    // 2. sum of Money::transfer() calldata input_value_commits == our input value commit
    let mut input_value_commits = pallas::Point::identity();
    for input in &money_transfer_call_data.inputs {
        input_value_commits += input.revealed.value_commit;
    }
    if input_value_commits != call_data.input_value_commit {
        return Err(Error::InvalidValueCommit)
    }

    // 3. get the ProposalVote from DAO::State
    let state =
        states.lookup::<dao::State>(*CONTRACT_ID).expect("Return type is not of type State");
    let proposal_votes = state.proposal_votes.get(&call_data.proposal.to_repr()).unwrap();

    // 4. check yes_votes_commit is the same as in ProposalVote
    if proposal_votes.yes_votes_commit != call_data.yes_votes_commit {
        return Err(Error::InvalidVoteCommit)
    }
    // 5. also check all_votes_commit
    if proposal_votes.all_votes_commit != call_data.all_votes_commit {
        return Err(Error::InvalidVoteCommit)
    }

    Ok(Box::new(Update { proposal: call_data.proposal }))
}

#[derive(Clone)]
pub struct Update {
    pub proposal: pallas::Base,
}

impl UpdateBase for Update {
    fn apply(self: Box<Self>, states: &mut StateRegistry) {
        let state = states
            .lookup_mut::<dao::State>(*CONTRACT_ID)
            .expect("Return type is not of type State");
        state.proposal_votes.remove(&self.proposal.to_repr()).unwrap();
    }
}
