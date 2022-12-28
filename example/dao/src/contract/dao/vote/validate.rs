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

use darkfi_sdk::crypto::{MerkleNode, Nullifier, PublicKey};
use darkfi_serial::{Encodable, SerialDecodable, SerialEncodable};
use log::error;
use pasta_curves::{
    arithmetic::CurveAffine,
    group::{Curve, Group},
    pallas,
};

use darkfi::{crypto::types::DrkCircuitField, Error as DarkFiError};

use crate::{
    contract::{dao, dao::State as DaoState, money, money::state::State as MoneyState},
    note::EncryptedNote2,
    util::{CallDataBase, StateRegistry, Transaction, UpdateBase},
};

#[derive(Debug, Clone, thiserror::Error)]
pub enum Error {
    #[error("Invalid proposal")]
    InvalidProposal,

    #[error("Voting with already spent coinage")]
    SpentCoin,

    #[error("Double voting")]
    DoubleVote,

    #[error("Invalid input merkle root")]
    InvalidInputMerkleRoot,

    #[error("DarkFi error: {0}")]
    DarkFiError(String),
}
type Result<T> = std::result::Result<T, Error>;

impl From<DarkFiError> for Error {
    fn from(err: DarkFiError) -> Self {
        Self::DarkFiError(err.to_string())
    }
}

#[derive(Clone, SerialEncodable, SerialDecodable)]
pub struct CallData {
    pub header: Header,
    pub inputs: Vec<Input>,
}

impl CallDataBase for CallData {
    fn zk_public_values(&self) -> Vec<(String, Vec<DrkCircuitField>)> {
        let mut zk_publics = Vec::new();
        let mut all_votes_commit = pallas::Point::identity();

        assert!(!self.inputs.is_empty(), "inputs length cannot be zero");
        for input in &self.inputs {
            all_votes_commit += input.vote_commit;
            let value_coords = input.vote_commit.to_affine().coordinates().unwrap();

            let (sig_x, sig_y) = input.signature_public.xy();

            zk_publics.push((
                "dao-vote-burn".to_string(),
                vec![
                    input.nullifier.inner(),
                    *value_coords.x(),
                    *value_coords.y(),
                    self.header.token_commit,
                    input.merkle_root.inner(),
                    sig_x,
                    sig_y,
                ],
            ));
        }

        let yes_vote_commit_coords = self.header.yes_vote_commit.to_affine().coordinates().unwrap();

        let vote_commit_coords = all_votes_commit.to_affine().coordinates().unwrap();

        zk_publics.push((
            "dao-vote-main".to_string(),
            vec![
                self.header.token_commit,
                self.header.proposal_bulla,
                *yes_vote_commit_coords.x(),
                *yes_vote_commit_coords.y(),
                *vote_commit_coords.x(),
                *vote_commit_coords.y(),
            ],
        ));

        zk_publics
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn signature_public_keys(&self) -> Vec<PublicKey> {
        let mut signature_public_keys = vec![];
        for input in self.inputs.clone() {
            signature_public_keys.push(input.signature_public);
        }
        signature_public_keys
    }

    fn encode_bytes(
        &self,
        mut writer: &mut dyn std::io::Write,
    ) -> std::result::Result<usize, std::io::Error> {
        self.encode(&mut writer)
    }
}

#[derive(Clone, SerialEncodable, SerialDecodable)]
pub struct Header {
    pub token_commit: pallas::Base,
    pub proposal_bulla: pallas::Base,
    pub yes_vote_commit: pallas::Point,
    pub enc_note: EncryptedNote2,
}

#[derive(Clone, SerialEncodable, SerialDecodable)]
pub struct Input {
    pub nullifier: Nullifier,
    pub vote_commit: pallas::Point,
    pub merkle_root: MerkleNode,
    pub signature_public: PublicKey,
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

    let dao_state = states.lookup::<DaoState>(*dao::CONTRACT_ID).unwrap();

    // Check proposal_bulla exists
    let votes_info = dao_state.lookup_proposal_votes(call_data.header.proposal_bulla);
    if votes_info.is_none() {
        return Err(Error::InvalidProposal)
    }
    let votes_info = votes_info.unwrap();

    // Check the merkle roots for the input coins are valid
    let mut vote_nulls = Vec::new();
    let mut all_vote_commit = pallas::Point::identity();
    for input in &call_data.inputs {
        let money_state = states.lookup::<MoneyState>(*money::CONTRACT_ID).unwrap();
        if !money_state.is_valid_merkle(&input.merkle_root) {
            return Err(Error::InvalidInputMerkleRoot)
        }

        if money_state.nullifier_exists(&input.nullifier) {
            return Err(Error::SpentCoin)
        }

        if votes_info.nullifier_exists(&input.nullifier) {
            return Err(Error::DoubleVote)
        }

        all_vote_commit += input.vote_commit;

        vote_nulls.push(input.nullifier);
    }

    Ok(Box::new(Update {
        proposal_bulla: call_data.header.proposal_bulla,
        vote_nulls,
        yes_vote_commit: call_data.header.yes_vote_commit,
        all_vote_commit,
    }))
}

#[derive(Clone)]
pub struct Update {
    proposal_bulla: pallas::Base,
    vote_nulls: Vec<Nullifier>,
    pub yes_vote_commit: pallas::Point,
    pub all_vote_commit: pallas::Point,
}

impl UpdateBase for Update {
    fn apply(mut self: Box<Self>, states: &mut StateRegistry) {
        let state = states.lookup_mut::<DaoState>(*dao::CONTRACT_ID).unwrap();
        let votes_info = state.lookup_proposal_votes_mut(self.proposal_bulla).unwrap();
        votes_info.yes_votes_commit += self.yes_vote_commit;
        votes_info.all_votes_commit += self.all_vote_commit;
        votes_info.vote_nulls.append(&mut self.vote_nulls);
    }
}
