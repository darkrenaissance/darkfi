use darkfi::{
    blockchain::Blockchain,
    consensus::{TESTNET_GENESIS_HASH_BYTES, TESTNET_GENESIS_TIMESTAMP},
    runtime::vm_runtime::Runtime,
    Result,
};
use darkfi_sdk::{crypto::ContractId, pasta::pallas, tx::ContractCall};
use darkfi_serial::{serialize, deserialize, Decodable, Encodable, WriteExt};
use std::io::Cursor;

use dao_contract::{DaoFunction, DaoMintParams};

mod contract;
mod error;
mod note;
mod schema;
mod util;

fn show_dao_state(chain: &Blockchain, contract_id: &ContractId) -> Result<()> {
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
    schema::schema().await?;
    return Ok(());

    // =============================
    // Initialize a dummy blockchain
    // =============================
    // TODO: This blockchain interface should perhaps be ValidatorState and Mutex/RwLock.
    let db = sled::Config::new().temporary(true).open()?;
    let blockchain = Blockchain::new(&db, *TESTNET_GENESIS_TIMESTAMP, *TESTNET_GENESIS_HASH_BYTES)?;

    // ================================================================
    // Load the wasm binary into memory and create an execution runtime
    // ================================================================
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

    show_dao_state(&blockchain, &dao_contract_id)?;

    Ok(())
}
