use std::any::TypeId;

use incrementalmerkletree::{bridgetree::BridgeTree, Tree};
use log::{debug, error};

use darkfi::{
    crypto::{coin::Coin, merkle_node::MerkleNode, note::EncryptedNote, nullifier::Nullifier},
    node::state::ProgramState,
};

use crate::{
    demo::{StateRegistry, Transaction},
    money_contract::{state::State, transfer::CallData},
};

/// A struct representing a state update.
/// This gets applied on top of an existing state.
#[derive(Clone)]
pub struct Update {
    /// All nullifiers in a transaction
    pub nullifiers: Vec<Nullifier>,
    /// All coins in a transaction
    pub coins: Vec<Coin>,
    /// All encrypted notes in a transaction
    pub enc_notes: Vec<EncryptedNote>,
}

pub fn apply(states: &mut StateRegistry, mut update: Update) {
    let state = states.lookup_mut::<State>(&"mint_contract".to_string()).unwrap();

    // Extend our list of nullifiers with the ones from the update
    state.nullifiers.append(&mut update.nullifiers);

    //// Update merkle tree and witnesses
    for (coin, enc_note) in update.coins.into_iter().zip(update.enc_notes.into_iter()) {
        // Add the new coins to the Merkle tree
        let node = MerkleNode(coin.0);
        state.tree.append(&node);

        // Keep track of all Merkle roots that have existed
        state.merkle_roots.push(state.tree.root(0).unwrap());
    }
}

pub fn state_transition(
    states: &StateRegistry,
    func_call_index: usize,
    parent_tx: &Transaction,
) -> Result<Update> {
    // Check the public keys in the clear inputs to see if they're coming
    // from a valid cashier or faucet.
    debug!(target: "money_contract::mint::validate::state_transition", "Iterate clear_inputs");
    let func_call = &parent_tx.func_calls[func_call_index];
    let call_data = func_call.call_data.as_any();

    assert_eq!((&*call_data).type_id(), TypeId::of::<CallData>());
    let call_data = call_data.downcast_ref::<CallData>();

    // This will be inside wasm so unwrap is fine.
    let call_data = call_data.unwrap();

    let state = states.lookup::<State>(&"mint_contract".to_string()).unwrap();

    // Code goes here
    for (i, input) in call_data.clear_inputs.iter().enumerate() {
        let pk = &input.signature_public;
        // TODO: this depends on the token ID
        if !state.is_valid_cashier_public_key(pk) && !state.is_valid_faucet_public_key(pk) {
            error!(target: "money_contract::mint::validate::state_transition", "Invalid pubkey for clear input: {:?}", pk);
            return Err(Error::VerifyFailed(VerifyFailed::InvalidCashierOrFaucetKey(i)))
        }
    }

    // Nullifiers in the transaction
    let mut nullifiers = Vec::with_capacity(call_data.inputs.len());

    debug!(target: "money_contract::mint::validate::state_transition", "Iterate inputs");
    for (i, input) in call_data.inputs.iter().enumerate() {
        let merkle = &input.revealed.merkle_root;

        // The Merkle root is used to know whether this is a coin that
        // existed in a previous state.
        if !state.is_valid_merkle(merkle) {
            error!(target: "money_contract::mint::validate::state_transition", "Invalid Merkle root (input {})", i);
            debug!(target: "money_contract::mint::validate::state_transition", "root: {:?}", merkle);
            return Err(Error::VerifyFailed(VerifyFailed::InvalidMerkle(i)))
        }

        // The nullifiers should not already exist.
        // It is the double-spend protection.
        let nullifier = &input.revealed.nullifier;
        if state.nullifier_exists(nullifier) ||
            (1..nullifiers.len()).any(|i| nullifiers[i..].contains(&nullifiers[i - 1]))
        {
            error!(target: "money_contract::mint::validate::state_transition", "Duplicate nullifier found (input {})", i);
            debug!(target: "money_contract::mint::validate::state_transition", "nullifier: {:?}", nullifier);
            return Err(Error::VerifyFailed(VerifyFailed::NullifierExists(i)))
        }

        nullifiers.push(input.revealed.nullifier);
    }

    debug!(target: "money_contract::mint::validate::state_transition", "Verifying zk proofs");
    match call_data.verify(state.mint_vk(), state.burn_vk()) {
        Ok(()) => {
            debug!(target: "money_contract::mint::validate::state_transition", "Verified successfully")
        }
        Err(e) => {
            error!(target: "money_contract::mint::validate::state_transition", "Failed verifying zk proofs: {}", e);
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

#[derive(Debug, Clone, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    VerifyFailed(#[from] VerifyFailed),
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
}

type Result<T> = std::result::Result<T, Error>;

impl From<Error> for VerifyFailed {
    fn from(err: Error) -> Self {
        Self::InternalError(err.to_string())
    }
}
