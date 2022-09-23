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

use crate::{dao_contract, example_contract, money_contract};

// TODO: Anonymity leaks in this proof of concept:
//
// * Vote updates are linked to the proposal_bulla
// * Nullifier of vote will link vote with the coin when it's spent

// TODO: strategize and cleanup Result/Error usage
// TODO: fix up code doc

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

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

pub struct Transaction {
    pub func_calls: Vec<FuncCall>,
    pub signatures: Vec<Signature>,
}

impl Transaction {
    /// Verify ZK contracts for the entire tx
    /// In real code, we could parallelize this for loop
    /// TODO: fix use of unwrap with Result type stuff
    fn zk_verify(&self, zk_bins: &ZkContractTable) {
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

    fn verify_sigs(&self) {
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

fn sign(signature_secrets: Vec<SecretKey>, func_calls: &Vec<FuncCall>) -> Vec<Signature> {
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
    fn new() -> Self {
        Self { states: HashMap::new() }
    }

    fn register(&mut self, contract_id: ContractId, state: GenericContractState) {
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

///////////////////////////////////////////////////
///// Example contract
///////////////////////////////////////////////////
pub async fn example() -> Result<()> {
    debug!(target: "demo", "Stage 0. Example contract");
    // Lookup table for smart contract states
    let mut states = StateRegistry::new();

    // Initialize ZK binary table
    let mut zk_bins = ZkContractTable::new();

    let zk_example_foo_bincode = include_bytes!("../proof/foo.zk.bin");
    let zk_example_foo_bin = ZkBinary::decode(zk_example_foo_bincode)?;
    zk_bins.add_contract("example-foo".to_string(), zk_example_foo_bin, 13);

    let example_state = example_contract::state::State::new();
    states.register(*example_contract::CONTRACT_ID, example_state);

    //// Wallet

    let foo = example_contract::foo::wallet::Foo { a: 5, b: 10 };
    let signature_secret = SecretKey::random(&mut OsRng);

    let builder = example_contract::foo::wallet::Builder { foo, signature_secret };
    let func_call = builder.build(&zk_bins);
    let func_calls = vec![func_call];

    let signatures = sign(vec![signature_secret], &func_calls);
    let tx = Transaction { func_calls, signatures };

    //// Validator

    let mut updates = vec![];
    // Validate all function calls in the tx
    for (idx, func_call) in tx.func_calls.iter().enumerate() {
        if func_call.func_id == *example_contract::foo::FUNC_ID {
            debug!("example_contract::foo::state_transition()");

            let update = example_contract::foo::validate::state_transition(&states, idx, &tx)
                .expect("example_contract::foo::validate::state_transition() failed!");
            updates.push(update);
        }
    }

    // Atomically apply all changes
    for update in updates {
        update.apply(&mut states);
    }

    tx.zk_verify(&zk_bins);
    tx.verify_sigs();

    Ok(())
}

pub async fn demo() -> Result<()> {
    // Example smart contract
    //// TODO: this will be moved to a different file
    example().await?;

    // Money parameters
    let xdrk_supply = 1_000_000;
    let xdrk_token_id = pallas::Base::random(&mut OsRng);

    // Governance token parameters
    let gdrk_supply = 1_000_000;
    let gdrk_token_id = pallas::Base::random(&mut OsRng);

    // DAO parameters
    let dao_proposer_limit = 110;
    let dao_quorum = 110;
    let dao_approval_ratio_quot = 1;
    let dao_approval_ratio_base = 2;

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
    debug!(target: "demo", "Loading dao-vote-main.zk");
    let zk_dao_vote_main_bincode = include_bytes!("../proof/dao-vote-main.zk.bin");
    let zk_dao_vote_main_bin = ZkBinary::decode(zk_dao_vote_main_bincode)?;
    zk_bins.add_contract("dao-vote-main".to_string(), zk_dao_vote_main_bin, 13);
    debug!(target: "demo", "Loading dao-vote-burn.zk");
    let zk_dao_vote_burn_bincode = include_bytes!("../proof/dao-vote-burn.zk.bin");
    let zk_dao_vote_burn_bin = ZkBinary::decode(zk_dao_vote_burn_bincode)?;
    zk_bins.add_contract("dao-vote-burn".to_string(), zk_dao_vote_burn_bin, 13);
    let zk_dao_exec_bincode = include_bytes!("../proof/dao-exec.zk.bin");
    let zk_dao_exec_bin = ZkBinary::decode(zk_dao_exec_bincode)?;
    zk_bins.add_contract("dao-exec".to_string(), zk_dao_exec_bin, 13);

    // State for money contracts
    let cashier_signature_secret = SecretKey::random(&mut OsRng);
    let cashier_signature_public = PublicKey::from_secret(cashier_signature_secret);
    let faucet_signature_secret = SecretKey::random(&mut OsRng);
    let faucet_signature_public = PublicKey::from_secret(faucet_signature_secret);

    ///////////////////////////////////////////////////

    let money_state =
        money_contract::state::State::new(cashier_signature_public, faucet_signature_public);
    states.register(*money_contract::CONTRACT_ID, money_state);

    /////////////////////////////////////////////////////

    let dao_state = dao_contract::State::new();
    states.register(*dao_contract::CONTRACT_ID, dao_state);

    /////////////////////////////////////////////////////
    ////// Create the DAO bulla
    /////////////////////////////////////////////////////
    debug!(target: "demo", "Stage 1. Creating DAO bulla");

    //// Wallet

    //// Setup the DAO
    let dao_keypair = Keypair::random(&mut OsRng);
    let dao_bulla_blind = pallas::Base::random(&mut OsRng);

    let signature_secret = SecretKey::random(&mut OsRng);
    // Create DAO mint tx
    let builder = dao_contract::mint::wallet::Builder {
        dao_proposer_limit,
        dao_quorum,
        dao_approval_ratio_quot,
        dao_approval_ratio_base,
        gov_token_id: gdrk_token_id,
        dao_pubkey: dao_keypair.public,
        dao_bulla_blind,
        _signature_secret: signature_secret,
    };
    let func_call = builder.build(&zk_bins);
    let func_calls = vec![func_call];

    let signatures = sign(vec![signature_secret], &func_calls);
    let tx = Transaction { func_calls, signatures };

    //// Validator

    let mut updates = vec![];
    // Validate all function calls in the tx
    for (idx, func_call) in tx.func_calls.iter().enumerate() {
        // So then the verifier will lookup the corresponding state_transition and apply
        // functions based off the func_id
        if func_call.func_id == *dao_contract::mint::FUNC_ID {
            debug!("dao_contract::mint::state_transition()");

            let update = dao_contract::mint::validate::state_transition(&states, idx, &tx)
                .expect("dao_contract::mint::validate::state_transition() failed!");
            updates.push(update);
        }
    }

    // Atomically apply all changes
    for update in updates {
        update.apply(&mut states);
    }

    tx.zk_verify(&zk_bins);
    tx.verify_sigs();

    // Wallet stuff

    // In your wallet, wait until you see the tx confirmed before doing anything below
    // So for example keep track of tx hash
    //assert_eq!(tx.hash(), tx_hash);

    // We need to witness() the value in our local merkle tree
    // Must be called as soon as this DAO bulla is added to the state
    let dao_leaf_position = {
        let state = states.lookup_mut::<dao_contract::State>(*dao_contract::CONTRACT_ID).unwrap();
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
    debug!(target: "demo", "Create DAO bulla: {:?}", dao_bulla.0);

    ///////////////////////////////////////////////////
    //// Mint the initial supply of treasury token
    //// and send it all to the DAO directly
    ///////////////////////////////////////////////////
    debug!(target: "demo", "Stage 2. Minting treasury token");

    let state = states.lookup_mut::<money_contract::State>(*money_contract::CONTRACT_ID).unwrap();
    state.wallet_cache.track(dao_keypair.secret);

    //// Wallet

    // Address of deployed contract in our example is dao_contract::exec::FUNC_ID
    // This field is public, you can see it's being sent to a DAO
    // but nothing else is visible.
    //
    // In the python code we wrote:
    //
    //   spend_hook = b"0xdao_ruleset"
    //
    let spend_hook = *dao_contract::exec::FUNC_ID;
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
            serial: pallas::Base::random(&mut OsRng),
            coin_blind: pallas::Base::random(&mut OsRng),
            spend_hook,
            user_data,
        }],
    };

    let func_call = builder.build(&zk_bins)?;
    let func_calls = vec![func_call];

    let signatures = sign(vec![cashier_signature_secret], &func_calls);
    let tx = Transaction { func_calls, signatures };

    //// Validator

    let mut updates = vec![];
    // Validate all function calls in the tx
    for (idx, func_call) in tx.func_calls.iter().enumerate() {
        // So then the verifier will lookup the corresponding state_transition and apply
        // functions based off the func_id
        if func_call.func_id == *money_contract::transfer::FUNC_ID {
            debug!("money_contract::transfer::state_transition()");

            let update = money_contract::transfer::validate::state_transition(&states, idx, &tx)
                .expect("money_contract::transfer::validate::state_transition() failed!");
            updates.push(update);
        }
    }

    // Atomically apply all changes
    for update in updates {
        update.apply(&mut states);
    }

    tx.zk_verify(&zk_bins);
    tx.verify_sigs();

    //// Wallet
    // DAO reads the money received from the encrypted note

    let state = states.lookup_mut::<money_contract::State>(*money_contract::CONTRACT_ID).unwrap();
    let mut recv_coins = state.wallet_cache.get_received(&dao_keypair.secret);
    assert_eq!(recv_coins.len(), 1);
    let dao_recv_coin = recv_coins.pop().unwrap();
    let treasury_note = dao_recv_coin.note;

    // Check the actual coin received is valid before accepting it

    let coords = dao_keypair.public.0.to_affine().coordinates().unwrap();
    let coin = poseidon_hash::<8>([
        *coords.x(),
        *coords.y(),
        DrkValue::from(treasury_note.value),
        treasury_note.token_id,
        treasury_note.serial,
        treasury_note.spend_hook,
        treasury_note.user_data,
        treasury_note.coin_blind,
    ]);
    assert_eq!(coin, dao_recv_coin.coin.0);

    assert_eq!(treasury_note.spend_hook, *dao_contract::exec::FUNC_ID);
    assert_eq!(treasury_note.user_data, dao_bulla.0);

    debug!("DAO received a coin worth {} xDRK", treasury_note.value);

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

    let state = states.lookup_mut::<money_contract::State>(*money_contract::CONTRACT_ID).unwrap();
    state.wallet_cache.track(gov_keypair_1.secret);
    state.wallet_cache.track(gov_keypair_2.secret);
    state.wallet_cache.track(gov_keypair_3.secret);

    let gov_keypairs = vec![gov_keypair_1, gov_keypair_2, gov_keypair_3];

    // Spend hook and user data disabled
    let spend_hook = DrkSpendHook::from(0);
    let user_data = DrkUserData::from(0);

    let output1 = money_contract::transfer::wallet::BuilderOutputInfo {
        value: 400000,
        token_id: gdrk_token_id,
        public: gov_keypair_1.public,
        serial: pallas::Base::random(&mut OsRng),
        coin_blind: pallas::Base::random(&mut OsRng),
        spend_hook,
        user_data,
    };

    let output2 = money_contract::transfer::wallet::BuilderOutputInfo {
        value: 400000,
        token_id: gdrk_token_id,
        public: gov_keypair_2.public,
        serial: pallas::Base::random(&mut OsRng),
        coin_blind: pallas::Base::random(&mut OsRng),
        spend_hook,
        user_data,
    };

    let output3 = money_contract::transfer::wallet::BuilderOutputInfo {
        value: 200000,
        token_id: gdrk_token_id,
        public: gov_keypair_3.public,
        serial: pallas::Base::random(&mut OsRng),
        coin_blind: pallas::Base::random(&mut OsRng),
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
    let func_calls = vec![func_call];

    let signatures = sign(vec![cashier_signature_secret], &func_calls);
    let tx = Transaction { func_calls, signatures };

    //// Validator

    let mut updates = vec![];
    // Validate all function calls in the tx
    for (idx, func_call) in tx.func_calls.iter().enumerate() {
        // So then the verifier will lookup the corresponding state_transition and apply
        // functions based off the func_id
        if func_call.func_id == *money_contract::transfer::FUNC_ID {
            debug!("money_contract::transfer::state_transition()");

            let update = money_contract::transfer::validate::state_transition(&states, idx, &tx)
                .expect("money_contract::transfer::validate::state_transition() failed!");
            updates.push(update);
        }
    }

    // Atomically apply all changes
    for update in updates {
        update.apply(&mut states);
    }

    tx.zk_verify(&zk_bins);
    tx.verify_sigs();

    //// Wallet

    let mut gov_recv = vec![None, None, None];
    // Check that each person received one coin
    for (i, key) in gov_keypairs.iter().enumerate() {
        let gov_recv_coin = {
            let state =
                states.lookup_mut::<money_contract::State>(*money_contract::CONTRACT_ID).unwrap();
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
        let state = states.lookup::<money_contract::State>(*money_contract::CONTRACT_ID).unwrap();
        let tree = &state.tree;
        let leaf_position = gov_recv[0].leaf_position.clone();
        let root = tree.root(0).unwrap();
        let merkle_path = tree.authentication_path(leaf_position, &root).unwrap();
        (leaf_position, merkle_path)
    };

    // TODO: is it possible for an invalid transfer() to be constructed on exec()?
    //       need to look into this
    let signature_secret = SecretKey::random(&mut OsRng);
    let input = dao_contract::propose::wallet::BuilderInput {
        secret: gov_keypair_1.secret,
        note: gov_recv[0].note.clone(),
        leaf_position: money_leaf_position,
        merkle_path: money_merkle_path,
        signature_secret,
    };

    let (dao_merkle_path, dao_merkle_root) = {
        let state = states.lookup::<dao_contract::State>(*dao_contract::CONTRACT_ID).unwrap();
        let tree = &state.dao_tree;
        let root = tree.root(0).unwrap();
        let merkle_path = tree.authentication_path(dao_leaf_position, &root).unwrap();
        (merkle_path, root)
    };

    let dao_params = dao_contract::mint::wallet::DaoParams {
        proposer_limit: dao_proposer_limit,
        quorum: dao_quorum,
        approval_ratio_base: dao_approval_ratio_base,
        approval_ratio_quot: dao_approval_ratio_quot,
        gov_token_id: gdrk_token_id,
        public_key: dao_keypair.public,
        bulla_blind: dao_bulla_blind,
    };

    let proposal = dao_contract::propose::wallet::Proposal {
        dest: user_keypair.public,
        amount: 1000,
        serial: pallas::Base::random(&mut OsRng),
        token_id: xdrk_token_id,
        blind: pallas::Base::random(&mut OsRng),
    };

    let builder = dao_contract::propose::wallet::Builder {
        inputs: vec![input],
        proposal: proposal.clone(),
        dao: dao_params.clone(),
        dao_leaf_position,
        dao_merkle_path,
        dao_merkle_root,
    };

    let func_call = builder.build(&zk_bins);
    let func_calls = vec![func_call];

    let signatures = sign(vec![signature_secret], &func_calls);
    let tx = Transaction { func_calls, signatures };

    //// Validator

    let mut updates = vec![];
    // Validate all function calls in the tx
    for (idx, func_call) in tx.func_calls.iter().enumerate() {
        if func_call.func_id == *dao_contract::propose::FUNC_ID {
            debug!(target: "demo", "dao_contract::propose::state_transition()");

            let update = dao_contract::propose::validate::state_transition(&states, idx, &tx)
                .expect("dao_contract::propose::validate::state_transition() failed!");
            updates.push(update);
        }
    }

    // Atomically apply all changes
    for update in updates {
        update.apply(&mut states);
    }

    tx.zk_verify(&zk_bins);
    tx.verify_sigs();

    //// Wallet

    // Read received proposal
    let (proposal, proposal_bulla) = {
        assert_eq!(tx.func_calls.len(), 1);
        let func_call = &tx.func_calls[0];
        let call_data = func_call.call_data.as_any();
        assert_eq!(
            (&*call_data).type_id(),
            TypeId::of::<dao_contract::propose::validate::CallData>()
        );
        let call_data =
            call_data.downcast_ref::<dao_contract::propose::validate::CallData>().unwrap();

        let header = &call_data.header;
        let note: dao_contract::propose::wallet::Note =
            header.enc_note.decrypt(&dao_keypair.secret).unwrap();

        // TODO: check it belongs to DAO bulla

        // Return the proposal info
        (note.proposal, call_data.header.proposal_bulla)
    };
    debug!(target: "demo", "Proposal now active!");
    debug!(target: "demo", "  destination: {:?}", proposal.dest);
    debug!(target: "demo", "  amount: {}", proposal.amount);
    debug!(target: "demo", "  token_id: {:?}", proposal.token_id);
    debug!(target: "demo", "  dao_bulla: {:?}", dao_bulla.0);
    debug!(target: "demo", "Proposal bulla: {:?}", proposal_bulla);

    ///////////////////////////////////////////////////
    // Proposal is accepted!
    // Start the voting
    ///////////////////////////////////////////////////

    // Copying these schizo comments from python code:
    // Lets the voting begin
    // Voters have access to the proposal and dao data
    //   vote_state = VoteState()
    // We don't need to copy nullifier set because it is checked from gov_state
    // in vote_state_transition() anyway
    //
    // TODO: what happens if voters don't unblind their vote
    // Answer:
    //   1. there is a time limit
    //   2. both the MPC or users can unblind
    //
    // TODO: bug if I vote then send money, then we can double vote
    // TODO: all timestamps missing
    //       - timelock (future voting starts in 2 days)
    // Fix: use nullifiers from money gov state only from
    // beginning of gov period
    // Cannot use nullifiers from before voting period

    debug!(target: "demo", "Stage 5. Start voting");

    // We were previously saving updates here for testing
    // let mut updates = vec![];

    // User 1: YES

    let (money_leaf_position, money_merkle_path) = {
        let state = states.lookup::<money_contract::State>(*money_contract::CONTRACT_ID).unwrap();
        let tree = &state.tree;
        let leaf_position = gov_recv[0].leaf_position.clone();
        let root = tree.root(0).unwrap();
        let merkle_path = tree.authentication_path(leaf_position, &root).unwrap();
        (leaf_position, merkle_path)
    };

    let signature_secret = SecretKey::random(&mut OsRng);
    let input = dao_contract::vote::wallet::BuilderInput {
        secret: gov_keypair_1.secret,
        note: gov_recv[0].note.clone(),
        leaf_position: money_leaf_position,
        merkle_path: money_merkle_path,
        signature_secret,
    };

    let vote_option: bool = true;

    assert!(vote_option == true || vote_option == false);

    // We create a new keypair to encrypt the vote.
    // For the demo MVP, you can just use the dao_keypair secret
    let vote_keypair_1 = Keypair::random(&mut OsRng);

    let builder = dao_contract::vote::wallet::Builder {
        inputs: vec![input],
        vote: dao_contract::vote::wallet::Vote {
            vote_option,
            vote_option_blind: pallas::Scalar::random(&mut OsRng),
        },
        vote_keypair: vote_keypair_1,
        proposal: proposal.clone(),
        dao: dao_params.clone(),
    };
    debug!(target: "demo", "build()...");
    let func_call = builder.build(&zk_bins);
    let func_calls = vec![func_call];

    let signatures = sign(vec![signature_secret], &func_calls);
    let tx = Transaction { func_calls, signatures };

    //// Validator

    let mut updates = vec![];
    // Validate all function calls in the tx
    for (idx, func_call) in tx.func_calls.iter().enumerate() {
        if func_call.func_id == *dao_contract::vote::FUNC_ID {
            debug!(target: "demo", "dao_contract::vote::state_transition()");

            let update = dao_contract::vote::validate::state_transition(&states, idx, &tx)
                .expect("dao_contract::vote::validate::state_transition() failed!");
            updates.push(update);
        }
    }

    // Atomically apply all changes
    for update in updates {
        update.apply(&mut states);
    }

    tx.zk_verify(&zk_bins);
    tx.verify_sigs();

    //// Wallet

    // Secret vote info. Needs to be revealed at some point.
    // TODO: look into verifiable encryption for notes
    // TODO: look into timelock puzzle as a possibility
    let vote_note_1 = {
        assert_eq!(tx.func_calls.len(), 1);
        let func_call = &tx.func_calls[0];
        let call_data = func_call.call_data.as_any();
        assert_eq!((&*call_data).type_id(), TypeId::of::<dao_contract::vote::validate::CallData>());
        let call_data = call_data.downcast_ref::<dao_contract::vote::validate::CallData>().unwrap();

        let header = &call_data.header;
        let note: dao_contract::vote::wallet::Note =
            header.enc_note.decrypt(&vote_keypair_1.secret).unwrap();
        note
    };
    debug!(target: "demo", "User 1 voted!");
    debug!(target: "demo", "  vote_option: {}", vote_note_1.vote.vote_option);
    debug!(target: "demo", "  value: {}", vote_note_1.vote_value);

    // User 2: NO

    let (money_leaf_position, money_merkle_path) = {
        let state = states.lookup::<money_contract::State>(*money_contract::CONTRACT_ID).unwrap();
        let tree = &state.tree;
        let leaf_position = gov_recv[1].leaf_position.clone();
        let root = tree.root(0).unwrap();
        let merkle_path = tree.authentication_path(leaf_position, &root).unwrap();
        (leaf_position, merkle_path)
    };

    let signature_secret = SecretKey::random(&mut OsRng);
    let input = dao_contract::vote::wallet::BuilderInput {
        secret: gov_keypair_2.secret,
        note: gov_recv[1].note.clone(),
        leaf_position: money_leaf_position,
        merkle_path: money_merkle_path,
        signature_secret,
    };

    let vote_option: bool = false;

    assert!(vote_option == true || vote_option == false);

    // We create a new keypair to encrypt the vote.
    let vote_keypair_2 = Keypair::random(&mut OsRng);

    let builder = dao_contract::vote::wallet::Builder {
        inputs: vec![input],
        vote: dao_contract::vote::wallet::Vote {
            vote_option,
            vote_option_blind: pallas::Scalar::random(&mut OsRng),
        },
        vote_keypair: vote_keypair_2,
        proposal: proposal.clone(),
        dao: dao_params.clone(),
    };
    debug!(target: "demo", "build()...");
    let func_call = builder.build(&zk_bins);
    let func_calls = vec![func_call];

    let signatures = sign(vec![signature_secret], &func_calls);
    let tx = Transaction { func_calls, signatures };

    //// Validator

    let mut updates = vec![];
    // Validate all function calls in the tx
    for (idx, func_call) in tx.func_calls.iter().enumerate() {
        if func_call.func_id == *dao_contract::vote::FUNC_ID {
            debug!(target: "demo", "dao_contract::vote::state_transition()");

            let update = dao_contract::vote::validate::state_transition(&states, idx, &tx)
                .expect("dao_contract::vote::validate::state_transition() failed!");
            updates.push(update);
        }
    }

    // Atomically apply all changes
    for update in updates {
        update.apply(&mut states);
    }

    tx.zk_verify(&zk_bins);
    tx.verify_sigs();

    //// Wallet

    // Secret vote info. Needs to be revealed at some point.
    // TODO: look into verifiable encryption for notes
    // TODO: look into timelock puzzle as a possibility
    let vote_note_2 = {
        assert_eq!(tx.func_calls.len(), 1);
        let func_call = &tx.func_calls[0];
        let call_data = func_call.call_data.as_any();
        assert_eq!((&*call_data).type_id(), TypeId::of::<dao_contract::vote::validate::CallData>());
        let call_data = call_data.downcast_ref::<dao_contract::vote::validate::CallData>().unwrap();

        let header = &call_data.header;
        let note: dao_contract::vote::wallet::Note =
            header.enc_note.decrypt(&vote_keypair_2.secret).unwrap();
        note
    };
    debug!(target: "demo", "User 2 voted!");
    debug!(target: "demo", "  vote_option: {}", vote_note_2.vote.vote_option);
    debug!(target: "demo", "  value: {}", vote_note_2.vote_value);

    // User 3: YES

    let (money_leaf_position, money_merkle_path) = {
        let state = states.lookup::<money_contract::State>(*money_contract::CONTRACT_ID).unwrap();
        let tree = &state.tree;
        let leaf_position = gov_recv[2].leaf_position.clone();
        let root = tree.root(0).unwrap();
        let merkle_path = tree.authentication_path(leaf_position, &root).unwrap();
        (leaf_position, merkle_path)
    };

    let signature_secret = SecretKey::random(&mut OsRng);
    let input = dao_contract::vote::wallet::BuilderInput {
        secret: gov_keypair_3.secret,
        note: gov_recv[2].note.clone(),
        leaf_position: money_leaf_position,
        merkle_path: money_merkle_path,
        signature_secret,
    };

    let vote_option: bool = true;

    assert!(vote_option == true || vote_option == false);

    // We create a new keypair to encrypt the vote.
    let vote_keypair_3 = Keypair::random(&mut OsRng);

    let builder = dao_contract::vote::wallet::Builder {
        inputs: vec![input],
        vote: dao_contract::vote::wallet::Vote {
            vote_option,
            vote_option_blind: pallas::Scalar::random(&mut OsRng),
        },
        vote_keypair: vote_keypair_3,
        proposal: proposal.clone(),
        dao: dao_params.clone(),
    };
    debug!(target: "demo", "build()...");
    let func_call = builder.build(&zk_bins);
    let func_calls = vec![func_call];

    let signatures = sign(vec![signature_secret], &func_calls);
    let tx = Transaction { func_calls, signatures };

    //// Validator

    let mut updates = vec![];
    // Validate all function calls in the tx
    for (idx, func_call) in tx.func_calls.iter().enumerate() {
        if func_call.func_id == *dao_contract::vote::FUNC_ID {
            debug!(target: "demo", "dao_contract::vote::state_transition()");

            let update = dao_contract::vote::validate::state_transition(&states, idx, &tx)
                .expect("dao_contract::vote::validate::state_transition() failed!");
            updates.push(update);
        }
    }

    // Atomically apply all changes
    for update in updates {
        update.apply(&mut states);
    }

    tx.zk_verify(&zk_bins);
    tx.verify_sigs();

    //// Wallet

    // Secret vote info. Needs to be revealed at some point.
    // TODO: look into verifiable encryption for notes
    // TODO: look into timelock puzzle as a possibility
    let vote_note_3 = {
        assert_eq!(tx.func_calls.len(), 1);
        let func_call = &tx.func_calls[0];
        let call_data = func_call.call_data.as_any();
        assert_eq!((&*call_data).type_id(), TypeId::of::<dao_contract::vote::validate::CallData>());
        let call_data = call_data.downcast_ref::<dao_contract::vote::validate::CallData>().unwrap();

        let header = &call_data.header;
        let note: dao_contract::vote::wallet::Note =
            header.enc_note.decrypt(&vote_keypair_3.secret).unwrap();
        note
    };
    debug!(target: "demo", "User 3 voted!");
    debug!(target: "demo", "  vote_option: {}", vote_note_3.vote.vote_option);
    debug!(target: "demo", "  value: {}", vote_note_3.vote_value);

    // Every votes produces a semi-homomorphic encryption of their vote.
    // Which is either yes or no
    // We copy the state tree for the governance token so coins can be used
    // to vote on other proposals at the same time.
    // With their vote, they produce a ZK proof + nullifier
    // The votes are unblinded by MPC to a selected party at the end of the
    // voting period.
    // (that's if we want votes to be hidden during voting)

    let mut yes_votes_value = 0;
    let mut yes_votes_blind = pallas::Scalar::from(0);
    let mut yes_votes_commit = pallas::Point::identity();

    let mut all_votes_value = 0;
    let mut all_votes_blind = pallas::Scalar::from(0);
    let mut all_votes_commit = pallas::Point::identity();

    // We were previously saving votes to a Vec<Update> for testing.
    // However since Update is now UpdateBase it gets moved into update.apply().
    // So we need to think of another way to run these tests.
    //assert!(updates.len() == 3);

    for (i, note /* update*/) in [vote_note_1, vote_note_2, vote_note_3]
        .iter() /*.zip(updates)*/
        .enumerate()
    {
        let vote_commit = pedersen_commitment_u64(note.vote_value, note.vote_value_blind);
        //assert!(update.value_commit == all_vote_value_commit);
        all_votes_commit += vote_commit;
        all_votes_blind += note.vote_value_blind;

        let yes_vote_commit = pedersen_commitment_u64(
            note.vote.vote_option as u64 * note.vote_value,
            note.vote.vote_option_blind,
        );
        //assert!(update.yes_vote_commit == yes_vote_commit);

        yes_votes_commit += yes_vote_commit;
        yes_votes_blind += note.vote.vote_option_blind;

        let vote_option = note.vote.vote_option;

        if vote_option {
            yes_votes_value += note.vote_value;
        }
        all_votes_value += note.vote_value;
        let vote_result: String = if vote_option { "yes".to_string() } else { "no".to_string() };

        debug!("Voter {} voted {}", i, vote_result);
    }

    debug!("Outcome = {} / {}", yes_votes_value, all_votes_value);

    assert!(all_votes_commit == pedersen_commitment_u64(all_votes_value, all_votes_blind));
    assert!(yes_votes_commit == pedersen_commitment_u64(yes_votes_value, yes_votes_blind));

    ///////////////////////////////////////////////////
    // Execute the vote
    ///////////////////////////////////////////////////

    //// Wallet

    // Used to export user_data from this coin so it can be accessed by DAO::exec()
    let user_data_blind = pallas::Base::random(&mut OsRng);

    let user_serial = pallas::Base::random(&mut OsRng);
    let user_coin_blind = pallas::Base::random(&mut OsRng);
    let dao_serial = pallas::Base::random(&mut OsRng);
    let dao_coin_blind = pallas::Base::random(&mut OsRng);
    let input_value = treasury_note.value;
    let input_value_blind = pallas::Scalar::random(&mut OsRng);
    let tx_signature_secret = SecretKey::random(&mut OsRng);
    let exec_signature_secret = SecretKey::random(&mut OsRng);

    let (treasury_leaf_position, treasury_merkle_path) = {
        let state = states.lookup::<money_contract::State>(*money_contract::CONTRACT_ID).unwrap();
        let tree = &state.tree;
        let leaf_position = dao_recv_coin.leaf_position.clone();
        let root = tree.root(0).unwrap();
        let merkle_path = tree.authentication_path(leaf_position, &root).unwrap();
        (leaf_position, merkle_path)
    };

    let input = money_contract::transfer::wallet::BuilderInputInfo {
        leaf_position: treasury_leaf_position,
        merkle_path: treasury_merkle_path,
        secret: dao_keypair.secret,
        note: treasury_note,
        user_data_blind,
        value_blind: input_value_blind,
        signature_secret: tx_signature_secret,
    };

    let builder = money_contract::transfer::wallet::Builder {
        clear_inputs: vec![],
        inputs: vec![input],
        outputs: vec![
            // Sending money
            money_contract::transfer::wallet::BuilderOutputInfo {
                value: 1000,
                token_id: xdrk_token_id,
                public: user_keypair.public,
                serial: proposal.serial,
                coin_blind: proposal.blind,
                spend_hook: pallas::Base::from(0),
                user_data: pallas::Base::from(0),
            },
            // Change back to DAO
            money_contract::transfer::wallet::BuilderOutputInfo {
                value: xdrk_supply - 1000,
                token_id: xdrk_token_id,
                public: dao_keypair.public,
                serial: dao_serial,
                coin_blind: dao_coin_blind,
                spend_hook: *dao_contract::exec::FUNC_ID,
                user_data: proposal_bulla,
            },
        ],
    };

    let transfer_func_call = builder.build(&zk_bins)?;

    let builder = dao_contract::exec::wallet::Builder {
        proposal,
        dao: dao_params,
        yes_votes_value,
        all_votes_value,
        yes_votes_blind,
        all_votes_blind,
        user_serial,
        user_coin_blind,
        dao_serial,
        dao_coin_blind,
        input_value,
        input_value_blind,
        hook_dao_exec: *dao_contract::exec::FUNC_ID,
        signature_secret: exec_signature_secret,
    };
    let exec_func_call = builder.build(&zk_bins);
    let func_calls = vec![transfer_func_call, exec_func_call];

    let signatures = sign(vec![tx_signature_secret, exec_signature_secret], &func_calls);
    let tx = Transaction { func_calls, signatures };

    {
        // Now the spend_hook field specifies the function DAO::exec()
        // so Money::transfer() must also be combined with DAO::exec()

        assert_eq!(tx.func_calls.len(), 2);
        let transfer_func_call = &tx.func_calls[0];
        let transfer_call_data = transfer_func_call.call_data.as_any();

        assert_eq!(
            (&*transfer_call_data).type_id(),
            TypeId::of::<money_contract::transfer::validate::CallData>()
        );
        let transfer_call_data =
            transfer_call_data.downcast_ref::<money_contract::transfer::validate::CallData>();
        let transfer_call_data = transfer_call_data.unwrap();
        // At least one input has this field value which means DAO::exec() is invoked.
        assert_eq!(transfer_call_data.inputs.len(), 1);
        let input = &transfer_call_data.inputs[0];
        assert_eq!(input.revealed.spend_hook, *dao_contract::exec::FUNC_ID);
        let user_data_enc = poseidon_hash::<2>([dao_bulla.0, user_data_blind]);
        assert_eq!(input.revealed.user_data_enc, user_data_enc);
    }

    //// Validator

    let mut updates = vec![];
    // Validate all function calls in the tx
    for (idx, func_call) in tx.func_calls.iter().enumerate() {
        if func_call.func_id == *dao_contract::exec::FUNC_ID {
            debug!("dao_contract::exec::state_transition()");

            let update = dao_contract::exec::validate::state_transition(&states, idx, &tx)
                .expect("dao_contract::exec::validate::state_transition() failed!");
            updates.push(update);
        } else if func_call.func_id == *money_contract::transfer::FUNC_ID {
            debug!("money_contract::transfer::state_transition()");

            let update = money_contract::transfer::validate::state_transition(&states, idx, &tx)
                .expect("money_contract::transfer::validate::state_transition() failed!");
            updates.push(update);
        }
    }

    // Atomically apply all changes
    for update in updates {
        update.apply(&mut states);
    }

    // Other stuff
    tx.zk_verify(&zk_bins);
    tx.verify_sigs();

    //// Wallet

    Ok(())
}
