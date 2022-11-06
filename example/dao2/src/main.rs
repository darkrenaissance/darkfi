use darkfi::{
    blockchain::Blockchain,
    consensus::{TESTNET_GENESIS_HASH_BYTES, TESTNET_GENESIS_TIMESTAMP},
    runtime::vm_runtime::Runtime,
    Result,
};
use darkfi_sdk::{crypto::ContractId, pasta::pallas, tx::FuncCall};
use darkfi_serial::{serialize, Decodable, Encodable, WriteExt};
use std::io::Cursor;

use dao_contract::DaoFunction;

fn main() -> Result<()> {
    println!("wakie wakie young wagie");

    // Debug log configuration
    let mut cfg = simplelog::ConfigBuilder::new();
    cfg.add_filter_ignore("sled".to_string());
    simplelog::TermLogger::init(
        simplelog::LevelFilter::Debug,
        cfg.build(),
        simplelog::TerminalMode::Mixed,
        simplelog::ColorChoice::Auto,
    )?;

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
    let contract_id = ContractId::from(pallas::Base::from(1));
    let mut runtime = Runtime::new(&wasm_bytes, blockchain.clone(), contract_id)?;

    // Deploy function to initialize the smart contract state.
    // Here we pass an empty payload, but it's possible to feed in arbitrary data.
    runtime.deploy(&[])?;

    // This is another call so we instantiate a new runtime.
    let mut runtime = Runtime::new(&wasm_bytes, blockchain, contract_id)?;

    // =============================================
    // Build some kind of payload to show an example
    // =============================================
    let func_calls = vec![FuncCall {
        contract_id: pallas::Base::from(110),
        func_id: pallas::Base::from(4),
        //call_data: serialize(&FooCallData { a: 777, b: 666 }),
        call_data: Vec::new()
    }];
    let func_call_index: u32 = 0;

    let mut payload = Vec::new();
    // Selects which path executes in the contract.
    //payload.write_u8(Function::Foo as u8)?;
    //// Write the actual payload data
    //payload.write_u32(func_call_index)?;
    //func_calls.encode(&mut payload)?;

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

    Ok(())
}
