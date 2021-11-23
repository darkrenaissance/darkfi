use std::fmt;

use log::debug;

use crate::{
    crypto::{coin::Coin, note::EncryptedNote, nullifier::Nullifier, proof::VerifyingKey, schnorr},
    tx::Transaction,
    types::{DrkCoinBlind, DrkPublicKey, DrkSecretKey, DrkSerial, DrkTokenId, DrkValueBlind},
};

pub trait ProgramState {
    fn is_valid_cashier_public_key(&self, public: &schnorr::PublicKey) -> bool;
    // TODO: fn is_valid_merkle(&self, merkle: &MerkleNode) -> bool;
    fn nullifier_exists(&self, nullifier: &Nullifier) -> bool;

    fn mint_pvk(&self) -> &VerifyingKey;
    fn spend_pvk(&self) -> &VerifyingKey;
}

pub struct StateUpdate {
    pub nullifiers: Vec<Nullifier>,
    pub coins: Vec<Coin>,
    pub enc_notes: Vec<EncryptedNote>,
}

pub type VerifyResult<T> = std::result::Result<T, VerifyFailed>;

#[derive(Debug)]
pub enum VerifyFailed {
    InvalidCashierKey(usize),
    InvalidMerkle(usize),
    DuplicateNullifier(usize),
    SpendProof(usize),
    MintProof(usize),
    ClearInputSignature(usize),
    InputSignature(usize),
    MissingFunds,
    AssetMismatch,
}

impl std::error::Error for VerifyFailed {}

impl fmt::Display for VerifyFailed {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::fmt::Result {
        match *self {
            VerifyFailed::InvalidCashierKey(i) => {
                write!(f, "Invalid cashier public key for clear input {}", i)
            }
            VerifyFailed::InvalidMerkle(i) => {
                write!(f, "Invalid merkle root for input {}", i)
            }
            VerifyFailed::DuplicateNullifier(i) => {
                write!(f, "Duplicate nullifier for input {}", i)
            }
            VerifyFailed::SpendProof(i) => write!(f, "Spend proof for input {}", i),
            VerifyFailed::MintProof(i) => write!(f, "Mint proof for input {}", i),
            VerifyFailed::ClearInputSignature(i) => {
                write!(f, "Invalid signature for clear input {}", i)
            }
            VerifyFailed::InputSignature(i) => write!(f, "Invalid signature for input {}", i),
            VerifyFailed::MissingFunds => {
                f.write_str("Money in does not match money out (value commits)")
            }
            VerifyFailed::AssetMismatch => {
                f.write_str("Assets don't match some inputs or outputs (token commits)")
            }
        }
    }
}

pub fn state_transition<S: ProgramState>(
    state: &async_std::sync::MutexGuard<S>,
    tx: Transaction,
) -> VerifyResult<StateUpdate> {
    // Check deposits are legit

    debug!(target: "STATE TRANSITION", "iterate clear_inputs");

    for (i, input) in tx.clear_inputs.iter().enumerate() {
        // Check the public key in the clear inputs
        // It should be a valid public key for the cashier

        if !state.is_valid_cashier_public_key(&input.signature_public) {
            log::error!(target: "STATE TRANSITION", "Not valid cashier public key");
            return Err(VerifyFailed::InvalidCashierKey(i));
        }
    }

    debug!(target: "STATE TRANSITION", "iterate inputs");

    for (i, input) in tx.inputs.iter().enumerate() {
        // TODO: Check merkle roots
        //let merkle = &input.revealed.merkle_root;

        // Merkle is used to know whether this is a coin that existed
        // in a previous state.
        // if !state.is_valid_merkle(merkle) {
        // return Err(VerifyFailed::InvalidMerkle(i));
        // }

        // The nullifiers should not already exist
        // It is double spend protection.
        let nullifier = &input.revealed.nullifier;

        if state.nullifier_exists(nullifier) {
            return Err(VerifyFailed::DuplicateNullifier(i));
        }
    }

    debug!(target: "STATE TRANSITION", "Check the tx Verifies correctly");
    // Check the tx verifies correctly
    tx.verify(state.mint_pvk(), state.spend_pvk())?;

    let mut nullifiers = vec![];
    for input in tx.inputs {
        nullifiers.push(input.revealed.nullifier);
    }

    // Newly created coins for this tx
    let mut coins = vec![];
    let mut enc_notes = vec![];
    for output in tx.outputs {
        // Gather all the coins
        coins.push(Coin(output.revealed.coin.clone()));
        enc_notes.push(output.enc_note);
    }

    Ok(StateUpdate {
        nullifiers,
        coins,
        enc_notes,
    })
}
