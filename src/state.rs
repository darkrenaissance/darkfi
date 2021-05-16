use bellman::groth16;
use bls12_381::Bls12;
use std::fmt;

use crate::crypto::note::{EncryptedNote, Note};
use crate::error::{Error, Result};
use crate::tx;

pub trait ProgramState {
    fn is_valid_cashier_public_key(&self, public: &jubjub::SubgroupPoint) -> bool;
    fn is_valid_merkle(&self, merkle: &bls12_381::Scalar) -> bool;
    fn nullifier_exists(&self, nullifier: &[u8; 32]) -> bool;

    fn mint_pvk(&self) -> &groth16::PreparedVerifyingKey<Bls12>;
    fn spend_pvk(&self) -> &groth16::PreparedVerifyingKey<Bls12>;

    fn try_decrypt_note(&self, ciphertext: EncryptedNote) -> Option<Note>;
}

pub struct StateUpdates {
    pub coins: Vec<([u8; 32], Option<Note>)>,
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
        }
    }
}

pub fn state_transition<S: ProgramState>(
    state: &S,
    tx: tx::Transaction,
) -> VerifyResult<StateUpdates> {
    // Check deposits are legit
    for (i, input) in tx.clear_inputs.iter().enumerate() {
        // Check the public key in the clear inputs
        // It should be a valid public key for the cashier
        if !state.is_valid_cashier_public_key(&input.signature_public) {
            return Err(VerifyFailed::InvalidCashierKey(i));
        }
    }

    for (i, input) in tx.inputs.iter().enumerate() {
        // Check merkle roots
        let merkle = &input.revealed.merkle_root;

        if !state.is_valid_merkle(merkle) {
            return Err(VerifyFailed::InvalidMerkle(i));
        }

        // Nullifiers don't already exist
        let nullifier = &input.revealed.nullifier;

        if state.nullifier_exists(nullifier) {
            return Err(VerifyFailed::DuplicateNullifier(i));
        }
    }

    // Check the tx verifies correctly
    tx.verify(state.mint_pvk(), state.spend_pvk())?;

    // Newly created coins for this tx
    let mut coins = vec![];
    for output in tx.outputs {
        // Any coins destined for this wallet?
        // Try to decrypt the ciphertext for this coin.
        // If successful then it belongs to us.
        // This contains the secret attributes so we can spend the coin
        let note = state.try_decrypt_note(output.enc_note);

        // Gather all the coins
        coins.push((output.revealed.coin, note));
    }

    Ok(StateUpdates { coins })
}
