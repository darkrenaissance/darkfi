/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
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

use std::{any::Any, collections::HashMap};

use darkfi_sdk::crypto::{
    schnorr::{SchnorrPublic, SchnorrSecret, Signature},
    PublicKey, SecretKey,
};
use lazy_static::lazy_static;
use log::debug;
use pasta_curves::{
    group::ff::{Field, PrimeField},
    pallas,
};
use rand::rngs::OsRng;

use darkfi::{
    crypto::{
        proof::{ProvingKey, VerifyingKey},
        types::DrkCircuitField,
        Proof,
    },
    zk::{vm::ZkCircuit, vm_stack::empty_witnesses},
    zkas::decoder::ZkBinary,
};
use darkfi_serial::Encodable;

use crate::error::{DaoError, DaoResult};

// /// Parse pallas::Base from a base58-encoded string
// pub fn parse_b58(s: &str) -> std::result::Result<pallas::Base, darkfi::Error> {
//     let bytes = bs58::decode(s).into_vec()?;
//     if bytes.len() != 32 {
//         return Err(Error::ParseFailed("Failed parsing DrkTokenId from base58 string"))
//     }

//     let ret = pallas::Base::from_repr(bytes.try_into().unwrap());
//     if ret.is_some().unwrap_u8() == 1 {
//         return Ok(ret.unwrap())
//     }

//     Err(Error::ParseFailed("Failed parsing DrkTokenId from base58 string"))
// }

// The token of the DAO treasury.
lazy_static! {
    pub static ref DRK_ID: pallas::Base = pallas::Base::random(&mut OsRng);
}

// Governance tokens that are airdropped to users to operate the DAO.
lazy_static! {
    pub static ref GOV_ID: pallas::Base = pallas::Base::random(&mut OsRng);
}

#[derive(Clone)]
pub struct ZkBinaryContractInfo {
    pub k_param: u32,
    pub bincode: ZkBinary,
    pub proving_key: ProvingKey,
    pub verifying_key: VerifyingKey,
}

#[derive(Clone)]
pub struct ZkNativeContractInfo {
    pub proving_key: ProvingKey,
    pub verifying_key: VerifyingKey,
}

#[derive(Clone)]
pub enum ZkContractInfo {
    Binary(ZkBinaryContractInfo),
    Native(ZkNativeContractInfo),
}

#[derive(Clone)]
pub struct ZkContractTable {
    // Key will be a hash of zk binary contract on chain
    table: HashMap<String, ZkContractInfo>,
}

impl ZkContractTable {
    pub fn new() -> Self {
        Self { table: HashMap::new() }
    }

    pub fn add_contract(&mut self, key: String, bincode: ZkBinary, k_param: u32) {
        let witnesses = empty_witnesses(&bincode);
        let circuit = ZkCircuit::new(witnesses, bincode.clone());
        let proving_key = ProvingKey::build(k_param, &circuit);
        let verifying_key = VerifyingKey::build(k_param, &circuit);
        let info = ZkContractInfo::Binary(ZkBinaryContractInfo {
            k_param,
            bincode,
            proving_key,
            verifying_key,
        });
        self.table.insert(key, info);
    }

    pub fn add_native(
        &mut self,
        key: String,
        proving_key: ProvingKey,
        verifying_key: VerifyingKey,
    ) {
        self.table.insert(
            key,
            ZkContractInfo::Native(ZkNativeContractInfo { proving_key, verifying_key }),
        );
    }

    pub fn lookup(&self, key: &String) -> Option<&ZkContractInfo> {
        self.table.get(key)
    }
}

pub struct Transaction {
    pub func_calls: Vec<FuncCall>,
    pub signatures: Vec<Vec<Signature>>,
}

impl Transaction {
    /// Verify ZK contracts for the entire tx
    /// In real code, we could parallelize this for loop
    /// TODO: fix use of unwrap with Result type stuff
    pub fn zk_verify(&self, zk_bins: &ZkContractTable) -> DaoResult<()> {
        for func_call in &self.func_calls {
            let proofs_public_vals = &func_call.call_data.zk_public_values();

            assert_eq!(
                proofs_public_vals.len(),
                func_call.proofs.len(),
                "proof_public_vals.len()={} and func_call.proofs.len()={} do not match",
                proofs_public_vals.len(),
                func_call.proofs.len()
            );
            for (i, (proof, (key, public_vals))) in
                func_call.proofs.iter().zip(proofs_public_vals.iter()).enumerate()
            {
                match zk_bins.lookup(key).unwrap() {
                    ZkContractInfo::Binary(info) => {
                        let verifying_key = &info.verifying_key;
                        let verify_result = proof.verify(verifying_key, public_vals);
                        if verify_result.is_err() {
                            return Err(DaoError::VerifyProofFailed(i, key.to_string()))
                        }
                        //assert!(verify_result.is_ok(), "verify proof[{}]='{}' failed", i, key);
                    }
                    ZkContractInfo::Native(info) => {
                        let verifying_key = &info.verifying_key;
                        let verify_result = proof.verify(verifying_key, public_vals);
                        if verify_result.is_err() {
                            return Err(DaoError::VerifyProofFailed(i, key.to_string()))
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
        let mut unsigned_tx_data = vec![];
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
    }
}

pub fn sign(signature_secrets: Vec<SecretKey>, func_call: &FuncCall) -> Vec<Signature> {
    let mut signatures = vec![];
    let mut unsigned_tx_data = vec![];
    for signature_secret in signature_secrets {
        func_call.encode(&mut unsigned_tx_data).expect("failed to encode data");
        let signature = signature_secret.sign(&mut OsRng, &unsigned_tx_data[..]);
        signatures.push(signature);
    }
    signatures
}

type ContractId = pallas::Base;
type FuncId = pallas::Base;

pub struct FuncCall {
    pub contract_id: ContractId,
    pub func_id: FuncId,
    pub call_data: Box<dyn CallDataBase + Send + Sync>,
    pub proofs: Vec<Proof>,
}

impl Encodable for FuncCall {
    fn encode<W: std::io::Write>(&self, mut w: W) -> std::result::Result<usize, std::io::Error> {
        let mut len = 0;
        len += self.contract_id.encode(&mut w)?;
        len += self.func_id.encode(&mut w)?;
        len += self.proofs.encode(&mut w)?;
        len += self.call_data.encode_bytes(&mut w)?;
        Ok(len)
    }
}

pub trait CallDataBase {
    // Public values for verifying the proofs
    // Needed so we can convert internal types so they can be used in Proof::verify()
    fn zk_public_values(&self) -> Vec<(String, Vec<DrkCircuitField>)>;

    // For upcasting to CallData itself so it can be read in state_transition()
    fn as_any(&self) -> &dyn Any;

    // Public keys we will use to verify transaction signatures.
    fn signature_public_keys(&self) -> Vec<PublicKey>;

    fn encode_bytes(
        &self,
        writer: &mut dyn std::io::Write,
    ) -> std::result::Result<usize, std::io::Error>;
}

type GenericContractState = Box<dyn Any + Send>;

pub struct StateRegistry {
    pub states: HashMap<[u8; 32], GenericContractState>,
}

impl StateRegistry {
    pub fn new() -> Self {
        Self { states: HashMap::new() }
    }

    pub fn register(&mut self, contract_id: ContractId, state: GenericContractState) {
        debug!(target: "StateRegistry::register()", "contract_id: {:?}", contract_id);
        self.states.insert(contract_id.to_repr(), state);
    }

    pub fn lookup_mut<'a, S: 'static>(&'a mut self, contract_id: ContractId) -> Option<&'a mut S> {
        self.states.get_mut(&contract_id.to_repr()).and_then(|state| state.downcast_mut())
    }

    pub fn lookup<'a, S: 'static>(&'a self, contract_id: ContractId) -> Option<&'a S> {
        self.states.get(&contract_id.to_repr()).and_then(|state| state.downcast_ref())
    }
}

pub trait UpdateBase {
    fn apply(self: Box<Self>, states: &mut StateRegistry);
}
