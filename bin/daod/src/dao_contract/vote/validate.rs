use darkfi::{
    crypto::{
        keypair::PublicKey, merkle_node::MerkleNode, nullifier::Nullifier, schnorr,
        schnorr::SchnorrPublic, types::DrkCircuitField,
    },
    util::serial::{Encodable, SerialDecodable, SerialEncodable},
    Error as DarkFiError,
};
use log::error;
use pasta_curves::{
    arithmetic::CurveAffine,
    group::{Curve, Group},
    pallas,
};
use std::any::{Any, TypeId};

use crate::{
    dao_contract::State as DaoState,
    demo::{CallDataBase, StateRegistry, Transaction, UpdateBase},
    money_contract::state::State as MoneyState,
    note::EncryptedNote2,
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

    #[error("Signature verification failed")]
    SignatureVerifyFailed,

    #[error("DarkFi error: {0}")]
    DarkFiError(String),
}
type Result<T> = std::result::Result<T, Error>;

impl From<DarkFiError> for Error {
    fn from(err: DarkFiError) -> Self {
        Self::DarkFiError(err.to_string())
    }
}

pub struct CallData {
    pub header: Header,
    pub inputs: Vec<Input>,
    pub signatures: Vec<schnorr::Signature>,
}

impl CallDataBase for CallData {
    fn zk_public_values(&self) -> Vec<(String, Vec<DrkCircuitField>)> {
        let mut zk_publics = Vec::new();
        let mut total_value_commit = pallas::Point::identity();

        assert!(self.inputs.len() > 0, "inputs length cannot be zero");
        for input in &self.inputs {
            total_value_commit += input.value_commit;
            let value_coords = input.value_commit.to_affine().coordinates().unwrap();

            let sigpub_coords = input.signature_public.0.to_affine().coordinates().unwrap();

            zk_publics.push((
                "dao-vote-burn".to_string(),
                vec![
                    input.nullifier.0,
                    *value_coords.x(),
                    *value_coords.y(),
                    self.header.token_commit,
                    input.merkle_root.0,
                    *sigpub_coords.x(),
                    *sigpub_coords.y(),
                ],
            ));
        }

        let vote_commit_coords = self.header.vote_commit.to_affine().coordinates().unwrap();

        let value_commit_coords = total_value_commit.to_affine().coordinates().unwrap();

        zk_publics.push((
            "dao-vote-main".to_string(),
            vec![
                self.header.token_commit,
                self.header.proposal_bulla,
                *vote_commit_coords.x(),
                *vote_commit_coords.y(),
                *value_commit_coords.x(),
                *value_commit_coords.y(),
            ],
        ));

        zk_publics
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[derive(Clone, SerialEncodable, SerialDecodable)]
pub struct Header {
    pub token_commit: pallas::Base,
    pub proposal_bulla: pallas::Base,
    pub vote_commit: pallas::Point,
    pub enc_note: EncryptedNote2,
}

#[derive(Clone, SerialEncodable, SerialDecodable)]
pub struct Input {
    pub nullifier: Nullifier,
    pub value_commit: pallas::Point,
    pub merkle_root: MerkleNode,
    pub signature_public: PublicKey,
}

pub fn state_transition(
    states: &StateRegistry,
    func_call_index: usize,
    parent_tx: &Transaction,
) -> Result<Box<dyn UpdateBase>> {
    let func_call = &parent_tx.func_calls[func_call_index];
    let call_data = func_call.call_data.as_any();

    assert_eq!((&*call_data).type_id(), TypeId::of::<CallData>());
    let call_data = call_data.downcast_ref::<CallData>();

    // This will be inside wasm so unwrap is fine.
    let call_data = call_data.unwrap();

    let dao_state = states.lookup::<DaoState>(&"DAO".to_string()).unwrap();

    // Check proposal_bulla exists
    let votes_info = dao_state.lookup_proposal_votes(call_data.header.proposal_bulla);
    if votes_info.is_none() {
        return Err(Error::InvalidProposal)
    }
    let votes_info = votes_info.unwrap();

    // Check the merkle roots for the input coins are valid
    let mut vote_nulls = Vec::new();
    let mut total_value_commit = pallas::Point::identity();
    for input in &call_data.inputs {
        let money_state = states.lookup::<MoneyState>(&"Money".to_string()).unwrap();
        if !money_state.is_valid_merkle(&input.merkle_root) {
            return Err(Error::InvalidInputMerkleRoot)
        }

        if money_state.nullifier_exists(&input.nullifier) {
            return Err(Error::SpentCoin)
        }

        if votes_info.nullifier_exists(&input.nullifier) {
            return Err(Error::DoubleVote)
        }

        total_value_commit += input.value_commit;

        vote_nulls.push(input.nullifier);
    }

    // Verify the available signatures
    let mut unsigned_tx_data = vec![];
    call_data.header.encode(&mut unsigned_tx_data).expect("failed to encode data");
    call_data.inputs.encode(&mut unsigned_tx_data).expect("failed to encode inputs");
    func_call.proofs.encode(&mut unsigned_tx_data).expect("failed to encode proofs");

    //debug!("unsigned_tx_data: {:?}", unsigned_tx_data);

    for (_i, (input, signature)) in
        call_data.inputs.iter().zip(call_data.signatures.iter()).enumerate()
    {
        let public = &input.signature_public;
        if !public.verify(&unsigned_tx_data[..], signature) {
            return Err(Error::SignatureVerifyFailed)
        }
    }

    Ok(Box::new(Update {
        proposal_bulla: call_data.header.proposal_bulla,
        vote_nulls,
        vote_commit: call_data.header.vote_commit,
        value_commit: total_value_commit,
    }))
}

#[derive(Clone)]
pub struct Update {
    proposal_bulla: pallas::Base,
    vote_nulls: Vec<Nullifier>,
    pub vote_commit: pallas::Point,
    pub value_commit: pallas::Point,
}

impl UpdateBase for Update {
    fn apply(mut self: Box<Self>, states: &mut StateRegistry) {
        let state = states.lookup_mut::<DaoState>(&"DAO".to_string()).unwrap();
        let votes_info = state.lookup_proposal_votes_mut(self.proposal_bulla).unwrap();
        votes_info.vote_commits += self.vote_commit;
        votes_info.value_commits += self.value_commit;
        votes_info.vote_nulls.append(&mut self.vote_nulls);
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}
