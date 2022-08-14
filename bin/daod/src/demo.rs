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

use crate::{
    dao_contract::{
        mint::Builder,
        state::{apply, state_transition, DaoBulla, State},
    },
    money_contract,
};

/// The state machine, held in memory.
struct MemoryState {
    /// The entire Merkle tree state
    tree: BridgeTree<MerkleNode, MERKLE_DEPTH>,
    /// List of all previous and the current Merkle roots.
    /// This is the hashed value of all the children.
    merkle_roots: Vec<MerkleNode>,
    /// Nullifiers prevent double spending
    nullifiers: Vec<Nullifier>,
    /// Verifying key for the mint zk circuit.
    mint_vk: VerifyingKey,
    /// Verifying key for the burn zk circuit.
    burn_vk: VerifyingKey,

    /// Public key of the cashier
    cashier_signature_public: PublicKey,

    /// Public key of the faucet
    faucet_signature_public: PublicKey,
}

impl ProgramState for MemoryState {
    fn is_valid_cashier_public_key(&self, public: &PublicKey) -> bool {
        public == &self.cashier_signature_public
    }

    fn is_valid_faucet_public_key(&self, public: &PublicKey) -> bool {
        public == &self.faucet_signature_public
    }

    fn is_valid_merkle(&self, merkle_root: &MerkleNode) -> bool {
        self.merkle_roots.iter().any(|m| m == merkle_root)
    }

    fn nullifier_exists(&self, nullifier: &Nullifier) -> bool {
        self.nullifiers.iter().any(|n| n == nullifier)
    }

    fn mint_vk(&self) -> &VerifyingKey {
        &self.mint_vk
    }

    fn burn_vk(&self) -> &VerifyingKey {
        &self.burn_vk
    }
}

impl MemoryState {
    fn apply(&mut self, mut update: StateUpdate) {
        // Extend our list of nullifiers with the ones from the update
        self.nullifiers.append(&mut update.nullifiers);

        // Update merkle tree and witnesses
        for (coin, enc_note) in update.coins.into_iter().zip(update.enc_notes.into_iter()) {
            // Add the new coins to the Merkle tree
            let node = MerkleNode(coin.0);
            self.tree.append(&node);

            // Keep track of all Merkle roots that have existed
            self.merkle_roots.push(self.tree.root(0).unwrap());
        }
    }
}
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub struct ZkContractInfo {
    pub k_param: u32,
    pub bincode: ZkBinary,
    pub proving_key: ProvingKey,
    pub verifying_key: VerifyingKey,
}

pub struct ZkBinaryTable {
    // Key will be a hash of zk binary contract on chain
    table: HashMap<String, ZkContractInfo>,
}

impl ZkBinaryTable {
    fn new() -> Self {
        Self { table: HashMap::new() }
    }

    fn add_contract(&mut self, key: String, bincode: ZkBinary, k_param: u32) {
        let witnesses = empty_witnesses(&bincode);
        let circuit = ZkCircuit::new(witnesses, bincode.clone());
        let proving_key = ProvingKey::build(k_param, &circuit);
        let verifying_key = VerifyingKey::build(k_param, &circuit);
        let info = ZkContractInfo { k_param, bincode, proving_key, verifying_key };
        self.table.insert(key, info);
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
    /// TODO: what should this return? plonk error?
    /// Verify ZK contracts for the entire tx
    /// In real code, we could parallelize this for loop
    fn zk_verify(&self, zk_bins: &ZkBinaryTable) {
        for func_call in &self.func_calls {
            let proofs_public_vals = &func_call.call_data.zk_public_values();
            let proofs_keys = &func_call.call_data.zk_proof_addrs();
            assert_eq!(proofs_public_vals.len(), proofs_keys.len());
            assert_eq!(proofs_keys.len(), func_call.proofs.len());
            for (key, (proof, public_vals)) in
                zip!(proofs_keys, &func_call.proofs, proofs_public_vals)
            {
                let zk_info = zk_bins.lookup(key).unwrap();
                let verifying_key = &zk_info.verifying_key;
                proof.verify(&verifying_key, public_vals).expect("verify DAO::mint() failed!");
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
        self.states.insert(contract_id, state);
    }

    pub fn lookup<'a, S: 'static>(&'a mut self, contract_id: &ContractId) -> Option<&'a mut S> {
        self.states.get_mut(contract_id).and_then(|state| state.downcast_mut())
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
    let mut zk_bins = ZkBinaryTable::new();
    let zk_dao_mint_bincode = include_bytes!("../proof/dao-mint.zk.bin");
    let zk_dao_mint_bin = ZkBinary::decode(zk_dao_mint_bincode)?;
    zk_bins.add_contract("dao-mint".to_string(), zk_dao_mint_bin, 13);

    /////////////////////////////////////////////////

    /*
    TODO: The following money_contract behaviors are still unimplemented:

    [ ] money_contract/transfer/builder.rs.
        The mint proof is currently part of its outputs and CallData::proofs is an empty vector.
    [ ] CallDataBase
        Not fully implemented for money_contract/mint/mod::CallData.
    [ ] money_contract/state.rs
        State transition function is totally unimplemented.

    /////////////////////////////////////////////////

    // State for money contracts
    let cashier_signature_secret = SecretKey::random(&mut OsRng);
    let cashier_signature_public = PublicKey::from_secret(cashier_signature_secret);
    let faucet_signature_secret = SecretKey::random(&mut OsRng);
    let faucet_signature_public = PublicKey::from_secret(faucet_signature_secret);

    let start = Instant::now();
    let mint_vk = VerifyingKey::build(11, &MintContract::default());
    debug!("Mint VK: [{:?}]", start.elapsed());
    let start = Instant::now();
    let burn_vk = VerifyingKey::build(11, &BurnContract::default());
    debug!("Burn VK: [{:?}]", start.elapsed());

    let money_state = Box::new(MemoryState {
        tree: BridgeTree::<MerkleNode, MERKLE_DEPTH>::new(100),
        merkle_roots: vec![],
        nullifiers: vec![],
        mint_vk,
        burn_vk,
        cashier_signature_public,
        faucet_signature_public,
    });
    states.register("money_contract".to_string(), money_state);
    */

    /////////////////////////////////////////////////

    let dao_state = State::new();
    //let dao_state = State::new();
    states.register("dao_contract".to_string(), dao_state);

    // For this demo lets create 10 random preexisting DAO bullas
    for _ in 0..10 {
        let bulla = pallas::Base::random(&mut OsRng);
    }

    /////////////////////////////////////////////////
    // Create the DAO bulla
    /////////////////////////////////////////////////

    // Setup the DAO
    let dao_keypair = Keypair::random(&mut OsRng);
    let dao_bulla_blind = pallas::Base::random(&mut OsRng);

    // Create DAO mint tx
    let builder = Builder::new(
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

            let update = state_transition(&states, idx, &tx).unwrap();
            apply(&mut states, update);
        }
    }

    tx.zk_verify(&zk_bins);

    /////////////////////////////////////////////////

    /*
        let token_id = pallas::Base::random(&mut OsRng);

        let builder = TransactionBuilder {
            clear_inputs: vec![TransactionBuilderClearInputInfo {
                value: 110,
                token_id,
                signature_secret: cashier_signature_secret,
            }],
            inputs: vec![],
            outputs: vec![TransactionBuilderOutputInfo {
                value: 110,
                token_id,
                public: keypair.public,
            }],
        };

        let start = Instant::now();
        let mint_pk = ProvingKey::build(11, &MintContract::default());
        debug!("Mint PK: [{:?}]", start.elapsed());
        let start = Instant::now();
        let burn_pk = ProvingKey::build(11, &BurnContract::default());
        debug!("Burn PK: [{:?}]", start.elapsed());
        let tx = builder.build(&mint_pk, &burn_pk)?;

        tx.verify(&money_state.mint_vk, &money_state.burn_vk)?;

        let _note = tx.outputs[0].enc_note.decrypt(&keypair.secret)?;

        let update = state_transition(&money_state, tx)?;
        money_state.apply(update);

        // Now spend
        let owncoin = &money_state.own_coins[0];
        let note = &owncoin.note;
        let leaf_position = owncoin.leaf_position;
        let root = money_state.tree.root(0).unwrap();
        let merkle_path = money_state.tree.authentication_path(leaf_position, &root).unwrap();

        let builder = TransactionBuilder {
            clear_inputs: vec![],
            inputs: vec![TransactionBuilderInputInfo {
                leaf_position,
                merkle_path,
                secret: keypair.secret,
                note: note.clone(),
            }],
            outputs: vec![TransactionBuilderOutputInfo {
                value: 110,
                token_id,
                public: keypair.public,
            }],
        };

        let tx = builder.build(&mint_pk, &burn_pk)?;

        let update = state_transition(&money_state, tx)?;
        money_state.apply(update);
    */

    Ok(())
}
