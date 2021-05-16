use bellman::groth16;
use bls12_381::Bls12;
use std::fmt;

use crate::error::{Error, Result};
use crate::tx;

pub trait ProgramState {
    fn is_valid_cashier_public_key(&self, public: &jubjub::SubgroupPoint) -> bool;

    fn mint_pvk(&self) -> &groth16::PreparedVerifyingKey<Bls12>;
    fn spend_pvk(&self) -> &groth16::PreparedVerifyingKey<Bls12>;
}

pub struct StateUpdates {}

pub type VerifyResult<T> = std::result::Result<T, VerifyFailed>;

#[derive(Debug)]
pub enum VerifyFailed {
    InvalidCashierKey(usize),
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
    for (i, input) in tx.clear_inputs.iter().enumerate() {
        if !state.is_valid_cashier_public_key(&input.signature_public) {
            return Err(VerifyFailed::InvalidCashierKey(i));
        }
    }

    tx.verify(state.mint_pvk(), state.spend_pvk())?;

    /*
    // Check the public key in the clear inputs
    // It should be a valid public key for the cashier
    assert_eq!(tx.clear_inputs[0].signature_public, cashier_public);
    // Check the tx verifies correctly
    assert!(tx.verify(&mint_pvk, &spend_pvk));
    // Add the new coins to the merkle tree
    tree.append(Coin::new(tx.outputs[0].revealed.coin))
        .expect("append merkle");

    // Now for every new tx we receive, the wallets should iterate over all outputs
    // and try to decrypt the coin's note.
    // If they can successfully decrypt it, then it's a coin destined for us.

    // Try to decrypt output note
    let note = tx.outputs[0]
        .enc_note
        .decrypt(&secret)
        .expect("note should be destined for us");
    // This contains the secret attributes so we can spend the coin
    */
    Ok(StateUpdates {})
}
