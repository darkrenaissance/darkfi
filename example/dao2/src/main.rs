use darkfi::{
    blockchain::Blockchain,
    consensus::{TESTNET_GENESIS_HASH_BYTES, TESTNET_GENESIS_TIMESTAMP},
    crypto::{
        coin::Coin,
        proof::{ProvingKey, VerifyingKey},
        types::{DrkSpendHook, DrkUserData, DrkValue},
        util::poseidon_hash,
    },
    runtime::vm_runtime::Runtime,
    zk::circuit::{BurnContract, MintContract},
    zkas::decoder::ZkBinary,
    Result,
};
use darkfi_sdk::{
    crypto::{
        constants::MERKLE_DEPTH, pedersen::pedersen_commitment_u64, ContractId, Keypair,
        MerkleNode, MerkleTree, PublicKey, SecretKey,
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

use crate::{
    contract::{dao, example, money},
    note::EncryptedNote2,
    schema::WalletCache,
    tx::Transaction,
    util::{sign, StateRegistry, ZkContractTable},
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
    debug!(target: "demo", "DAO tree: {} bytes", value.len());
    debug!(target: "demo", "set size: {}", set_size);

    let db_roots = chain.contracts.lookup(&chain.sled_db, contract_id, "dao_roots")?;
    for i in 0..set_size {
        let root = db_roots.get(&serialize(&i)).expect("dao_roots").unwrap();
        let root: MerkleNode = deserialize(&root)?;
        debug!(target: "demo", "root {}: {:?}", i, root);
    }

    Ok(())
}

fn show_money_state(chain: &Blockchain, contract_id: &ContractId) -> Result<()> {
    let db = chain.contracts.lookup(&chain.sled_db, contract_id, "wagies")?;
    for obj in db.iter() {
        let (key, value) = obj.unwrap();
        let name: String = deserialize(&key)?;
        let age: u32 = deserialize(&value)?;
        println!("{}: {}", name, age);
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
    //tx.verify_sigs();

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
    let xdrk_token_id = pallas::Base::random(&mut OsRng);

    // Governance token parameters
    let gdrk_supply = 1_000_000;
    let gdrk_token_id = pallas::Base::random(&mut OsRng);

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

    /*
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
    */

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

    /////////////////////////////////////////////////
    // Old stuff
    /////////////////////////////////////////////////
    /*
    let wasm_bytes = std::fs::read("dao_contract.wasm")?;
    let dao_contract_id = ContractId::from(pallas::Base::from(1));
    let mut runtime = Runtime::new(&wasm_bytes, blockchain.clone(), dao_contract_id)?;

    // Deploy function to initialize the smart contract state.
    // Here we pass an empty payload, but it's possible to feed in arbitrary data.
    runtime.deploy(&[])?;

    // This is another call so we instantiate a new runtime.
    let mut runtime = Runtime::new(&wasm_bytes, blockchain.clone(), dao_contract_id)?;

    // =============================================
    // Build some kind of payload to show an example
    // =============================================
    // Write the actual call data
    let mut calldata = Vec::new();
    // Selects which path executes in the contract.
    calldata.write_u8(DaoFunction::Mint as u8)?;
    let params = DaoMintParams { a: 777, b: 666 };
    params.encode(&mut calldata)?;

    let func_calls = vec![ContractCall {
        contract_id: dao_contract_id,
        calldata
    }];

    let mut payload = Vec::new();
    //// Write the actual payload data
    let call_index = 0;
    payload.write_u32(call_index)?;
    func_calls.encode(&mut payload)?;

    // ============================================================
    // Serialize the payload into the runtime format and execute it
    // ============================================================
    let update = runtime.exec(&payload)?;

    // =====================================================
    // If exec was successful, try to apply the state change
    // =====================================================
    runtime.apply(&update)?;

    // =====================================================
    // Verify ZK proofs and signatures
    // =====================================================
    let metadata = runtime.metadata(&payload)?;
    let mut decoder = Cursor::new(&metadata);
    let zk_public_values: Vec<(String, Vec<pallas::Base>)> = Decodable::decode(&mut decoder)?;
    let signature_public_keys: Vec<pallas::Point> = Decodable::decode(decoder)?;
    */

    show_dao_state(&blockchain, &dao_contract_id)?;
    show_money_state(&blockchain, &money_contract_id)?;

    Ok(())
}
