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
        types::{DrkCircuitField, DrkSpendHook, DrkUserData, DrkValue},
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

use crate::{dao_contract, money_contract, util::poseidon_hash};

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
    debug!(target: "demo", "Loading dao-mint.zk");
    let zk_dao_mint_bincode = include_bytes!("../proof/dao-mint.zk.bin");
    let zk_dao_mint_bin = ZkBinary::decode(zk_dao_mint_bincode)?;
    zk_bins.add_contract("dao-mint".to_string(), zk_dao_mint_bin, 13);

    debug!(target: "demo", "Loading money-transfer contracts");
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
    debug!(target: "demo", "Loading dao-propose-main.zk");
    let zk_dao_propose_main_bincode = include_bytes!("../proof/dao-propose-main.zk.bin");
    let zk_dao_propose_main_bin = ZkBinary::decode(zk_dao_propose_main_bincode)?;
    zk_bins.add_contract("dao-propose-main".to_string(), zk_dao_propose_main_bin, 13);
    debug!(target: "demo", "Loading dao-propose-burn.zk");
    let zk_dao_propose_burn_bincode = include_bytes!("../proof/dao-propose-burn.zk.bin");
    let zk_dao_propose_burn_bin = ZkBinary::decode(zk_dao_propose_burn_bincode)?;
    zk_bins.add_contract("dao-propose-burn".to_string(), zk_dao_propose_burn_bin, 13);

    // State for money contracts
    let cashier_signature_secret = SecretKey::random(&mut OsRng);
    let cashier_signature_public = PublicKey::from_secret(cashier_signature_secret);
    let faucet_signature_secret = SecretKey::random(&mut OsRng);
    let faucet_signature_public = PublicKey::from_secret(faucet_signature_secret);

    ///////////////////////////////////////////////////

    let money_state =
        money_contract::state::State::new(cashier_signature_public, faucet_signature_public);
    states.register("Money".to_string(), money_state);

    /////////////////////////////////////////////////////

    let dao_state = dao_contract::State::new();
    states.register("DAO".to_string(), dao_state);

    // For this demo lets create 10 random preexisting DAO bullas
    for _ in 0..10 {
        let bulla = pallas::Base::random(&mut OsRng);
    }

    /////////////////////////////////////////////////////
    ////// Create the DAO bulla
    /////////////////////////////////////////////////////
    debug!(target: "demo", "Stage 1. Creating DAO bulla");

    //// Wallet

    //// Setup the DAO
    let dao_keypair = Keypair::random(&mut OsRng);
    let dao_bulla_blind = pallas::Base::random(&mut OsRng);

    // Create DAO mint tx
    let builder = dao_contract::mint::wallet::Builder::new(
        dao_proposer_limit,
        dao_quorum,
        dao_approval_ratio,
        gdrk_token_id,
        dao_keypair.public,
        dao_bulla_blind,
    );
    let func_call = builder.build(&zk_bins);

    let tx = Transaction { func_calls: vec![func_call] };

    //// Validator

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

    // Wallet stuff

    // In your wallet, wait until you see the tx confirmed before doing anything below
    // So for example keep track of tx hash
    //assert_eq!(tx.hash(), tx_hash);

    // We need to witness() the value in our local merkle tree
    // Must be called as soon as this DAO bulla is added to the state
    let dao_leaf_position = {
        let state = states.lookup_mut::<dao_contract::State>(&"DAO".to_string()).unwrap();
        state.dao_tree.witness().unwrap()
    };

    // It might just be easier to hash it ourselves from keypair and blind...
    let dao_bulla = {
        assert_eq!(tx.func_calls.len(), 1);
        let func_call = &tx.func_calls[0];
        let call_data = func_call.call_data.as_any();
        assert_eq!((&*call_data).type_id(), TypeId::of::<dao_contract::mint::validate::CallData>());
        let call_data = call_data.downcast_ref::<dao_contract::mint::validate::CallData>().unwrap();
        call_data.dao_bulla.clone()
    };

    ///////////////////////////////////////////////////
    //// Mint the initial supply of treasury token
    //// and send it all to the DAO directly
    ///////////////////////////////////////////////////
    debug!(target: "demo", "Stage 2. Minting treasury token");

    let state = states.lookup_mut::<money_contract::State>(&"Money".to_string()).unwrap();
    state.wallet_cache.track(dao_keypair.secret);

    //// Wallet

    // Address of deployed contract in our example is hook_dao_exec
    // This field is public, you can see it's being sent to a DAO
    // but nothing else is visible.
    //
    // In the python code we wrote:
    //
    //   spend_hook = b"0xdao_ruleset"
    //
    let hook_dao_exec = DrkSpendHook::random(&mut OsRng);
    let spend_hook = hook_dao_exec;
    // The user_data can be a simple hash of the items passed into the ZK proof
    // up to corresponding linked ZK proof to interpret however they need.
    // In out case, it's the bulla for the DAO
    let user_data = dao_bulla.0;

    let builder = money_contract::transfer::wallet::Builder {
        clear_inputs: vec![money_contract::transfer::wallet::BuilderClearInputInfo {
            value: xdrk_supply,
            token_id: xdrk_token_id,
            signature_secret: cashier_signature_secret,
        }],
        inputs: vec![],
        outputs: vec![money_contract::transfer::wallet::BuilderOutputInfo {
            value: xdrk_supply,
            token_id: xdrk_token_id,
            public: dao_keypair.public,
            spend_hook,
            user_data,
        }],
    };

    let func_call = builder.build(&zk_bins)?;

    let tx = Transaction { func_calls: vec![func_call] };

    //// Validator

    for (idx, func_call) in tx.func_calls.iter().enumerate() {
        // So then the verifier will lookup the corresponding state_transition and apply
        // functions based off the func_id
        if func_call.func_id == "Money::transfer()" {
            debug!("money_contract::transfer::state_transition()");

            let update = money_contract::transfer::validate::state_transition(&states, idx, &tx)
                .expect("money_contract::transfer::validate::state_transition() failed!");
            money_contract::transfer::validate::apply(&mut states, update);
        }
    }

    tx.zk_verify(&zk_bins);

    //// Wallet
    // DAO reads the money received from the encrypted note

    let dao_recv = {
        let state = states.lookup_mut::<money_contract::State>(&"Money".to_string()).unwrap();
        let mut recv_coins = state.wallet_cache.get_received(&dao_keypair.secret);
        assert_eq!(recv_coins.len(), 1);
        let recv_coin = recv_coins.pop().unwrap();
        let note = &recv_coin.note;

        // Check the actual coin received is valid before accepting it

        let coords = dao_keypair.public.0.to_affine().coordinates().unwrap();
        let coin = poseidon_hash::<8>([
            *coords.x(),
            *coords.y(),
            DrkValue::from(note.value),
            note.token_id,
            note.serial,
            note.spend_hook,
            note.user_data,
            note.coin_blind,
        ]);
        assert_eq!(coin, recv_coin.coin.0);

        assert_eq!(note.spend_hook, hook_dao_exec);
        assert_eq!(note.user_data, dao_bulla.0);

        debug!("DAO received a coin worth {} xDRK", note.value);

        recv_coin
    };

    ///////////////////////////////////////////////////
    //// Mint the governance token
    //// Send it to three hodlers
    ///////////////////////////////////////////////////
    debug!(target: "demo", "Stage 3. Minting governance token");

    //// Wallet

    // Hodler 1
    let gov_keypair_1 = Keypair::random(&mut OsRng);
    // Hodler 2
    let gov_keypair_2 = Keypair::random(&mut OsRng);
    // Hodler 3: the tiebreaker
    let gov_keypair_3 = Keypair::random(&mut OsRng);

    let state = states.lookup_mut::<money_contract::State>(&"Money".to_string()).unwrap();
    state.wallet_cache.track(gov_keypair_1.secret);
    state.wallet_cache.track(gov_keypair_2.secret);
    state.wallet_cache.track(gov_keypair_3.secret);

    let gov_keypairs = vec![gov_keypair_1, gov_keypair_2, gov_keypair_3];

    // We don't use this because money-transfer expects a cashier.
    // let signature_secret = SecretKey::random(&mut OsRng);

    // Spend hook and user data disabled
    let spend_hook = DrkSpendHook::from(0);
    let user_data = DrkUserData::from(0);

    let output1 = money_contract::transfer::wallet::BuilderOutputInfo {
        value: 400000,
        token_id: gdrk_token_id,
        public: gov_keypair_1.public,
        spend_hook,
        user_data,
    };

    let output2 = money_contract::transfer::wallet::BuilderOutputInfo {
        value: 400000,
        token_id: gdrk_token_id,
        public: gov_keypair_2.public,
        spend_hook,
        user_data,
    };

    let output3 = money_contract::transfer::wallet::BuilderOutputInfo {
        value: 200000,
        token_id: gdrk_token_id,
        public: gov_keypair_3.public,
        spend_hook,
        user_data,
    };

    assert!(2 * 400000 + 200000 == gdrk_supply);

    let builder = money_contract::transfer::wallet::Builder {
        clear_inputs: vec![money_contract::transfer::wallet::BuilderClearInputInfo {
            value: gdrk_supply,
            token_id: gdrk_token_id,
            signature_secret: cashier_signature_secret,
        }],
        inputs: vec![],
        outputs: vec![output1, output2, output3],
    };

    let func_call = builder.build(&zk_bins)?;

    let tx = Transaction { func_calls: vec![func_call] };

    //// Validator

    for (idx, func_call) in tx.func_calls.iter().enumerate() {
        // So then the verifier will lookup the corresponding state_transition and apply
        // functions based off the func_id
        if func_call.func_id == "Money::transfer()" {
            debug!("money_contract::transfer::state_transition()");

            let update = money_contract::transfer::validate::state_transition(&states, idx, &tx)
                .expect("money_contract::transfer::validate::state_transition() failed!");
            money_contract::transfer::validate::apply(&mut states, update);
        }
    }

    tx.zk_verify(&zk_bins);

    //// Wallet

    let mut gov_recv = vec![None, None, None];
    // Check that each person received one coin
    for (i, key) in gov_keypairs.iter().enumerate() {
        let gov_recv_coin = {
            let state = states.lookup_mut::<money_contract::State>(&"Money".to_string()).unwrap();
            let mut recv_coins = state.wallet_cache.get_received(&key.secret);
            assert_eq!(recv_coins.len(), 1);
            let recv_coin = recv_coins.pop().unwrap();
            let note = &recv_coin.note;

            assert_eq!(note.token_id, gdrk_token_id);
            // Normal payment
            assert_eq!(note.spend_hook, pallas::Base::from(0));
            assert_eq!(note.user_data, pallas::Base::from(0));

            let coords = key.public.0.to_affine().coordinates().unwrap();
            let coin = poseidon_hash::<8>([
                *coords.x(),
                *coords.y(),
                DrkValue::from(note.value),
                note.token_id,
                note.serial,
                note.spend_hook,
                note.user_data,
                note.coin_blind,
            ]);
            assert_eq!(coin, recv_coin.coin.0);

            debug!("Holder{} received a coin worth {} gDRK", i, note.value);

            recv_coin
        };
        gov_recv[i] = Some(gov_recv_coin);
    }
    // unwrap them for this demo
    let gov_recv: Vec<_> = gov_recv.into_iter().map(|r| r.unwrap()).collect();

    ///////////////////////////////////////////////////
    // DAO rules:
    // 1. gov token IDs must match on all inputs
    // 2. proposals must be submitted by minimum amount
    // 3. all votes >= quorum
    // 4. outcome > approval_ratio
    // 5. structure of outputs
    //   output 0: value and address
    //   output 1: change address
    ///////////////////////////////////////////////////

    ///////////////////////////////////////////////////
    // Propose the vote
    // In order to make a valid vote, first the proposer must
    // meet a criteria for a minimum number of gov tokens
    ///////////////////////////////////////////////////
    debug!(target: "demo", "Stage 4. Propose the vote");

    //// Wallet

    // TODO: look into proposal expiry once time for voting has finished

    let user_keypair = Keypair::random(&mut OsRng);

    let (money_leaf_position, money_merkle_path) = {
        let state = states.lookup::<money_contract::State>(&"Money".to_string()).unwrap();
        let tree = &state.tree;
        let leaf_position = gov_recv[0].leaf_position.clone();
        let root = tree.root(0).unwrap();
        let merkle_path = tree.authentication_path(leaf_position, &root).unwrap();
        (leaf_position, merkle_path)
    };

    // TODO: is it possible for an invalid transfer() to be constructed on exec()?
    //       need to look into this
    let input = dao_contract::propose::wallet::BuilderInput {
        secret: gov_keypair_1.secret,
        note: gov_recv[0].note.clone(),
        leaf_position: money_leaf_position,
        merkle_path: money_merkle_path,
    };

    let (dao_merkle_path, dao_merkle_root) = {
        let state = states.lookup::<dao_contract::State>(&"DAO".to_string()).unwrap();
        let tree = &state.dao_tree;
        let root = tree.root(0).unwrap();
        let merkle_path = tree.authentication_path(dao_leaf_position, &root).unwrap();
        (merkle_path, root)
    };

    let builder = dao_contract::propose::wallet::Builder {
        inputs: vec![input],
        proposal: dao_contract::propose::wallet::Proposal {
            dest: user_keypair.public,
            amount: 1000,
            serial: pallas::Base::random(&mut OsRng),
            token_id: xdrk_token_id,
            blind: pallas::Base::random(&mut OsRng),
        },
        dao: dao_contract::propose::wallet::DaoParams {
            proposer_limit: dao_proposer_limit,
            quorum: dao_quorum,
            approval_ratio: dao_approval_ratio,
            gov_token_id: gdrk_token_id,
            public_key: dao_keypair.public,
            bulla_blind: dao_bulla_blind,
        },
        dao_leaf_position,
        dao_merkle_path,
        dao_merkle_root,
    };

    let func_call = builder.build(&zk_bins);

    let tx = Transaction { func_calls: vec![func_call] };

    //// Validator

    for (idx, func_call) in tx.func_calls.iter().enumerate() {
        if func_call.func_id == "DAO::propose()" {
            debug!(target: "demo", "dao_contract::propose::state_transition()");

            let update = dao_contract::propose::validate::state_transition(&states, idx, &tx)
                .expect("dao_contract::propose::validate::state_transition() failed!");
            dao_contract::propose::validate::apply(&mut states, update);
        }
    }

    tx.zk_verify(&zk_bins);

    //// Wallet

    Ok(())
}
