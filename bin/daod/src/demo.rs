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

mod dao_contract {
    use pasta_curves::pallas;
    use std::any::Any;

    pub struct DaoBulla(pub pallas::Base);

    /// This DAO state is for all DAOs on the network. There should only be a single instance.
    pub struct State {
        dao_bullas: Vec<DaoBulla>,
    }

    impl State {
        pub fn new() -> Box<dyn Any> {
            Box::new(Self { dao_bullas: Vec::new() })
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
        use darkfi::crypto::{keypair::PublicKey, types::DrkCircuitField};
        use log::debug;
        use pasta_curves::pallas;
        use std::{
            any::{Any, TypeId},
            time::Instant,
        };

        use super::super::{CallDataBase, FuncCall, StateRegistry, Transaction};

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
            pub fn build(self) -> FuncCall {
                let call_data = CallData {};
                FuncCall {
                    contract_id: "DAO".to_string(),
                    func_id: "DAO::mint()".to_string(),
                    call_data: Box::new(call_data),
                    proofs: vec![],
                }
            }
        }

        pub struct CallData {}

        impl CallDataBase for CallData {
            fn public_inputs(&self) -> Vec<Vec<DrkCircuitField>> {
                vec![]
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
            Ok(Update {})
        }

        pub struct Update {}

        pub fn apply(states: &mut StateRegistry, update: Update) {}
    }
}

pub struct Transaction {
    func_calls: Vec<FuncCall>,
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
    // Public inputs for verifying the proofs
    // Needed so we can convert internal types so they can be used in Proof::verify()
    fn public_inputs(&self) -> Vec<Vec<DrkCircuitField>>;

    // For upcasting to CallData itself so it can be read in state_transition()
    fn as_any(&self) -> &dyn Any;
}

type GenericContractState = Box<dyn Any>;

pub struct StateRegistry {
    states: HashMap<ContractId, GenericContractState>,
}

impl StateRegistry {
    fn new() -> Self {
        Self { states: HashMap::new() }
    }

    fn register(&mut self, contract_id: ContractId, state: GenericContractState) {
        self.states.insert(contract_id, state);
    }

    // Lookup State
    fn lookup<S>(&self, contract_id: ContractId) -> Option<&S> {
        None
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
        let messages = [pallas::Base::random(&mut OsRng)];
        let bulla =
            poseidon::Hash::<_, poseidon::P128Pow5T3, poseidon::ConstantLength<1>, 3, 2>::init()
                .hash(messages);
    }

    /////////////////////////////////////////////////
    // Create the DAO bulla
    /////////////////////////////////////////////////

    // Setup the DAO
    let dao_keypair = Keypair::random(&mut OsRng);
    let dao_bulla_blind = pallas::Base::random(&mut OsRng);

    // !!!!!!! TEST !!!!!!!!!!!!!!!!!!!!!!!!!!!!!!

    let dao_proposer_limit__base = pallas::Base::from(110);
    let dao_quorum__base = pallas::Base::from(110);
    let dao_approval_ratio__base = pallas::Base::from(2);

    let dao_pubkey_coords = dao_keypair.public.0.to_affine().coordinates().unwrap();
    let dao_public_x = *dao_pubkey_coords.x();
    let dao_public_y = *dao_pubkey_coords.x();

    let messages = [
        dao_proposer_limit__base,
        //dao_quorum__base,
        //dao_approval_ratio__base,
        //gdrk_token_id,
        //dao_public_x,
        //dao_public_y,
        //dao_bulla_blind,
    ];
    let dao_bulla =
        poseidon::Hash::<_, poseidon::P128Pow5T3, poseidon::ConstantLength<1>, 3, 2>::init()
            .hash(messages);

    // Lets repeat this in ZK

    let bincode = include_bytes!("../proof/dao-mint.zk.bin");
    let zkbin = ZkBinary::decode(bincode)?;

    // ======
    // Prover
    // ======
    // Bigger k = more rows, but slower circuit
    // Number of rows is 2^k
    let k = 13;

    // Witness values
    let prover_witnesses = vec![
        Witness::Base(Value::known(dao_proposer_limit__base)),
        //Witness::Base(Value::known(dao_quorum__base)),
        //Witness::Base(Value::known(dao_approval_ratio__base)),
        //Witness::Base(Value::known(gdrk_token_id)),
        //Witness::Base(Value::known(dao_public_x)),
        //Witness::Base(Value::known(dao_public_y)),
        //Witness::Base(Value::known(dao_bulla_blind)),
    ];

    // Create the public inputs
    let public_inputs = vec![dao_bulla];

    // Create the circuit
    let circuit = ZkCircuit::new(prover_witnesses, zkbin.clone());

    let now = std::time::Instant::now();
    let proving_key = ProvingKey::build(k, &circuit);
    println!("DAO::mint() ProvingKey built [{} s]", now.elapsed().as_secs_f64());
    let now = std::time::Instant::now();
    let proof = Proof::create(&proving_key, &[circuit], &public_inputs, &mut OsRng)
        .expect("DAO::mint() proving error!");
    println!("DAO::mint() Proof created [{} s]", now.elapsed().as_secs_f64());

    // ========
    // Verifier
    // ========

    // Construct empty witnesses
    let verifier_witnesses = empty_witnesses(&zkbin);

    // Create the circuit
    let circuit = ZkCircuit::new(verifier_witnesses, zkbin);

    let now = std::time::Instant::now();
    let verifying_key = VerifyingKey::build(k, &circuit);
    println!("DAO::mint() VerifyingKey built [{} s]", now.elapsed().as_secs_f64());
    let now = std::time::Instant::now();
    proof.verify(&verifying_key, &public_inputs).expect("verify DAO::mint() failed!");
    println!("DAO::mint() proof verify [{} s]", now.elapsed().as_secs_f64());

    // !!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!

    // Create DAO mint tx
    let builder = dao_contract::mint::Builder::new(
        dao_proposer_limit,
        dao_quorum,
        dao_approval_ratio,
        gdrk_token_id,
        dao_keypair.public,
        dao_bulla_blind,
    );
    let func_call = builder.build();

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
