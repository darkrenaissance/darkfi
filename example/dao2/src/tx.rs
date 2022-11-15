/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2022 Dyne.org foundation
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

use darkfi::{crypto::Proof, Result, VerifyFailed::ProofVerifyFailed};
use darkfi_sdk::{
    crypto::{
        schnorr::{SchnorrPublic, Signature},
        PublicKey,
    },
    pasta::pallas,
    tx::ContractCall,
};
use darkfi_serial::Encodable;
use log::debug;

use crate::{
    contract::{dao, example, money},
    note::EncryptedNote2,
    schema::WalletCache,
    util::{sign, StateRegistry, ZkContractInfo, ZkContractTable},
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
    pub fn zk_verify(
        &self,
        zk_bins: &ZkContractTable,
        zkpub_table: &Vec<Vec<(String, Vec<pallas::Base>)>>,
    ) -> Result<()> {
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

            for (i, (proof, (key, public_vals))) in proofs.iter().zip(pubvals.iter()).enumerate() {
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

    pub fn verify_sigs(&self, sigpub_table: &Vec<Vec<pallas::Point>>) -> Result<()> {
        let mut tx_data = Vec::new();
        self.calls.encode(&mut tx_data)?;
        self.proofs.encode(&mut tx_data)?;
        // TODO: Hash it and use the hash as the signing data
        // let sighash = ...

        for (i, (signatures, signature_public_keys)) in
            self.signatures.iter().zip(sigpub_table.iter()).enumerate()
        {
            for (signature_pub_key, signature) in signature_public_keys.iter().zip(signatures) {
                let signature_pub_key = PublicKey::from(*signature_pub_key);
                let verify_result = signature_pub_key.verify(&tx_data[..], &signature);
                assert!(verify_result, "verify sigs[{}] failed", i);
            }
            debug!(target: "demo", "verify_sigs({}) passed", i);
        }
        Ok(())
    }
}
