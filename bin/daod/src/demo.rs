#![allow(unused)]

use halo2_gadgets::poseidon::primitives as poseidon;
use halo2_proofs::circuit::Value;
use incrementalmerkletree::{bridgetree::BridgeTree, Tree};
use log::debug;
use pasta_curves::{
    arithmetic::CurveAffine,
    group::{ff::Field, Curve},
    pallas,
};
use rand::rngs::OsRng;
use std::{
    any::{Any, TypeId},
    collections::HashMap,
    time::Instant,
};

use darkfi::{
    crypto::{
        constants::MERKLE_DEPTH,
        keypair::{Keypair, PublicKey, SecretKey},
        merkle_node::MerkleNode,
        note::{EncryptedNote, Note},
        nullifier::Nullifier,
        proof::{ProvingKey, VerifyingKey},
        token_id::generate_id,
        types::DrkCircuitField,
        OwnCoin, OwnCoins, Proof,
    },
    node::state::{ProgramState, StateUpdate},
    tx::builder::{
        TransactionBuilder, TransactionBuilderClearInputInfo, TransactionBuilderInputInfo,
        TransactionBuilderOutputInfo,
    },
    util::NetworkName,
    zk::{
        circuit::{BurnContract, MintContract},
        vm::{Witness, ZkCircuit},
        vm_stack::empty_witnesses,
    },
    zkas::decoder::ZkBinary,
};

use crate::{dao_contract, money_contract};

// TODO: reenable unused vars warning and fix it
// TODO: strategize and cleanup Result/Error usage
// TODO: fix up code doc

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

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
    fn new() -> Self {
        Self { table: HashMap::new() }
    }

    fn add_contract(&mut self, key: String, bincode: ZkBinary, k_param: u32) {
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

    fn add_native(&mut self, key: String, proving_key: ProvingKey, verifying_key: VerifyingKey) {
        self.table.insert(
            key,
            ZkContractInfo::Native(ZkNativeContractInfo { proving_key, verifying_key }),
        );
    }

    pub fn lookup(&self, key: &String) -> Option<&ZkContractInfo> {
        self.table.get(key)
    }
}

macro_rules! zip {
    ($x: expr) => ($x);
    ($x: expr, $($y: expr), +) => (
        $x.iter().zip(
            zip!($($y), +))
    )
}

pub struct Transaction {
    pub func_calls: Vec<FuncCall>,
}

impl Transaction {
    /// Verify ZK contracts for the entire tx
    /// In real code, we could parallelize this for loop
    /// TODO: fix use of unwrap with Result type stuff
    fn zk_verify(&self, zk_bins: &ZkContractTable) {
        for func_call in &self.func_calls {
            let proofs_public_vals = &func_call.call_data.zk_public_values();
            let proofs_keys = &func_call.call_data.zk_proof_addrs();
            assert_eq!(proofs_public_vals.len(), proofs_keys.len());
            assert_eq!(proofs_keys.len(), func_call.proofs.len());
            for (key, (proof, public_vals)) in
                zip!(proofs_keys, &func_call.proofs, proofs_public_vals)
            {
                match zk_bins.lookup(key).unwrap() {
                    ZkContractInfo::Binary(info) => {
                        let verifying_key = &info.verifying_key;
                        proof.verify(&verifying_key, public_vals).expect("verify zk proof failed!");
                    }
                    ZkContractInfo::Native(info) => {
                        let verifying_key = &info.verifying_key;
                        proof.verify(&verifying_key, public_vals).expect("verify zk proof failed!");
                    }
                };
                debug!("zk_verify({}) passed", key);
            }
        }
    }
}

// These would normally be a hash or sth
type ContractId = String;
type FuncId = String;

pub struct FuncCall {
    pub contract_id: ContractId,
    pub func_id: FuncId,
    pub call_data: Box<dyn CallDataBase>,
    pub proofs: Vec<Proof>,
}

pub trait CallDataBase {
    // Public values for verifying the proofs
    // Needed so we can convert internal types so they can be used in Proof::verify()
    fn zk_public_values(&self) -> Vec<Vec<DrkCircuitField>>;

    // The zk contract ID needed to lookup in the table
    fn zk_proof_addrs(&self) -> Vec<String>;

    // For upcasting to CallData itself so it can be read in state_transition()
    fn as_any(&self) -> &dyn Any;
}

type GenericContractState = Box<dyn Any>;

pub struct StateRegistry {
    pub states: HashMap<ContractId, GenericContractState>,
}

impl StateRegistry {
    fn new() -> Self {
        Self { states: HashMap::new() }
    }

    fn register(&mut self, contract_id: ContractId, state: GenericContractState) {
        debug!(target: "StateRegistry::register()", "contract_id: {:?}", contract_id);
        self.states.insert(contract_id, state);
    }

    pub fn lookup_mut<'a, S: 'static>(&'a mut self, contract_id: &ContractId) -> Option<&'a mut S> {
        self.states.get_mut(contract_id).and_then(|state| state.downcast_mut())
    }

    pub fn lookup<'a, S: 'static>(&'a self, contract_id: &ContractId) -> Option<&'a S> {
        self.states.get(contract_id).and_then(|state| state.downcast_ref())
    }
}

pub async fn demo() -> Result<()> {
    // Money parameters
    let xdrk_supply = 1_000_000;
    let xdrk_token_id = pallas::Base::random(&mut OsRng);

    // Governance token parameters
    let gdrk_supply = 1_000_000;
    let gdrk_token_id = pallas::Base::random(&mut OsRng);

    // DAO parameters
    let dao_proposer_limit = 110;
    let dao_quorum = 110;
    let dao_approval_ratio = 2;

    // Lookup table for smart contract states
    let mut states = StateRegistry::new();

    // Initialize ZK binary table
    let mut zk_bins = ZkContractTable::new();
    let zk_dao_mint_bincode = include_bytes!("../proof/dao-mint.zk.bin");
    let zk_dao_mint_bin = ZkBinary::decode(zk_dao_mint_bincode)?;
    zk_bins.add_contract("dao-mint".to_string(), zk_dao_mint_bin, 13);

    {
        let start = Instant::now();
        let mint_pk = ProvingKey::build(11, &MintContract::default());
        debug!("Mint PK: [{:?}]", start.elapsed());
        let start = Instant::now();
        let burn_pk = ProvingKey::build(11, &BurnContract::default());
        debug!("Burn PK: [{:?}]", start.elapsed());
        let start = Instant::now();
        let mint_vk = VerifyingKey::build(11, &MintContract::default());
        debug!("Mint VK: [{:?}]", start.elapsed());
        let start = Instant::now();
        let burn_vk = VerifyingKey::build(11, &BurnContract::default());
        debug!("Burn VK: [{:?}]", start.elapsed());

        zk_bins.add_native("money-transfer-mint".to_string(), mint_pk, mint_vk);
        zk_bins.add_native("money-transfer-burn".to_string(), burn_pk, burn_vk);
    }

    // State for money contracts
    let cashier_signature_secret = SecretKey::random(&mut OsRng);
    let cashier_signature_public = PublicKey::from_secret(cashier_signature_secret);
    let faucet_signature_secret = SecretKey::random(&mut OsRng);
    let faucet_signature_public = PublicKey::from_secret(faucet_signature_secret);

    ///////////////////////////////////////////////////

    let money_state = Box::new(money_contract::state::State {
        tree: BridgeTree::<MerkleNode, MERKLE_DEPTH>::new(100),
        merkle_roots: vec![],
        nullifiers: vec![],
        cashier_signature_public,
        faucet_signature_public,
    });
    states.register("money_contract".to_string(), money_state);

    /////////////////////////////////////////////////////

    let dao_state = dao_contract::State::new();
    states.register("dao_contract".to_string(), dao_state);

    // For this demo lets create 10 random preexisting DAO bullas
    for _ in 0..10 {
        let bulla = pallas::Base::random(&mut OsRng);
    }

    /////////////////////////////////////////////////////
    ////// Create the DAO bulla
    /////////////////////////////////////////////////////

    //// Setup the DAO
    let dao_keypair = Keypair::random(&mut OsRng);
    let dao_bulla_blind = pallas::Base::random(&mut OsRng);

    // Create DAO mint tx
    let builder = dao_contract::mint::Builder::new(
        dao_proposer_limit,
        dao_quorum,
        dao_approval_ratio,
        gdrk_token_id,
        dao_keypair.public,
        dao_bulla_blind,
    );
    let func_call = builder.build(&zk_bins);

    let tx = Transaction { func_calls: vec![func_call] };

    for (idx, func_call) in tx.func_calls.iter().enumerate() {
        // So then the verifier will lookup the corresponding state_transition and apply
        // functions based off the func_id
        if func_call.func_id == "DAO::mint()" {
            debug!("dao_contract::mint::state_transition()");

            let update = dao_contract::mint::validate::state_transition(&states, idx, &tx)
                .expect("dao_contract::mint::validate::state_transition() failed!");
            dao_contract::mint::validate::apply(&mut states, update);
        }
    }

    tx.zk_verify(&zk_bins);

    ///////////////////////////////////////////////////
    //// Mint the initial supply of treasury token
    //// and send it all to the DAO directly
    ///////////////////////////////////////////////////

    let token_id = pallas::Base::random(&mut OsRng);
    let keypair = Keypair::random(&mut OsRng);

    let builder = money_contract::transfer::builder::Builder {
        clear_inputs: vec![money_contract::transfer::builder::BuilderClearInputInfo {
            value: 110,
            token_id,
            signature_secret: cashier_signature_secret,
        }],
        inputs: vec![],
        outputs: vec![money_contract::transfer::builder::BuilderOutputInfo {
            value: 110,
            token_id,
            public: keypair.public,
        }],
    };

    let func_call = builder.build(&zk_bins)?;

    let tx = Transaction { func_calls: vec![func_call] };

    //    let _note = tx.outputs[0].enc_note.decrypt(&keypair.secret)?;
    for (idx, func_call) in tx.func_calls.iter().enumerate() {
        // So then the verifier will lookup the corresponding state_transition and apply
        // functions based off the func_id
        if func_call.func_id == "money::transfer()" {
            debug!("money_contract::transfer::state_transition()");

            let update = money_contract::transfer::validate::state_transition(&states, idx, &tx)
                .expect("money_contract::state_transition() failed!");
            money_contract::transfer::validate::apply(&mut states, update);
        }
    }

    tx.zk_verify(&zk_bins);

    ///////////////////////////////////////////////////

    Ok(())
}
