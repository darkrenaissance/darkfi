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

use std::{
    any::{Any, TypeId},
    time::Instant,
};

use darkfi_sdk::crypto::{
    constants::MERKLE_DEPTH, pedersen::pedersen_commitment_u64, poseidon_hash, Keypair, MerkleNode,
    PublicKey, SecretKey, TokenId,
};
use incrementalmerkletree::{bridgetree::BridgeTree, Tree};
use log::debug;
use pasta_curves::{
    group::{ff::Field, Group},
    pallas,
};
use rand::rngs::OsRng;

use darkfi::{
    crypto::{
        coin::Coin,
        proof::{ProvingKey, VerifyingKey},
        types::{DrkSpendHook, DrkUserData, DrkValue},
    },
    zk::circuit::{BurnContract, MintContract},
    zkas::decoder::ZkBinary,
};

mod contract;
mod error;
mod note;
mod util;

use crate::{
    contract::{dao, example, money},
    note::EncryptedNote2,
    util::{sign, StateRegistry, Transaction, ZkContractTable},
};

type MerkleTree = BridgeTree<MerkleNode, { MERKLE_DEPTH }>;

pub struct OwnCoin {
    pub coin: Coin,
    pub note: money::transfer::wallet::Note,
    pub leaf_position: incrementalmerkletree::Position,
}

pub struct WalletCache {
    // Normally this would be a HashMap, but SecretKey is not Hash-able
    // TODO: This can be HashableBase
    cache: Vec<(SecretKey, Vec<OwnCoin>)>,
    /// The entire Merkle tree state
    tree: MerkleTree,
}

impl Default for WalletCache {
    fn default() -> Self {
        Self { cache: Vec::new(), tree: MerkleTree::new(100) }
    }
}

impl WalletCache {
    pub fn new() -> Self {
        Self { cache: Vec::new(), tree: MerkleTree::new(100) }
    }

    /// Must be called at the start to begin tracking received coins for this secret.
    pub fn track(&mut self, secret: SecretKey) {
        self.cache.push((secret, Vec::new()));
    }

    /// Get all coins received by this secret key
    /// track() must be called on this secret before calling this or the function will panic.
    pub fn get_received(&mut self, secret: &SecretKey) -> Vec<OwnCoin> {
        for (other_secret, own_coins) in self.cache.iter_mut() {
            if *secret == *other_secret {
                // clear own_coins vec, and return current contents
                return std::mem::take(own_coins)
            }
        }
        panic!("you forget to track() this secret!");
    }

    pub fn try_decrypt_note(&mut self, coin: Coin, ciphertext: &EncryptedNote2) {
        // Add the new coins to the Merkle tree
        let node = MerkleNode::from(coin.0);
        self.tree.append(&node);

        // Loop through all our secret keys...
        for (secret, own_coins) in self.cache.iter_mut() {
            // .. attempt to decrypt the note ...
            if let Ok(note) = ciphertext.decrypt(secret) {
                let leaf_position = self.tree.witness().expect("coin should be in tree");
                own_coins.push(OwnCoin { coin, note, leaf_position });
            }
        }
    }
}

// TODO: Anonymity leaks in this proof of concept:
//
// * Vote updates are linked to the proposal_bulla
// * Nullifier of vote will link vote with the coin when it's spent

// TODO: strategize and cleanup Result/Error usage
// TODO: fix up code doc

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

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

    let example_state = example::state::State::new();
    states.register(*example::CONTRACT_ID, example_state);

    //// Wallet

    let foo_w = example::foo::wallet::Foo { a: 5, b: 10 };
    let signature_secret = SecretKey::random(&mut OsRng);

    let builder = example::foo::wallet::Builder { foo: foo_w, signature_secret };
    let func_call = builder.build(&zk_bins);
    let func_calls = vec![func_call];

    let mut signatures = vec![];
    for func_call in &func_calls {
        let sign = sign([signature_secret].to_vec(), func_call);
        signatures.push(sign);
    }

    let tx = Transaction { func_calls, signatures };

    //// Validator

    let mut updates = vec![];
    // Validate all function calls in the tx
    for (idx, func_call) in tx.func_calls.iter().enumerate() {
        if func_call.func_id == *example::foo::FUNC_ID {
            debug!("example::foo::state_transition()");

            let update = example::foo::validate::state_transition(&states, idx, &tx)
                .expect("example::foo::validate::state_transition() failed!");
            updates.push(update);
        }
    }

    // Atomically apply all changes
    for update in updates {
        update.apply(&mut states);
    }

    tx.zk_verify(&zk_bins).unwrap();
    tx.verify_sigs();

    Ok(())
}

#[async_std::main]
async fn main() -> Result<()> {
    env_logger::init();

    // Example smart contract
    //// TODO: this will be moved to a different file
    example().await?;

    // Money parameters
    let xdrk_supply = 1_000_000;
    let xdrk_token_id = TokenId::from(pallas::Base::random(&mut OsRng));

    // Governance token parameters
    let gdrk_supply = 1_000_000;
    let gdrk_token_id = TokenId::from(pallas::Base::random(&mut OsRng));

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

    // We use this to receive coins
    let mut cache = WalletCache::new();

    ///////////////////////////////////////////////////

    let money_state = money::state::State::new(cashier_signature_public, faucet_signature_public);
    states.register(*money::CONTRACT_ID, money_state);

    /////////////////////////////////////////////////////

    let dao_state = dao::State::new();
    states.register(*dao::CONTRACT_ID, dao_state);

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
    let builder = dao::mint::wallet::Builder {
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

    let mut signatures = vec![];
    for func_call in &func_calls {
        let sign = sign([signature_secret].to_vec(), func_call);
        signatures.push(sign);
    }

    let tx = Transaction { func_calls, signatures };

    //// Validator

    let mut updates = vec![];
    // Validate all function calls in the tx
    for (idx, func_call) in tx.func_calls.iter().enumerate() {
        // So then the verifier will lookup the corresponding state_transition and apply
        // functions based off the func_id
        if func_call.func_id == *dao::mint::FUNC_ID {
            debug!("dao::mint::state_transition()");

            let update = dao::mint::validate::state_transition(&states, idx, &tx)
                .expect("dao::mint::validate::state_transition() failed!");
            updates.push(update);
        }
    }

    // Atomically apply all changes
    for update in updates {
        update.apply(&mut states);
    }

    tx.zk_verify(&zk_bins).unwrap();
    tx.verify_sigs();

    // Wallet stuff

    // In your wallet, wait until you see the tx confirmed before doing anything below
    // So for example keep track of tx hash
    //assert_eq!(tx.hash(), tx_hash);

    // We need to witness() the value in our local merkle tree
    // Must be called as soon as this DAO bulla is added to the state
    let dao_leaf_position = {
        let state = states.lookup_mut::<dao::State>(*dao::CONTRACT_ID).unwrap();
        state.dao_tree.witness().unwrap()
    };

    // It might just be easier to hash it ourselves from keypair and blind...
    let dao_bulla = {
        assert_eq!(tx.func_calls.len(), 1);
        let func_call = &tx.func_calls[0];
        let call_data = func_call.call_data.as_any();
        assert_eq!((*call_data).type_id(), TypeId::of::<dao::mint::validate::CallData>());
        let call_data = call_data.downcast_ref::<dao::mint::validate::CallData>().unwrap();
        call_data.dao_bulla.clone()
    };
    debug!(target: "demo", "Create DAO bulla: {:?}", dao_bulla.0);

    ///////////////////////////////////////////////////
    //// Mint the initial supply of treasury token
    //// and send it all to the DAO directly
    ///////////////////////////////////////////////////
    debug!(target: "demo", "Stage 2. Minting treasury token");

    cache.track(dao_keypair.secret);

    //// Wallet

    // Address of deployed contract in our example is dao::exec::FUNC_ID
    // This field is public, you can see it's being sent to a DAO
    // but nothing else is visible.
    //
    // In the python code we wrote:
    //
    //   spend_hook = b"0xdao_ruleset"
    //
    let spend_hook = *dao::exec::FUNC_ID;
    // The user_data can be a simple hash of the items passed into the ZK proof
    // up to corresponding linked ZK proof to interpret however they need.
    // In out case, it's the bulla for the DAO
    let user_data = dao_bulla.0;

    let builder = money::transfer::wallet::Builder {
        clear_inputs: vec![money::transfer::wallet::BuilderClearInputInfo {
            value: xdrk_supply,
            token_id: xdrk_token_id,
            signature_secret: cashier_signature_secret,
        }],
        inputs: vec![],
        outputs: vec![money::transfer::wallet::BuilderOutputInfo {
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

    let mut signatures = vec![];
    for func_call in &func_calls {
        let sign = sign([cashier_signature_secret].to_vec(), func_call);
        signatures.push(sign);
    }

    let tx = Transaction { func_calls, signatures };

    //// Validator

    let mut updates = vec![];
    // Validate all function calls in the tx
    for (idx, func_call) in tx.func_calls.iter().enumerate() {
        // So then the verifier will lookup the corresponding state_transition and apply
        // functions based off the func_id
        if func_call.func_id == *money::transfer::FUNC_ID {
            debug!("money::transfer::state_transition()");

            let update = money::transfer::validate::state_transition(&states, idx, &tx)
                .expect("money::transfer::validate::state_transition() failed!");
            updates.push(update);
        }
    }

    // Atomically apply all changes
    for update in updates {
        update.apply(&mut states);
    }

    tx.zk_verify(&zk_bins).unwrap();
    tx.verify_sigs();

    //// Wallet
    // DAO reads the money received from the encrypted note
    {
        assert_eq!(tx.func_calls.len(), 1);
        let func_call = &tx.func_calls[0];
        let call_data = func_call.call_data.as_any();
        assert_eq!((*call_data).type_id(), TypeId::of::<money::transfer::validate::CallData>());
        let call_data = call_data.downcast_ref::<money::transfer::validate::CallData>().unwrap();

        for output in &call_data.outputs {
            let coin = &output.revealed.coin;
            let enc_note = &output.enc_note;

            cache.try_decrypt_note(*coin, enc_note);
        }
    }

    let mut recv_coins = cache.get_received(&dao_keypair.secret);
    assert_eq!(recv_coins.len(), 1);
    let dao_recv_coin = recv_coins.pop().unwrap();
    let treasury_note = dao_recv_coin.note;

    // Check the actual coin received is valid before accepting it

    let (pub_x, pub_y) = dao_keypair.public.xy();
    let coin = poseidon_hash::<8>([
        pub_x,
        pub_y,
        DrkValue::from(treasury_note.value),
        treasury_note.token_id.inner(),
        treasury_note.serial,
        treasury_note.spend_hook,
        treasury_note.user_data,
        treasury_note.coin_blind,
    ]);
    assert_eq!(coin, dao_recv_coin.coin.0);

    assert_eq!(treasury_note.spend_hook, *dao::exec::FUNC_ID);
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

    cache.track(gov_keypair_1.secret);
    cache.track(gov_keypair_2.secret);
    cache.track(gov_keypair_3.secret);

    let gov_keypairs = vec![gov_keypair_1, gov_keypair_2, gov_keypair_3];

    // Spend hook and user data disabled
    let spend_hook = DrkSpendHook::from(0);
    let user_data = DrkUserData::from(0);

    let output1 = money::transfer::wallet::BuilderOutputInfo {
        value: 400000,
        token_id: gdrk_token_id,
        public: gov_keypair_1.public,
        serial: pallas::Base::random(&mut OsRng),
        coin_blind: pallas::Base::random(&mut OsRng),
        spend_hook,
        user_data,
    };

    let output2 = money::transfer::wallet::BuilderOutputInfo {
        value: 400000,
        token_id: gdrk_token_id,
        public: gov_keypair_2.public,
        serial: pallas::Base::random(&mut OsRng),
        coin_blind: pallas::Base::random(&mut OsRng),
        spend_hook,
        user_data,
    };

    let output3 = money::transfer::wallet::BuilderOutputInfo {
        value: 200000,
        token_id: gdrk_token_id,
        public: gov_keypair_3.public,
        serial: pallas::Base::random(&mut OsRng),
        coin_blind: pallas::Base::random(&mut OsRng),
        spend_hook,
        user_data,
    };

    assert!(2 * 400000 + 200000 == gdrk_supply);

    let builder = money::transfer::wallet::Builder {
        clear_inputs: vec![money::transfer::wallet::BuilderClearInputInfo {
            value: gdrk_supply,
            token_id: gdrk_token_id,
            signature_secret: cashier_signature_secret,
        }],
        inputs: vec![],
        outputs: vec![output1, output2, output3],
    };

    let func_call = builder.build(&zk_bins)?;
    let func_calls = vec![func_call];

    let mut signatures = vec![];
    for func_call in &func_calls {
        let sign = sign([cashier_signature_secret].to_vec(), func_call);
        signatures.push(sign);
    }

    let tx = Transaction { func_calls, signatures };

    //// Validator

    let mut updates = vec![];
    // Validate all function calls in the tx
    for (idx, func_call) in tx.func_calls.iter().enumerate() {
        // So then the verifier will lookup the corresponding state_transition and apply
        // functions based off the func_id
        if func_call.func_id == *money::transfer::FUNC_ID {
            debug!("money::transfer::state_transition()");

            let update = money::transfer::validate::state_transition(&states, idx, &tx)
                .expect("money::transfer::validate::state_transition() failed!");
            updates.push(update);
        }
    }

    // Atomically apply all changes
    for update in updates {
        update.apply(&mut states);
    }

    tx.zk_verify(&zk_bins).unwrap();
    tx.verify_sigs();

    //// Wallet
    {
        assert_eq!(tx.func_calls.len(), 1);
        let func_call = &tx.func_calls[0];
        let call_data = func_call.call_data.as_any();
        assert_eq!((*call_data).type_id(), TypeId::of::<money::transfer::validate::CallData>());
        let call_data = call_data.downcast_ref::<money::transfer::validate::CallData>().unwrap();

        for output in &call_data.outputs {
            let coin = &output.revealed.coin;
            let enc_note = &output.enc_note;

            cache.try_decrypt_note(*coin, enc_note);
        }
    }

    let mut gov_recv = vec![None, None, None];
    // Check that each person received one coin
    for (i, key) in gov_keypairs.iter().enumerate() {
        let gov_recv_coin = {
            let mut recv_coins = cache.get_received(&key.secret);
            assert_eq!(recv_coins.len(), 1);
            let recv_coin = recv_coins.pop().unwrap();
            let note = &recv_coin.note;

            assert_eq!(note.token_id, gdrk_token_id);
            // Normal payment
            assert_eq!(note.spend_hook, pallas::Base::from(0));
            assert_eq!(note.user_data, pallas::Base::from(0));

            let (pub_x, pub_y) = key.public.xy();
            let coin = poseidon_hash::<8>([
                pub_x,
                pub_y,
                DrkValue::from(note.value),
                note.token_id.inner(),
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
        let tree = &cache.tree;
        let leaf_position = gov_recv[0].leaf_position;
        let root = tree.root(0).unwrap();
        let merkle_path = tree.authentication_path(leaf_position, &root).unwrap();
        (leaf_position, merkle_path)
    };

    // TODO: is it possible for an invalid transfer() to be constructed on exec()?
    //       need to look into this
    let signature_secret = SecretKey::random(&mut OsRng);
    let input = dao::propose::wallet::BuilderInput {
        secret: gov_keypair_1.secret,
        note: gov_recv[0].note.clone(),
        leaf_position: money_leaf_position,
        merkle_path: money_merkle_path,
        signature_secret,
    };

    let (dao_merkle_path, dao_merkle_root) = {
        let state = states.lookup::<dao::State>(*dao::CONTRACT_ID).unwrap();
        let tree = &state.dao_tree;
        let root = tree.root(0).unwrap();
        let merkle_path = tree.authentication_path(dao_leaf_position, &root).unwrap();
        (merkle_path, root)
    };

    let dao_params = dao::mint::wallet::DaoParams {
        proposer_limit: dao_proposer_limit,
        quorum: dao_quorum,
        approval_ratio_base: dao_approval_ratio_base,
        approval_ratio_quot: dao_approval_ratio_quot,
        gov_token_id: gdrk_token_id,
        public_key: dao_keypair.public,
        bulla_blind: dao_bulla_blind,
    };

    let proposal = dao::propose::wallet::Proposal {
        dest: user_keypair.public,
        amount: 1000,
        serial: pallas::Base::random(&mut OsRng),
        token_id: xdrk_token_id,
        blind: pallas::Base::random(&mut OsRng),
    };

    let builder = dao::propose::wallet::Builder {
        inputs: vec![input],
        proposal,
        dao: dao_params.clone(),
        dao_leaf_position,
        dao_merkle_path,
        dao_merkle_root,
    };

    let func_call = builder.build(&zk_bins);
    let func_calls = vec![func_call];

    let mut signatures = vec![];
    for func_call in &func_calls {
        let sign = sign([signature_secret].to_vec(), func_call);
        signatures.push(sign);
    }

    let tx = Transaction { func_calls, signatures };

    //// Validator

    let mut updates = vec![];
    // Validate all function calls in the tx
    for (idx, func_call) in tx.func_calls.iter().enumerate() {
        if func_call.func_id == *dao::propose::FUNC_ID {
            debug!(target: "demo", "dao::propose::state_transition()");

            let update = dao::propose::validate::state_transition(&states, idx, &tx)
                .expect("dao::propose::validate::state_transition() failed!");
            updates.push(update);
        }
    }

    // Atomically apply all changes
    for update in updates {
        update.apply(&mut states);
    }

    tx.zk_verify(&zk_bins).unwrap();
    tx.verify_sigs();

    //// Wallet

    // Read received proposal
    let (proposal, proposal_bulla) = {
        assert_eq!(tx.func_calls.len(), 1);
        let func_call = &tx.func_calls[0];
        let call_data = func_call.call_data.as_any();
        assert_eq!((*call_data).type_id(), TypeId::of::<dao::propose::validate::CallData>());
        let call_data = call_data.downcast_ref::<dao::propose::validate::CallData>().unwrap();

        let header = &call_data.header;
        let note: dao::propose::wallet::Note =
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
        let tree = &cache.tree;
        let leaf_position = gov_recv[0].leaf_position;
        let root = tree.root(0).unwrap();
        let merkle_path = tree.authentication_path(leaf_position, &root).unwrap();
        (leaf_position, merkle_path)
    };

    let signature_secret = SecretKey::random(&mut OsRng);
    let input = dao::vote::wallet::BuilderInput {
        secret: gov_keypair_1.secret,
        note: gov_recv[0].note.clone(),
        leaf_position: money_leaf_position,
        merkle_path: money_merkle_path,
        signature_secret,
    };

    let vote_option: bool = true;
    // assert!(vote_option || !vote_option); // wtf

    // We create a new keypair to encrypt the vote.
    // For the demo MVP, you can just use the dao_keypair secret
    let vote_keypair_1 = Keypair::random(&mut OsRng);

    let builder = dao::vote::wallet::Builder {
        inputs: vec![input],
        vote: dao::vote::wallet::Vote {
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

    let mut signatures = vec![];
    for func_call in &func_calls {
        let sign = sign([signature_secret].to_vec(), func_call);
        signatures.push(sign);
    }

    let tx = Transaction { func_calls, signatures };

    //// Validator

    let mut updates = vec![];
    // Validate all function calls in the tx
    for (idx, func_call) in tx.func_calls.iter().enumerate() {
        if func_call.func_id == *dao::vote::FUNC_ID {
            debug!(target: "demo", "dao::vote::state_transition()");

            let update = dao::vote::validate::state_transition(&states, idx, &tx)
                .expect("dao::vote::validate::state_transition() failed!");
            updates.push(update);
        }
    }

    // Atomically apply all changes
    for update in updates {
        update.apply(&mut states);
    }

    tx.zk_verify(&zk_bins).unwrap();
    tx.verify_sigs();

    //// Wallet

    // Secret vote info. Needs to be revealed at some point.
    // TODO: look into verifiable encryption for notes
    // TODO: look into timelock puzzle as a possibility
    let vote_note_1 = {
        assert_eq!(tx.func_calls.len(), 1);
        let func_call = &tx.func_calls[0];
        let call_data = func_call.call_data.as_any();
        assert_eq!((*call_data).type_id(), TypeId::of::<dao::vote::validate::CallData>());
        let call_data = call_data.downcast_ref::<dao::vote::validate::CallData>().unwrap();

        let header = &call_data.header;
        let note: dao::vote::wallet::Note =
            header.enc_note.decrypt(&vote_keypair_1.secret).unwrap();
        note
    };
    debug!(target: "demo", "User 1 voted!");
    debug!(target: "demo", "  vote_option: {}", vote_note_1.vote.vote_option);
    debug!(target: "demo", "  value: {}", vote_note_1.vote_value);

    // User 2: NO

    let (money_leaf_position, money_merkle_path) = {
        let tree = &cache.tree;
        let leaf_position = gov_recv[1].leaf_position;
        let root = tree.root(0).unwrap();
        let merkle_path = tree.authentication_path(leaf_position, &root).unwrap();
        (leaf_position, merkle_path)
    };

    let signature_secret = SecretKey::random(&mut OsRng);
    let input = dao::vote::wallet::BuilderInput {
        secret: gov_keypair_2.secret,
        note: gov_recv[1].note.clone(),
        leaf_position: money_leaf_position,
        merkle_path: money_merkle_path,
        signature_secret,
    };

    let vote_option: bool = false;
    // assert!(vote_option || !vote_option); // wtf

    // We create a new keypair to encrypt the vote.
    let vote_keypair_2 = Keypair::random(&mut OsRng);

    let builder = dao::vote::wallet::Builder {
        inputs: vec![input],
        vote: dao::vote::wallet::Vote {
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

    let mut signatures = vec![];
    for func_call in &func_calls {
        let sign = sign([signature_secret].to_vec(), func_call);
        signatures.push(sign);
    }

    let tx = Transaction { func_calls, signatures };

    //// Validator

    let mut updates = vec![];
    // Validate all function calls in the tx
    for (idx, func_call) in tx.func_calls.iter().enumerate() {
        if func_call.func_id == *dao::vote::FUNC_ID {
            debug!(target: "demo", "dao::vote::state_transition()");

            let update = dao::vote::validate::state_transition(&states, idx, &tx)
                .expect("dao::vote::validate::state_transition() failed!");
            updates.push(update);
        }
    }

    // Atomically apply all changes
    for update in updates {
        update.apply(&mut states);
    }

    tx.zk_verify(&zk_bins).unwrap();
    tx.verify_sigs();

    //// Wallet

    // Secret vote info. Needs to be revealed at some point.
    // TODO: look into verifiable encryption for notes
    // TODO: look into timelock puzzle as a possibility
    let vote_note_2 = {
        assert_eq!(tx.func_calls.len(), 1);
        let func_call = &tx.func_calls[0];
        let call_data = func_call.call_data.as_any();
        assert_eq!((*call_data).type_id(), TypeId::of::<dao::vote::validate::CallData>());
        let call_data = call_data.downcast_ref::<dao::vote::validate::CallData>().unwrap();

        let header = &call_data.header;
        let note: dao::vote::wallet::Note =
            header.enc_note.decrypt(&vote_keypair_2.secret).unwrap();
        note
    };
    debug!(target: "demo", "User 2 voted!");
    debug!(target: "demo", "  vote_option: {}", vote_note_2.vote.vote_option);
    debug!(target: "demo", "  value: {}", vote_note_2.vote_value);

    // User 3: YES

    let (money_leaf_position, money_merkle_path) = {
        let tree = &cache.tree;
        let leaf_position = gov_recv[2].leaf_position;
        let root = tree.root(0).unwrap();
        let merkle_path = tree.authentication_path(leaf_position, &root).unwrap();
        (leaf_position, merkle_path)
    };

    let signature_secret = SecretKey::random(&mut OsRng);
    let input = dao::vote::wallet::BuilderInput {
        secret: gov_keypair_3.secret,
        note: gov_recv[2].note.clone(),
        leaf_position: money_leaf_position,
        merkle_path: money_merkle_path,
        signature_secret,
    };

    let vote_option: bool = true;
    // assert!(vote_option || !vote_option); // wtf

    // We create a new keypair to encrypt the vote.
    let vote_keypair_3 = Keypair::random(&mut OsRng);

    let builder = dao::vote::wallet::Builder {
        inputs: vec![input],
        vote: dao::vote::wallet::Vote {
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

    let mut signatures = vec![];
    for func_call in &func_calls {
        let sign = sign([signature_secret].to_vec(), func_call);
        signatures.push(sign);
    }

    let tx = Transaction { func_calls, signatures };

    //// Validator

    let mut updates = vec![];
    // Validate all function calls in the tx
    for (idx, func_call) in tx.func_calls.iter().enumerate() {
        if func_call.func_id == *dao::vote::FUNC_ID {
            debug!(target: "demo", "dao::vote::state_transition()");

            let update = dao::vote::validate::state_transition(&states, idx, &tx)
                .expect("dao::vote::validate::state_transition() failed!");
            updates.push(update);
        }
    }

    // Atomically apply all changes
    for update in updates {
        update.apply(&mut states);
    }

    tx.zk_verify(&zk_bins).unwrap();
    tx.verify_sigs();

    //// Wallet

    // Secret vote info. Needs to be revealed at some point.
    // TODO: look into verifiable encryption for notes
    // TODO: look into timelock puzzle as a possibility
    let vote_note_3 = {
        assert_eq!(tx.func_calls.len(), 1);
        let func_call = &tx.func_calls[0];
        let call_data = func_call.call_data.as_any();
        assert_eq!((*call_data).type_id(), TypeId::of::<dao::vote::validate::CallData>());
        let call_data = call_data.downcast_ref::<dao::vote::validate::CallData>().unwrap();

        let header = &call_data.header;
        let note: dao::vote::wallet::Note =
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
        let tree = &cache.tree;
        let leaf_position = dao_recv_coin.leaf_position;
        let root = tree.root(0).unwrap();
        let merkle_path = tree.authentication_path(leaf_position, &root).unwrap();
        (leaf_position, merkle_path)
    };

    let input = money::transfer::wallet::BuilderInputInfo {
        leaf_position: treasury_leaf_position,
        merkle_path: treasury_merkle_path,
        secret: dao_keypair.secret,
        note: treasury_note,
        user_data_blind,
        value_blind: input_value_blind,
        signature_secret: tx_signature_secret,
    };

    let builder = money::transfer::wallet::Builder {
        clear_inputs: vec![],
        inputs: vec![input],
        outputs: vec![
            // Sending money
            money::transfer::wallet::BuilderOutputInfo {
                value: 1000,
                token_id: xdrk_token_id,
                public: user_keypair.public,
                serial: proposal.serial,
                coin_blind: proposal.blind,
                spend_hook: pallas::Base::from(0),
                user_data: pallas::Base::from(0),
            },
            // Change back to DAO
            money::transfer::wallet::BuilderOutputInfo {
                value: xdrk_supply - 1000,
                token_id: xdrk_token_id,
                public: dao_keypair.public,
                serial: dao_serial,
                coin_blind: dao_coin_blind,
                spend_hook: *dao::exec::FUNC_ID,
                user_data: dao_bulla.0,
            },
        ],
    };

    let transfer_func_call = builder.build(&zk_bins)?;

    let builder = dao::exec::wallet::Builder {
        proposal,
        dao: dao_params.clone(),
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
        hook_dao_exec: *dao::exec::FUNC_ID,
        signature_secret: exec_signature_secret,
    };
    let exec_func_call = builder.build(&zk_bins);
    let func_calls = vec![transfer_func_call, exec_func_call];

    let mut signatures = vec![];
    for func_call in &func_calls {
        let sign = sign([signature_secret].to_vec(), func_call);
        signatures.push(sign);
    }

    let tx = Transaction { func_calls, signatures };

    {
        // Now the spend_hook field specifies the function DAO::exec()
        // so Money::transfer() must also be combined with DAO::exec()

        assert_eq!(tx.func_calls.len(), 2);
        let transfer_func_call = &tx.func_calls[0];
        let transfer_call_data = transfer_func_call.call_data.as_any();

        assert_eq!(
            (*transfer_call_data).type_id(),
            TypeId::of::<money::transfer::validate::CallData>()
        );
        let transfer_call_data =
            transfer_call_data.downcast_ref::<money::transfer::validate::CallData>();
        let transfer_call_data = transfer_call_data.unwrap();
        // At least one input has this field value which means DAO::exec() is invoked.
        assert_eq!(transfer_call_data.inputs.len(), 1);
        let input = &transfer_call_data.inputs[0];
        assert_eq!(input.revealed.spend_hook, *dao::exec::FUNC_ID);
        let user_data_enc = poseidon_hash::<2>([dao_bulla.0, user_data_blind]);
        assert_eq!(input.revealed.user_data_enc, user_data_enc);

        let (dao_pub_x, dao_pub_y) = dao_params.public_key.xy();
        let coin_1 = Coin(poseidon_hash::<8>([
            dao_pub_x,
            dao_pub_y,
            pallas::Base::from(xdrk_supply - 1000),
            xdrk_token_id.inner(),
            dao_serial,
            *dao::exec::FUNC_ID,
            dao_bulla.0,
            dao_coin_blind,
        ]));
        debug!("coin_1: {:?}", coin_1);

        let money_transfer_call_data = tx.func_calls[0].call_data.as_any();
        let money_transfer_call_data =
            money_transfer_call_data.downcast_ref::<money::transfer::validate::CallData>();
        let money_transfer_call_data = money_transfer_call_data.unwrap();
        assert_eq!(
            money_transfer_call_data.type_id(),
            TypeId::of::<money::transfer::validate::CallData>()
        );
        assert_eq!(money_transfer_call_data.outputs.len(), 2);
        let money_transfer_coin_1 = &money_transfer_call_data.outputs[1].revealed.coin;
        debug!("money::transfer() coin 1 = {:?}", money_transfer_coin_1);

        let dao_exec_call_data = tx.func_calls[1].call_data.as_any();
        let dao_exec_call_data = dao_exec_call_data.downcast_ref::<dao::exec::validate::CallData>();
        let dao_exec_call_data = dao_exec_call_data.unwrap();
        assert_eq!(dao_exec_call_data.type_id(), TypeId::of::<dao::exec::validate::CallData>());
        let dao_exec_coin_1 = &dao_exec_call_data.coin_1;
        debug!("dao::exec() coin 1 = {:?}", dao_exec_coin_1);

        assert_eq!(coin_1, *money_transfer_coin_1);
        assert_eq!(coin_1, Coin(*dao_exec_coin_1));
    }

    //// Validator

    let mut updates = vec![];
    // Validate all function calls in the tx
    for (idx, func_call) in tx.func_calls.iter().enumerate() {
        if func_call.func_id == *dao::exec::FUNC_ID {
            debug!("dao::exec::state_transition()");

            let update = dao::exec::validate::state_transition(&states, idx, &tx)
                .expect("dao::exec::validate::state_transition() failed!");
            updates.push(update);
        } else if func_call.func_id == *money::transfer::FUNC_ID {
            debug!("money::transfer::state_transition()");

            let update = money::transfer::validate::state_transition(&states, idx, &tx)
                .expect("money::transfer::validate::state_transition() failed!");
            updates.push(update);
        }
    }

    // Atomically apply all changes
    for update in updates {
        update.apply(&mut states);
    }

    // Other stuff
    tx.zk_verify(&zk_bins).unwrap();
    tx.verify_sigs();

    //// Wallet

    Ok(())
}
