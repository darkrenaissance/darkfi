use log::debug;
use darkfi::{crypto::{schnorr::Signature, Proof}, Result, VerifyFailed::ProofVerifyFailed};
use darkfi_sdk::{tx::ContractCall, pasta::pallas};

use crate::{
    contract::{dao, example, money},
    note::EncryptedNote2,
    schema::WalletCache,
    util::{sign, StateRegistry, ZkContractTable, ZkContractInfo},
};

macro_rules! zip {
    ($x: expr) => ($x);
    ($x: expr, $($y: expr), +) => (
        $x.iter().zip(
            zip!($($y), +))
    )
}

pub struct Transaction {
    pub calls: Vec<ContractCall>,
    pub proofs: Vec<Vec<Proof>>,
    pub signatures: Vec<Vec<Signature>>,
}

impl Transaction {
    /// Verify ZK contracts for the entire tx
    /// In real code, we could parallelize this for loop
    /// TODO: fix use of unwrap with Result type stuff
    pub fn zk_verify(&self, zk_bins: &ZkContractTable, zkpub_table: &Vec<Vec<(String, Vec<pallas::Base>)>>) -> Result<()> {
        assert_eq!(
            self.calls.len(),
            self.proofs.len(),
            "calls.len()={} and proofs.len()={} do not match",
            self.calls.len(),
            self.proofs.len()
        );
        assert_eq!(
            self.calls.len(),
            zkpub_table.len(),
            "calls.len()={} and zkpub_table.len()={} do not match",
            self.calls.len(),
            zkpub_table.len()
        );
        for (call, (proofs, pubvals)) in zip!(self.calls, self.proofs, zkpub_table) {
            assert_eq!(
                proofs.len(),
                pubvals.len(),
                "proofs.len()={} and pubvals.len()={} do not match",
                proofs.len(),
                pubvals.len()
            );

            for (i, (proof, (key, public_vals))) in
                proofs.iter().zip(pubvals.iter()).enumerate()
            {
                match zk_bins.lookup(key).unwrap() {
                    ZkContractInfo::Binary(info) => {
                        let verifying_key = &info.verifying_key;
                        let verify_result = proof.verify(verifying_key, public_vals);
                        if verify_result.is_err() {
                            return Err(ProofVerifyFailed(key.to_string()).into())
                        }
                        //assert!(verify_result.is_ok(), "verify proof[{}]='{}' failed", i, key);
                    }
                    ZkContractInfo::Native(info) => {
                        let verifying_key = &info.verifying_key;
                        let verify_result = proof.verify(verifying_key, public_vals);
                        if verify_result.is_err() {
                            return Err(ProofVerifyFailed(key.to_string()).into())
                        }
                        //assert!(verify_result.is_ok(), "verify proof[{}]='{}' failed", i, key);
                    }
                };
                debug!(target: "demo", "zk_verify({}) passed [i={}]", key, i);
            }
        }
        Ok(())
    }

    pub fn verify_sigs(&self) {
        //let mut unsigned_tx_data = vec![];
        /*
        for (i, (func_call, signatures)) in
            self.func_calls.iter().zip(self.signatures.clone()).enumerate()
        {
            func_call.encode(&mut unsigned_tx_data).expect("failed to encode data");
            let signature_pub_keys = func_call.call_data.signature_public_keys();
            for (signature_pub_key, signature) in signature_pub_keys.iter().zip(signatures) {
                let verify_result = signature_pub_key.verify(&unsigned_tx_data[..], &signature);
                assert!(verify_result, "verify sigs[{}] failed", i);
            }
            debug!(target: "demo", "verify_sigs({}) passed", i);
        }
        */
    }
}
