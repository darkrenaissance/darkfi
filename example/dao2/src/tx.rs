use darkfi::crypto::{schnorr::Signature, Proof};
use darkfi_sdk::tx::ContractCall;

pub struct Transaction {
    pub calls: Vec<ContractCall>,
    pub proofs: Vec<Vec<Proof>>,
    pub signatures: Vec<Vec<Signature>>,
}
