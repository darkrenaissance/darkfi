use std::{
    any::{Any, TypeId},
    io,
};

use incrementalmerkletree::{bridgetree::BridgeTree, Tree};
use log::{debug, error};

use pasta_curves::group::Group;

use darkfi::{
    crypto::{
        burn_proof::verify_burn_proof,
        coin::Coin,
        keypair::PublicKey,
        merkle_node::MerkleNode,
        mint_proof::verify_mint_proof,
        nullifier::Nullifier,
        proof::VerifyingKey,
        schnorr,
        schnorr::SchnorrPublic,
        types::{DrkCircuitField, DrkTokenId, DrkValueBlind, DrkValueCommit},
        util::{pedersen_commitment_base, pedersen_commitment_u64},
        BurnRevealedValues, MintRevealedValues, Proof,
    },
    node::state::ProgramState,
    util::serial::{Encodable, SerialDecodable, SerialEncodable, VarInt},
    Error as DarkFiError,
};

use crate::{
    demo::{CallDataBase, StateRegistry, Transaction},
    money_contract::state::State,
    note::EncryptedNote2,
};

const TARGET: &str = "money_contract::transfer::validate::state_transition()";

/// A struct representing a state update.
/// This gets applied on top of an existing state.
#[derive(Clone)]
pub struct Update {
    /// All nullifiers in a transaction
    pub nullifiers: Vec<Nullifier>,
    /// All coins in a transaction
    pub coins: Vec<Coin>,
    /// All encrypted notes in a transaction
    pub enc_notes: Vec<EncryptedNote2>,
}

pub fn apply(states: &mut StateRegistry, mut update: Update) {
    let state = states.lookup_mut::<State>(&"Money".to_string()).unwrap();

    // Extend our list of nullifiers with the ones from the update
    state.nullifiers.append(&mut update.nullifiers);

    //// Update merkle tree and witnesses
    for (coin, enc_note) in update.coins.into_iter().zip(update.enc_notes.into_iter()) {
        // Add the new coins to the Merkle tree
        let node = MerkleNode(coin.0);
        state.tree.append(&node);

        // Keep track of all Merkle roots that have existed
        state.merkle_roots.push(state.tree.root(0).unwrap());

        state.wallet_cache.try_decrypt_note(coin, enc_note, &mut state.tree);
    }
}

pub fn state_transition(
    states: &StateRegistry,
    func_call_index: usize,
    parent_tx: &Transaction,
) -> Result<Update> {
    // Check the public keys in the clear inputs to see if they're coming
    // from a valid cashier or faucet.
    debug!(target: TARGET, "Iterate clear_inputs");
    let func_call = &parent_tx.func_calls[func_call_index];
    let call_data = func_call.call_data.as_any();

    assert_eq!((&*call_data).type_id(), TypeId::of::<CallData>());
    let call_data = call_data.downcast_ref::<CallData>();

    // This will be inside wasm so unwrap is fine.
    let call_data = call_data.unwrap();

    let state =
        states.lookup::<State>(&"Money".to_string()).expect("Return type is not of type State");

    // Code goes here
    for (i, input) in call_data.clear_inputs.iter().enumerate() {
        let pk = &input.signature_public;
        // TODO: this depends on the token ID
        if !state.is_valid_cashier_public_key(pk) && !state.is_valid_faucet_public_key(pk) {
            error!(target: TARGET, "Invalid pubkey for clear input: {:?}", pk);
            return Err(Error::VerifyFailed(VerifyFailed::InvalidCashierOrFaucetKey(i)))
        }
    }

    // Nullifiers in the transaction
    let mut nullifiers = Vec::with_capacity(call_data.inputs.len());

    debug!(target: TARGET, "Iterate inputs");
    for (i, input) in call_data.inputs.iter().enumerate() {
        let merkle = &input.revealed.merkle_root;

        // The Merkle root is used to know whether this is a coin that
        // existed in a previous state.
        if !state.is_valid_merkle(merkle) {
            error!(target: TARGET, "Invalid Merkle root (input {})", i);
            debug!(target: TARGET, "root: {:?}", merkle);
            return Err(Error::VerifyFailed(VerifyFailed::InvalidMerkle(i)))
        }

        // The nullifiers should not already exist.
        // It is the double-spend protection.
        let nullifier = &input.revealed.nullifier;
        if state.nullifier_exists(nullifier) ||
            (1..nullifiers.len()).any(|i| nullifiers[i..].contains(&nullifiers[i - 1]))
        {
            error!(target: TARGET, "Duplicate nullifier found (input {})", i);
            debug!(target: TARGET, "nullifier: {:?}", nullifier);
            return Err(Error::VerifyFailed(VerifyFailed::NullifierExists(i)))
        }

        nullifiers.push(input.revealed.nullifier);
    }

    debug!(target: TARGET, "Verifying call data");
    match call_data.verify(&func_call.proofs) {
        Ok(()) => {
            debug!(target: TARGET, "Verified successfully")
        }
        Err(e) => {
            error!(target: TARGET, "Failed verifying zk proofs: {}", e);
            return Err(Error::VerifyFailed(VerifyFailed::ProofVerifyFailed(e.to_string())))
        }
    }

    // Newly created coins for this transaction
    let mut coins = Vec::with_capacity(call_data.outputs.len());
    let mut enc_notes = Vec::with_capacity(call_data.outputs.len());

    for output in &call_data.outputs {
        // Gather all the coins
        coins.push(output.revealed.coin);
        enc_notes.push(output.enc_note.clone());
    }

    Ok(Update { nullifiers, coins, enc_notes })
}

/// A DarkFi transaction
#[derive(Debug, Clone, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub struct CallData {
    /// Clear inputs
    pub clear_inputs: Vec<ClearInput>,
    /// Anonymous inputs
    pub inputs: Vec<Input>,
    /// Anonymous outputs
    pub outputs: Vec<Output>,
    /// Clear input signatures
    pub clear_signatures: Vec<schnorr::Signature>,
    /// Input signatures
    pub signatures: Vec<schnorr::Signature>,
}

impl CallDataBase for CallData {
    fn zk_public_values(&self) -> Vec<(String, Vec<DrkCircuitField>)> {
        let mut public_values = Vec::new();
        for input in &self.inputs {
            public_values.push(("money-transfer-burn".to_string(), input.revealed.make_outputs()));
        }
        for output in &self.outputs {
            public_values.push(("money-transfer-mint".to_string(), output.revealed.make_outputs()));
        }
        public_values
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
impl CallData {
    /// Verify the transaction
    pub fn verify(&self, proofs: &Vec<Proof>) -> VerifyResult<()> {
        //  must have minimum 1 clear or anon input, and 1 output
        if self.clear_inputs.len() + self.inputs.len() == 0 {
            error!("tx::verify(): Missing inputs");
            return Err(VerifyFailed::LackingInputs)
        }
        if self.outputs.len() == 0 {
            error!("tx::verify(): Missing outputs");
            return Err(VerifyFailed::LackingOutputs)
        }

        // Accumulator for the value commitments
        let mut valcom_total = DrkValueCommit::identity();

        // Add values from the clear inputs
        for input in &self.clear_inputs {
            valcom_total += pedersen_commitment_u64(input.value, input.value_blind);
        }
        // Add values from the inputs
        for input in &self.inputs {
            valcom_total += &input.revealed.value_commit;
        }
        // Subtract values from the outputs
        for output in &self.outputs {
            valcom_total -= &output.revealed.value_commit;
        }

        // If the accumulator is not back in its initial state,
        // there's a value mismatch.
        if valcom_total != DrkValueCommit::identity() {
            error!("tx::verify(): Missing funds");
            return Err(VerifyFailed::MissingFunds)
        }

        // Verify that the token commitments match
        if !self.verify_token_commitments() {
            error!("tx::verify(): Token ID mismatch");
            return Err(VerifyFailed::TokenMismatch)
        }

        // Verify the available signatures
        let mut unsigned_tx_data = vec![];
        self.clear_inputs.encode(&mut unsigned_tx_data)?;
        self.inputs.encode(&mut unsigned_tx_data)?;
        self.outputs.encode(&mut unsigned_tx_data)?;

        for (i, (input, signature)) in
            self.clear_inputs.iter().zip(self.clear_signatures.iter()).enumerate()
        {
            let public = &input.signature_public;
            if !public.verify(&unsigned_tx_data[..], signature) {
                error!("tx::verify(): Failed to verify Clear Input signature {}", i);
                return Err(VerifyFailed::ClearInputSignature(i))
            }
        }

        for (i, (input, signature)) in self.inputs.iter().zip(self.signatures.iter()).enumerate() {
            let public = &input.revealed.signature_public;
            if !public.verify(&unsigned_tx_data[..], signature) {
                error!("tx::verify(): Failed to verify Input signature {}", i);
                return Err(VerifyFailed::InputSignature(i))
            }
        }

        Ok(())
    }

    fn verify_token_commitments(&self) -> bool {
        assert_ne!(self.outputs.len(), 0);
        let token_commit_value = self.outputs[0].revealed.token_commit;

        let mut failed =
            self.inputs.iter().any(|input| input.revealed.token_commit != token_commit_value);

        failed = failed ||
            self.outputs.iter().any(|output| output.revealed.token_commit != token_commit_value);

        failed = failed ||
            self.clear_inputs.iter().any(|input| {
                pedersen_commitment_base(input.token_id, input.token_blind) != token_commit_value
            });
        !failed
    }
}

/// A transaction's clear input
#[derive(Debug, Clone, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub struct ClearInput {
    /// Input's value (amount)
    pub value: u64,
    /// Input's token ID
    pub token_id: DrkTokenId,
    /// Blinding factor for `value`
    pub value_blind: DrkValueBlind,
    /// Blinding factor for `token_id`
    pub token_blind: DrkValueBlind,
    /// Public key for the signature
    pub signature_public: PublicKey,
}

/// A transaction's anonymous input
#[derive(Debug, Clone, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub struct Input {
    /// Public inputs for the zero-knowledge proof
    pub revealed: BurnRevealedValues,
}

/// A transaction's anonymous output
#[derive(Debug, Clone, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub struct Output {
    /// Public inputs for the zero-knowledge proof
    pub revealed: MintRevealedValues,
    /// The encrypted note
    pub enc_note: EncryptedNote2,
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    VerifyFailed(#[from] VerifyFailed),

    #[error("DarkFi error: {0}")]
    DarkFiError(String),
}

/// Transaction verification errors
#[derive(Debug, Clone, thiserror::Error)]
pub enum VerifyFailed {
    #[error("Transaction has no inputs")]
    LackingInputs,

    #[error("Transaction has no outputs")]
    LackingOutputs,

    #[error("Invalid cashier/faucet public key for clear input {0}")]
    InvalidCashierOrFaucetKey(usize),

    #[error("Invalid Merkle root for input {0}")]
    InvalidMerkle(usize),

    #[error("Nullifier already exists for input {0}")]
    NullifierExists(usize),

    #[error("Invalid signature for input {0}")]
    InputSignature(usize),

    #[error("Invalid signature for clear input {0}")]
    ClearInputSignature(usize),

    #[error("Token commitments in inputs or outputs to not match")]
    TokenMismatch,

    #[error("Money in does not match money out (value commitments)")]
    MissingFunds,

    #[error("Mint proof verification failure for input {0}")]
    MintProof(usize),

    #[error("Burn proof verification failure for input {0}")]
    BurnProof(usize),

    #[error("Failed verifying zk proofs: {0}")]
    ProofVerifyFailed(String),

    #[error("Internal error: {0}")]
    InternalError(String),

    #[error("DarkFi error: {0}")]
    DarkFiError(String),
}

type Result<T> = std::result::Result<T, Error>;

impl From<Error> for VerifyFailed {
    fn from(err: Error) -> Self {
        Self::InternalError(err.to_string())
    }
}

impl From<DarkFiError> for VerifyFailed {
    fn from(err: DarkFiError) -> Self {
        Self::DarkFiError(err.to_string())
    }
}

impl From<DarkFiError> for Error {
    fn from(err: DarkFiError) -> Self {
        Self::DarkFiError(err.to_string())
    }
}
/// Result type used in transaction verifications
pub type VerifyResult<T> = std::result::Result<T, VerifyFailed>;
