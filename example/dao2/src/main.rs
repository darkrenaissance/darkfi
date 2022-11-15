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

use darkfi::{
    blockchain::Blockchain,
    consensus::{TESTNET_GENESIS_HASH_BYTES, TESTNET_GENESIS_TIMESTAMP},
    crypto::{
        coin::Coin,
        proof::{ProvingKey, VerifyingKey},
        types::{DrkSpendHook, DrkUserData, DrkValue},
    },
    runtime::vm_runtime::Runtime,
    zk::circuit::{BurnContract, MintContract},
    zkas::decoder::ZkBinary,
    Result,
};
use darkfi_sdk::{
    crypto::{
        constants::MERKLE_DEPTH, pedersen::pedersen_commitment_u64, poseidon_hash,
        schnorr::SchnorrSecret, ContractId, Keypair, MerkleNode, MerkleTree, PublicKey, SecretKey,
        TokenId,
    },
    tx::ContractCall,
};
use darkfi_serial::{deserialize, serialize, Decodable, Encodable, WriteExt};
use incrementalmerkletree::{bridgetree::BridgeTree, Tree};
use log::{debug, error};
use pasta_curves::{
    arithmetic::CurveAffine,
    group::{ff::Field, Curve},
    pallas,
};
use rand::rngs::OsRng;
use std::{
    any::{Any, TypeId},
    io::Cursor,
    time::Instant,
};

use dao_contract::{DaoFunction, DaoMintParams};
use money_contract::{MoneyFunction, MoneyTransferParams};

use crate::{
    contract::{dao, example, money},
    note::EncryptedNote2,
    schema::WalletCache,
    tx::Transaction,
    util::{StateRegistry, ZkContractTable},
};

mod contract;
mod error;
mod note;
mod schema;
mod tx;
mod util;

fn show_dao_state(chain: &Blockchain, contract_id: &ContractId) -> Result<()> {
    let db_info = chain.contracts.lookup(&chain.sled_db, contract_id, "info")?;
    let value = db_info.get(&serialize(&"dao_tree".to_string())).expect("dao_tree").unwrap();
    let mut decoder = Cursor::new(&value);
    let set_size: u32 = Decodable::decode(&mut decoder)?;
    let tree: MerkleTree = Decodable::decode(decoder)?;
    debug!(target: "demo", "DAO state:");
    debug!(target: "demo", "    tree: {} bytes", value.len());
    debug!(target: "demo", "    set size: {}", set_size);

    let db_roots = chain.contracts.lookup(&chain.sled_db, contract_id, "dao_roots")?;
    for i in 0..set_size {
        let root = db_roots.get(&serialize(&i)).expect("dao_roots").unwrap();
        let root: MerkleNode = deserialize(&root)?;
        debug!(target: "demo", "    root {}: {:?}", i, root);
    }

    Ok(())
}

fn show_money_state(chain: &Blockchain, contract_id: &ContractId) -> Result<()> {
    let db_info = chain.contracts.lookup(&chain.sled_db, contract_id, "info")?;
    let value = db_info.get(&serialize(&"coin_tree".to_string())).expect("coin_tree").unwrap();
    let mut decoder = Cursor::new(&value);
    let set_size: u32 = Decodable::decode(&mut decoder)?;
    let tree: MerkleTree = Decodable::decode(decoder)?;
    debug!(target: "demo", "Money state:");
    debug!(target: "demo", "    tree: {} bytes", value.len());
    debug!(target: "demo", "    set size: {}", set_size);

    let db_roots = chain.contracts.lookup(&chain.sled_db, contract_id, "coin_roots")?;
    for i in 0..set_size {
        let root = db_roots.get(&serialize(&i)).expect("coin_roots").unwrap();
        let root: MerkleNode = deserialize(&root)?;
        debug!(target: "demo", "    root {}: {:?}", i, root);
    }

    let db_nulls = chain.contracts.lookup(&chain.sled_db, contract_id, "info")?;
    debug!(target: "demo", "    nullifiers:");
    for obj in db_nulls.iter() {
        let (key, value) = obj.unwrap();
        debug!(target: "demo", "        {:02x?}", &key[..]);
    }
    Ok(())
}

type BoxResult<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn validate(
    tx: &Transaction,
    dao_wasm_bytes: &[u8],
    dao_contract_id: ContractId,
    money_wasm_bytes: &[u8],
    money_contract_id: ContractId,
    blockchain: &Blockchain,
    zk_bins: &ZkContractTable,
) -> Result<()> {
    // ContractId is not Hashable so put them in a Vec and do linear scan
    let wasm_bytes_lookup = vec![
        (dao_contract_id, "DAO", dao_wasm_bytes),
        (money_contract_id, "Money", money_wasm_bytes),
    ];

    // We can do all exec(), zk proof checks and signature verifies in parallel.
    let mut updates = vec![];
    let mut zkpublic_table = vec![];
    let mut sigpub_table = vec![];
    // Validate all function calls in the tx
    for (idx, call) in tx.calls.iter().enumerate() {
        // So then the verifier will lookup the corresponding state_transition and apply
        // functions based off the func_id

        // Write the actual payload data
        let mut payload = Vec::new();
        // Call index
        payload.write_u32(idx as u32)?;
        // Actuall calldata
        tx.calls.encode(&mut payload)?;

        // Lookup the wasm bytes
        let (_, contract_name, wasm_bytes) =
            wasm_bytes_lookup.iter().find(|(id, _name, _bytes)| *id == call.contract_id).unwrap();
        debug!(target: "demo", "{}::exec() contract called", contract_name);

        let mut runtime = Runtime::new(wasm_bytes, blockchain.clone(), call.contract_id)?;
        let update = runtime.exec(&payload)?;
        updates.push(update);

        let metadata = runtime.metadata(&payload)?;
        let mut decoder = Cursor::new(&metadata);
        let zk_public_values: Vec<(String, Vec<pallas::Base>)> = Decodable::decode(&mut decoder)?;
        let signature_public_keys: Vec<pallas::Point> = Decodable::decode(&mut decoder)?;

        zkpublic_table.push(zk_public_values);
        sigpub_table.push(signature_public_keys);
    }

    tx.zk_verify(&zk_bins, &zkpublic_table)?;
    tx.verify_sigs(&sigpub_table)?;

    // Now we finished verification stage, just apply all changes
    assert_eq!(tx.calls.len(), updates.len());
    for (call, update) in tx.calls.iter().zip(updates.iter()) {
        // Lookup the wasm bytes
        let (_, contract_name, wasm_bytes) =
            wasm_bytes_lookup.iter().find(|(id, _name, _bytes)| *id == call.contract_id).unwrap();
        debug!(target: "demo", "{}::apply() contract called", contract_name);

        let mut runtime = Runtime::new(wasm_bytes, blockchain.clone(), call.contract_id)?;

        runtime.apply(&update)?;
    }

    Ok(())
}

#[async_std::main]
async fn main() -> BoxResult<()> {
    // Debug log configuration
    let mut cfg = simplelog::ConfigBuilder::new();
    cfg.add_filter_ignore("sled".to_string());
    simplelog::TermLogger::init(
        simplelog::LevelFilter::Debug,
        cfg.build(),
        simplelog::TerminalMode::Mixed,
        simplelog::ColorChoice::Auto,
    )?;

    println!("wakie wakie young wagie");

    //return Ok(());

    //schema::schema().await?;
    //return Ok(());

    // =============================
    // Setup initial program parameters
    // =============================

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
    /*
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
    */

    // State for money contracts
    let cashier_signature_secret = SecretKey::random(&mut OsRng);
    let cashier_signature_public = PublicKey::from_secret(cashier_signature_secret);
    let faucet_signature_secret = SecretKey::random(&mut OsRng);
    let faucet_signature_public = PublicKey::from_secret(faucet_signature_secret);

    // We use this to receive coins
    let mut cache = WalletCache::new();

    // Initialize a dummy blockchain
    // TODO: This blockchain interface should perhaps be ValidatorState and Mutex/RwLock.
    let db = sled::Config::new().temporary(true).open()?;
    let blockchain = Blockchain::new(&db, *TESTNET_GENESIS_TIMESTAMP, *TESTNET_GENESIS_HASH_BYTES)?;

    // ================================================================
    // Deploy the wasm contracts
    // ================================================================

    let dao_wasm_bytes = std::fs::read("dao_contract.wasm")?;
    let dao_contract_id = ContractId::from(pallas::Base::from(1));
    let money_wasm_bytes = std::fs::read("money_contract.wasm")?;
    let money_contract_id = ContractId::from(pallas::Base::from(2));

    // Block 1
    // This has 2 transaction deploying the DAO and Money wasm contracts
    // together with their ZK proofs.
    {
        let mut dao_runtime = Runtime::new(&dao_wasm_bytes, blockchain.clone(), dao_contract_id)?;
        let mut money_runtime =
            Runtime::new(&money_wasm_bytes, blockchain.clone(), money_contract_id)?;

        // 1. exec() - zk and sig verify also
        // ... none in this block

        // 2. commit() - all apply() and deploy()
        // Deploy function to initialize the smart contract state.
        // Here we pass an empty payload, but it's possible to feed in arbitrary data.
        dao_runtime.deploy(&[])?;
        money_runtime.deploy(&[])?;
        debug!(target: "demo", "Deployed DAO and money contracts");
    }

    // ================================================================
    // DAO::mint()
    // ================================================================

    // Wallet
    let dao_keypair = Keypair::random(&mut OsRng);
    let dao_bulla_blind = pallas::Base::random(&mut OsRng);
    let tx = {
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
            signature_secret,
        };
        let (params, dao_mint_proofs) = builder.build(&zk_bins);

        // Write the actual call data
        let mut calldata = Vec::new();
        // Selects which path executes in the contract.
        calldata.write_u8(DaoFunction::Mint as u8)?;
        params.encode(&mut calldata)?;

        let calls = vec![ContractCall { contract_id: dao_contract_id, data: calldata }];

        let signatures = vec![];
        //for func_call in &func_calls {
        //    let sign = sign([signature_secret].to_vec(), func_call);
        //    signatures.push(sign);
        //}

        let proofs = vec![dao_mint_proofs];

        Transaction { calls, proofs, signatures }
    };

    //// Validator

    validate(
        &tx,
        &dao_wasm_bytes,
        dao_contract_id,
        &money_wasm_bytes,
        money_contract_id,
        &blockchain,
        &zk_bins,
    )
    .expect("validate failed");

    // Wallet stuff

    // In your wallet, wait until you see the tx confirmed before doing anything below
    // So for example keep track of tx hash
    //
    // We also need to loop through all newly added items to the validator node
    // and repeat the same for our local merkle tree. The order of added items
    // to local merkle trees must be the same.
    //
    // One way to do this would be that .apply() keeps an in-memory per block
    // list of the order txs were applied. So then we can repeat the same order
    // for our local wallet trees.
    //
    //   [ tx1, tx2, ... ]
    //
    // So the wallets know these are the new txs and this was the order they
    // were applied to the state in.
    // State updates are atomic so this will always be linear.
    //
    // When we see our DAO bulla, we call .witness()

    // We need to witness() the value in our local merkle tree
    let dao_bulla = {
        assert_eq!(tx.calls.len(), 1);
        let calldata = &tx.calls[0].data;
        let params_data = &calldata[1..];
        let params: DaoMintParams = Decodable::decode(params_data)?;
        params.dao_bulla.clone()
    };

    let mut dao_tree = MerkleTree::new(100);
    let dao_leaf_position = {
        let node = MerkleNode::from(dao_bulla.0);
        dao_tree.append(&node);
        dao_tree.witness().unwrap()
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
    let tx = {
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
        let (params, proofs) = builder.build(&zk_bins)?;

        // Write the actual call data
        let mut calldata = Vec::new();
        // Selects which path executes in the contract.
        calldata.write_u8(MoneyFunction::Transfer as u8)?;
        params.encode(&mut calldata)?;

        let calls = vec![ContractCall { contract_id: money_contract_id, data: calldata }];

        let proofs = vec![proofs];

        // We sign everything
        let mut unsigned_tx_data = vec![];
        calls.encode(&mut unsigned_tx_data)?;
        proofs.encode(&mut unsigned_tx_data)?;
        let signature = cashier_signature_secret.sign(&mut OsRng, &unsigned_tx_data[..]);

        // Our tx has a single contract call which itself has a single input
        let signatures = vec![vec![signature]];

        Transaction { calls, proofs, signatures }
    };

    //// Validator

    validate(
        &tx,
        &dao_wasm_bytes,
        dao_contract_id,
        &money_wasm_bytes,
        money_contract_id,
        &blockchain,
        &zk_bins,
    )
    .expect("validate failed");

    // Wallet stuff

    // DAO reads the money received from the encrypted note
    {
        assert_eq!(tx.calls.len(), 1);
        let calldata = &tx.calls[0].data;
        let params_data = &calldata[1..];
        let params: MoneyTransferParams = Decodable::decode(params_data)?;

        for output in params.outputs {
            let coin = output.coin;
            let enc_note = note::EncryptedNote2 {
                ciphertext: output.ciphertext,
                ephem_public: output.ephem_public,
            };

            let coin = Coin(coin);
            cache.try_decrypt_note(coin, &enc_note);
        }
    }

    let mut recv_coins = cache.get_received(&dao_keypair.secret);
    assert_eq!(recv_coins.len(), 1);
    let dao_recv_coin = recv_coins.pop().unwrap();
    let treasury_note = dao_recv_coin.note;

    // Check the actual coin received is valid before accepting it

    let coords = dao_keypair.public.inner().to_affine().coordinates().unwrap();
    let coin = poseidon_hash::<8>([
        *coords.x(),
        *coords.y(),
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

    show_dao_state(&blockchain, &dao_contract_id)?;
    show_money_state(&blockchain, &money_contract_id)?;

    Ok(())
}
