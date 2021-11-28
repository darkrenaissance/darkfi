use log::debug;

use crate::{
    crypto::{
        coin::Coin, merkle_node2::MerkleNode, note::EncryptedNote, nullifier::Nullifier,
        proof::VerifyingKey, schnorr,
    },
    tx::Transaction,
};

pub trait ProgramState {
    fn is_valid_cashier_public_key(&self, public: &schnorr::PublicKey) -> bool;
    fn is_valid_merkle(&self, merkle: &MerkleNode) -> bool;
    fn nullifier_exists(&self, nullifier: &Nullifier) -> bool;

    fn mint_vk(&self) -> &VerifyingKey;
    fn spend_vk(&self) -> &VerifyingKey;
}

pub struct StateUpdate {
    pub nullifiers: Vec<Nullifier>,
    pub coins: Vec<Coin>,
    pub enc_notes: Vec<EncryptedNote>,
}

pub type VerifyResult<T> = std::result::Result<T, VerifyFailed>;

#[derive(Debug, Clone, thiserror::Error)]
pub enum VerifyFailed {
    #[error("Invalid cashier public key for clear input {0}")]
    InvalidCashierKey(usize),
    #[error("Invalid merkle root for input {0}")]
    InvalidMerkle(usize),
    #[error("Duplicate nullifier for input {0}")]
    DuplicateNullifier(usize),
    #[error("Spend proof for input {0}")]
    SpendProof(usize),
    #[error("Mint proof for input {0}")]
    MintProof(usize),
    #[error("Invalid signature for clear input {0}")]
    ClearInputSignature(usize),
    #[error("Invalid signature for input {0}")]
    InputSignature(usize),
    #[error("Money in does not match money out (value commits)")]
    MissingFunds,
    #[error("Assets don't match some inputs or outputs (token commits)")]
    AssetMismatch,
}

pub fn state_transition<S: ProgramState>(state: &S, tx: Transaction) -> VerifyResult<StateUpdate> {
    // Check deposits are legit

    debug!(target: "STATE TRANSITION", "iterate clear_inputs");

    for (i, input) in tx.clear_inputs.iter().enumerate() {
        // Check the public key in the clear inputs
        // It should be a valid public key for the cashier

        if !state.is_valid_cashier_public_key(&input.signature_public) {
            log::error!(target: "STATE TRANSITION", "Not valid cashier public key");
            return Err(VerifyFailed::InvalidCashierKey(i))
        }
    }

    debug!(target: "STATE TRANSITION", "iterate inputs");

    for (i, input) in tx.inputs.iter().enumerate() {
        let merkle = &input.revealed.merkle_root;

        // Merkle is used to know whether this is a coin that existed
        // in a previous state.
        if !state.is_valid_merkle(merkle) {
            return Err(VerifyFailed::InvalidMerkle(i))
        }

        // The nullifiers should not already exist
        // It is double spend protection.
        let nullifier = &input.revealed.nullifier;

        if state.nullifier_exists(nullifier) {
            return Err(VerifyFailed::DuplicateNullifier(i))
        }
    }

    debug!(target: "STATE TRANSITION", "Check the tx Verifies correctly");
    // Check the tx verifies correctly
    tx.verify(state.mint_vk(), state.spend_vk())?;

    let mut nullifiers = vec![];
    for input in tx.inputs {
        nullifiers.push(input.revealed.nullifier);
    }

    // Newly created coins for this tx
    let mut coins = vec![];
    let mut enc_notes = vec![];
    for output in tx.outputs {
        // Gather all the coins
        coins.push(Coin(output.revealed.coin));
        enc_notes.push(output.enc_note);
    }

    Ok(StateUpdate { nullifiers, coins, enc_notes })
}
