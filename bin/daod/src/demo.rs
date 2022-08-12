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
    node::state::{state_transition, ProgramState, StateUpdate},
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

/// The state machine, held in memory.
struct MemoryState {
    /// The entire Merkle tree state
    tree: BridgeTree<MerkleNode, MERKLE_DEPTH>,
    /// List of all previous and the current Merkle roots.
    /// This is the hashed value of all the children.
    merkle_roots: Vec<MerkleNode>,
    /// Nullifiers prevent double spending
    nullifiers: Vec<Nullifier>,
    /// All received coins
    // NOTE: We need maybe a flag to keep track of which ones are
    // spent. Maybe the spend field links to a tx hash:input index.
    // We should also keep track of the tx hash:output index where
    // this coin was received.
    own_coins: OwnCoins,
    /// Verifying key for the mint zk circuit.
    mint_vk: VerifyingKey,
    /// Verifying key for the burn zk circuit.
    burn_vk: VerifyingKey,

    /// Public key of the cashier
    cashier_signature_public: PublicKey,

    /// Public key of the faucet
    faucet_signature_public: PublicKey,

    /// List of all our secret keys
    secrets: Vec<SecretKey>,
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

            // If it's our own coin, witness it and append to the vector.
            if let Some((note, secret)) = self.try_decrypt_note(enc_note) {
                let leaf_position = self.tree.witness().unwrap();
                let nullifier = Nullifier::new(secret, note.serial);
                let own_coin = OwnCoin { coin, note, secret, nullifier, leaf_position };
                self.own_coins.push(own_coin);
            }
        }
    }

    fn try_decrypt_note(&self, ciphertext: EncryptedNote) -> Option<(Note, SecretKey)> {
        // Loop through all our secret keys...
        for secret in &self.secrets {
            // .. attempt to decrypt the note ...
            if let Ok(note) = ciphertext.decrypt(secret) {
                // ... and return the decrypted note for this coin.
                return Some((note, *secret))
            }
        }

        // We weren't able to decrypt the note with any of our keys.
        None
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

mod dao_contract {
    use pasta_curves::pallas;
    use std::any::Any;

    #[derive(Clone)]
    pub struct DaoBulla(pub pallas::Base);

    /// This DAO state is for all DAOs on the network. There should only be a single instance.
    pub struct State {
        dao_bullas: Vec<DaoBulla>,
    }

    impl State {
        pub fn new() -> Box<dyn Any> {
            Box::new(Self { dao_bullas: Vec::new() })
        }

        pub fn add_bulla(&mut self, bulla: DaoBulla) {
            self.dao_bullas.push(bulla);
        }
    }

    /// This is an anonymous contract function that mutates the internal DAO state.
    ///
    /// Corresponds to `mint(proposer_limit, quorum, approval_ratio, dao_pubkey, dao_blind)`
    ///
    /// The prover creates a `Builder`, which then constructs the `Tx` that the verifier can
    /// check using `state_transition()`.
    ///
    /// # Arguments
    ///
    /// * `proposer_limit` - Number of governance tokens that holder must possess in order to
    ///   propose a new vote.
    /// * `quorum` - Number of minimum votes that must be met for a proposal to pass.
    /// * `approval_ratio` - Ratio of winning to total votes for a proposal to pass.
    /// * `dao_pubkey` - Public key of the DAO for permissioned access. This can also be
    ///   shared publicly if you want a full decentralized DAO.
    /// * `dao_blind` - Blinding factor for the DAO bulla.
    ///
    /// # Example
    ///
    /// ```rust
    /// let dao_proposer_limit = 110;
    /// let dao_quorum = 110;
    /// let dao_approval_ratio = 2;
    ///
    /// let builder = dao_contract::Mint::Builder(
    ///     dao_proposer_limit,
    ///     dao_quorum,
    ///     dao_approval_ratio,
    ///     gov_token_id,
    ///     dao_pubkey,
    ///     dao_blind
    /// );
    /// let tx = builder.build();
    /// ```
    pub mod mint {
        use darkfi::{
            crypto::{keypair::PublicKey, proof::ProvingKey, types::DrkCircuitField, Proof},
            zk::vm::{Witness, ZkCircuit},
        };
        use halo2_gadgets::poseidon::primitives as poseidon;
        use halo2_proofs::circuit::Value;
        use log::debug;
        use pasta_curves::{
            arithmetic::CurveAffine,
            group::{ff::Field, Curve},
            pallas,
        };
        use rand::rngs::OsRng;
        use std::{
            any::{Any, TypeId},
            time::Instant,
        };

        use super::{
            super::{CallDataBase, FuncCall, StateRegistry, Transaction, ZkBinaryTable},
            DaoBulla,
        };

        pub struct Builder {
            dao_proposer_limit: u64,
            dao_quorum: u64,
            dao_approval_ratio: u64,
            gov_token_id: pallas::Base,
            dao_pubkey: PublicKey,
            dao_bulla_blind: pallas::Base,
        }

        impl Builder {
            pub fn new(
                dao_proposer_limit: u64,
                dao_quorum: u64,
                dao_approval_ratio: u64,
                gov_token_id: pallas::Base,
                dao_pubkey: PublicKey,
                dao_bulla_blind: pallas::Base,
            ) -> Self {
                Self {
                    dao_proposer_limit,
                    dao_quorum,
                    dao_approval_ratio,
                    gov_token_id,
                    dao_pubkey,
                    dao_bulla_blind,
                }
            }

            /// Consumes self, and produces the function call
            pub fn build(self, zk_bins: &ZkBinaryTable) -> FuncCall {
                // Dao bulla
                let dao_proposer_limit = pallas::Base::from(self.dao_proposer_limit);
                let dao_quorum = pallas::Base::from(self.dao_quorum);
                let dao_approval_ratio = pallas::Base::from(self.dao_approval_ratio);

                let dao_pubkey_coords = self.dao_pubkey.0.to_affine().coordinates().unwrap();
                let dao_public_x = *dao_pubkey_coords.x();
                let dao_public_y = *dao_pubkey_coords.x();

                let messages = [
                    dao_proposer_limit,
                    dao_quorum,
                    dao_approval_ratio,
                    self.gov_token_id,
                    dao_public_x,
                    dao_public_y,
                    self.dao_bulla_blind,
                    // @tmp-workaround
                    self.dao_bulla_blind,
                ];
                let dao_bulla = poseidon::Hash::<
                    _,
                    poseidon::P128Pow5T3,
                    poseidon::ConstantLength<8>,
                    3,
                    2,
                >::init()
                .hash(messages);
                let dao_bulla = DaoBulla(dao_bulla);

                // Now create the mint proof
                let zk_info = zk_bins.lookup(&"dao-mint".to_string()).unwrap();
                let zk_bin = zk_info.bincode.clone();
                let prover_witnesses = vec![
                    Witness::Base(Value::known(dao_proposer_limit)),
                    Witness::Base(Value::known(dao_quorum)),
                    Witness::Base(Value::known(dao_approval_ratio)),
                    Witness::Base(Value::known(self.gov_token_id)),
                    Witness::Base(Value::known(dao_public_x)),
                    Witness::Base(Value::known(dao_public_y)),
                    Witness::Base(Value::known(self.dao_bulla_blind)),
                ];
                let public_inputs = vec![dao_bulla.0];
                let circuit = ZkCircuit::new(prover_witnesses, zk_bin);

                let proving_key = &zk_info.proving_key;
                let mint_proof = Proof::create(proving_key, &[circuit], &public_inputs, &mut OsRng)
                    .expect("DAO::mint() proving error!");

                // [x] 1. move proving key to zkbins table (and k value)
                // [x] 2. do verification of zk proofs in main code
                // [ ] 3. implement apply(update) function

                // Return call data
                let call_data = CallData { dao_bulla };
                FuncCall {
                    contract_id: "DAO".to_string(),
                    func_id: "DAO::mint()".to_string(),
                    call_data: Box::new(call_data),
                    proofs: vec![mint_proof],
                }
            }
        }

        pub struct CallData {
            dao_bulla: DaoBulla,
        }

        impl CallDataBase for CallData {
            fn zk_public_values(&self) -> Vec<Vec<DrkCircuitField>> {
                vec![vec![self.dao_bulla.0]]
            }

            fn zk_proof_addrs(&self) -> Vec<String> {
                vec!["dao-mint".to_string()]
            }

            fn as_any(&self) -> &dyn Any {
                self
            }
        }

        #[derive(Debug, Clone, thiserror::Error)]
        pub enum Error {
            #[error("Malformed packet")]
            MalformedPacket,
        }
        type Result<T> = std::result::Result<T, Error>;

        pub fn state_transition(
            states: &StateRegistry,
            func_call_index: usize,
            parent_tx: &Transaction,
        ) -> Result<Update> {
            let func_call = &parent_tx.func_calls[func_call_index];
            let call_data = func_call.call_data.as_any();

            assert_eq!((&*call_data).type_id(), TypeId::of::<CallData>());
            let call_data = call_data.downcast_ref::<CallData>();

            // This will be inside wasm so unwrap is fine.
            let call_data = call_data.unwrap();

            // Code goes here

            Ok(Update { dao_bulla: call_data.dao_bulla.clone() })
        }

        pub struct Update {
            dao_bulla: DaoBulla,
        }

        pub fn apply(states: &mut StateRegistry, update: Update) {
            // Lookup dao_contract state from registry
            ////// FIXME /////////
            let state = states.states.get_mut(&"dao_contract".to_string()).unwrap();
            let state = state.downcast_mut::<super::State>().unwrap();
            //////////////////////
            // Add dao_bulla to state.dao_bullas
            state.add_bulla(update.dao_bulla);
        }
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
    func_calls: Vec<FuncCall>,
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
    contract_id: ContractId,
    func_id: FuncId,
    call_data: Box<dyn CallDataBase>,
    proofs: Vec<Proof>,
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

    /*
    fn lookup<'a, S>(&'a mut self, contract_id: &ContractId) -> Option<StateRefWrapper<'a, S>> {
        match self.states.get_mut(contract_id) {
            Some(state) => {
                let ptr = state.downcast_mut::<S>();
                match ptr {
                    Some(ptr) => Some(StateRefWrapper { _mut: state, ptr }),
                    None => None,
                }
            }
            None => None,
        }
    }
    */
}

/*
struct StateRefWrapper<'a, S> {
    _mut: &'a mut Box<dyn Any>,
    ptr: &'a mut S,
}
*/

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

        // TODO: this should not be here.
        // We should separate wallet functionality from the State completely
        let keypair = Keypair::random(&mut OsRng);

        let money_state = Box::new(MemoryState {
            tree: BridgeTree::<MerkleNode, MERKLE_DEPTH>::new(100),
            merkle_roots: vec![],
            nullifiers: vec![],
            own_coins: vec![],
            mint_vk,
            burn_vk,
            cashier_signature_public,
            faucet_signature_public,
            secrets: vec![keypair.secret],
        });
        states.register("MoneyContract".to_string(), money_state);
    */

    /////////////////////////////////////////////////

    let dao_state = dao_contract::State::new();
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

            let update = dao_contract::mint::state_transition(&states, idx, &tx).unwrap();
            dao_contract::mint::apply(&mut states, update);
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
