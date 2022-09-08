use incrementalmerkletree::Tree;
use log::debug;
use pasta_curves::{
    arithmetic::CurveAffine,
    group::{
        ff::{Field, PrimeField},
        Curve, Group,
    },
    pallas,
};
use rand::rngs::OsRng;
use std::{
    any::{Any, TypeId},
    collections::HashMap,
    hash::Hasher,
    time::Instant,
};

use darkfi::{
    crypto::{
        keypair::{Keypair, PublicKey, SecretKey},
        proof::{ProvingKey, VerifyingKey},
        schnorr::{SchnorrPublic, SchnorrSecret, Signature},
        types::{DrkCircuitField, DrkSpendHook, DrkUserData, DrkValue},
        util::{pedersen_commitment_u64, poseidon_hash},
        Proof,
    },
    util::serial::Encodable,
    zk::{
        circuit::{BurnContract, MintContract},
        vm::ZkCircuit,
        vm_stack::empty_witnesses,
    },
    zkas::decoder::ZkBinary,
};
use std::sync::Arc;

use simplelog::{ColorChoice, LevelFilter, TermLogger, TerminalMode};
use url::Url;

#[derive(Eq, PartialEq)]
pub struct HashableBase(pub pallas::Base);

impl std::hash::Hash for HashableBase {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let bytes = self.0.to_repr();
        bytes.hash(state);
    }
}

pub struct ZkBinaryContractInfo {
    pub k_param: u32,
    pub bincode: ZkBinary,
    pub proving_key: ProvingKey,
    pub verifying_key: VerifyingKey,
}
pub struct ZkNativeContractInfo {
    pub proving_key: ProvingKey,
    pub verifying_key: VerifyingKey,
}

pub enum ZkContractInfo {
    Binary(ZkBinaryContractInfo),
    Native(ZkNativeContractInfo),
}

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
    pub signatures: Vec<Signature>,
}

impl Transaction {
    /// Verify ZK contracts for the entire tx
    /// In real code, we could parallelize this for loop
    /// TODO: fix use of unwrap with Result type stuff
    pub fn zk_verify(&self, zk_bins: &ZkContractTable) {
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
                        let verify_result = proof.verify(&verifying_key, public_vals);
                        assert!(verify_result.is_ok(), "verify proof[{}]='{}' failed", i, key);
                    }
                    ZkContractInfo::Native(info) => {
                        let verifying_key = &info.verifying_key;
                        let verify_result = proof.verify(&verifying_key, public_vals);
                        assert!(verify_result.is_ok(), "verify proof[{}]='{}' failed", i, key);
                    }
                };
                debug!(target: "demo", "zk_verify({}) passed [i={}]", key, i);
            }
        }
    }

    pub fn verify_sigs(&self) {
        let mut unsigned_tx_data = vec![];
        for (i, (func_call, signature)) in
            self.func_calls.iter().zip(self.signatures.clone()).enumerate()
        {
            func_call.encode(&mut unsigned_tx_data).expect("failed to encode data");
            let signature_pub_keys = func_call.call_data.signature_public_keys();
            for signature_pub_key in signature_pub_keys {
                let verify_result = signature_pub_key.verify(&unsigned_tx_data[..], &signature);
                assert!(verify_result, "verify sigs[{}] failed", i);
            }
            debug!(target: "demo", "verify_sigs({}) passed", i);
        }
    }
}

pub fn sign(signature_secrets: Vec<SecretKey>, func_calls: &Vec<FuncCall>) -> Vec<Signature> {
    let mut signatures = vec![];
    let mut unsigned_tx_data = vec![];
    for (_i, (signature_secret, func_call)) in
        signature_secrets.iter().zip(func_calls.iter()).enumerate()
    {
        func_call.encode(&mut unsigned_tx_data).expect("failed to encode data");
        let signature = signature_secret.sign(&unsigned_tx_data[..]);
        signatures.push(signature);
    }
    signatures
}

type ContractId = pallas::Base;
type FuncId = pallas::Base;

pub struct FuncCall {
    pub contract_id: ContractId,
    pub func_id: FuncId,
    pub call_data: Box<dyn CallDataBase>,
    pub proofs: Vec<Proof>,
}

impl Encodable for FuncCall {
    fn encode<W: std::io::Write>(&self, mut w: W) -> std::result::Result<usize, darkfi::Error> {
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
    ) -> std::result::Result<usize, darkfi::Error>;
}

type GenericContractState = Box<dyn Any>;

pub struct StateRegistry {
    pub states: HashMap<HashableBase, GenericContractState>,
}

impl StateRegistry {
    pub fn new() -> Self {
        Self { states: HashMap::new() }
    }

    pub fn register(&mut self, contract_id: ContractId, state: GenericContractState) {
        debug!(target: "StateRegistry::register()", "contract_id: {:?}", contract_id);
        self.states.insert(HashableBase(contract_id), state);
    }

    pub fn lookup_mut<'a, S: 'static>(&'a mut self, contract_id: ContractId) -> Option<&'a mut S> {
        self.states.get_mut(&HashableBase(contract_id)).and_then(|state| state.downcast_mut())
    }

    pub fn lookup<'a, S: 'static>(&'a self, contract_id: ContractId) -> Option<&'a S> {
        self.states.get(&HashableBase(contract_id)).and_then(|state| state.downcast_ref())
    }
}

pub trait UpdateBase {
    fn apply(self: Box<Self>, states: &mut StateRegistry);
}
