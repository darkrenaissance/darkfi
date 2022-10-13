use incrementalmerkletree::bridgetree::BridgeTree;
use lazy_init::Lazy;
use pasta_curves::pallas;

use darkfi::{
    blockchain::Blockchain,
    consensus::{TESTNET_GENESIS_HASH_BYTES, TESTNET_GENESIS_TIMESTAMP},
    crypto::{merkle_node::MerkleNode, nullifier::Nullifier},
    node::{MemoryState, State},
    runtime::{util::serialize_payload, vm_runtime::Runtime},
    serial::serialize,
    Result,
};

use smart_contract::Args;

#[test]
fn run_contract() -> Result<()> {
    let mut logcfg = simplelog::ConfigBuilder::new();
    logcfg.add_filter_ignore("sled".to_string());
    simplelog::TermLogger::init(
        simplelog::LevelFilter::Debug,
        logcfg.build(),
        simplelog::TerminalMode::Mixed,
        simplelog::ColorChoice::Auto,
    )?;

    // ============================================================
    // Build a ledger state so the runtime has something to work on
    // ============================================================
    let state_machine = MemoryState::new(State::dummy())?;

    // We check if this nullifier is in the set from the contract
    state_machine.nullifiers.insert(&[Nullifier::from(pallas::Base::from(0x10))])?;

    // ================================================================
    // Load the wasm binary into memory and create an execution runtime
    // ================================================================
    let wasm_bytes = std::fs::read("smart_contract.wasm")?;
    let mut runtime = Runtime::new(&wasm_bytes, MemoryState::new(state_machine))?;

    // ===========================================================
    // Build some kind of payload for the wasm entrypoint function
    // ===========================================================
    let args = Args { a: 777, b: 666 };
    let payload = serialize(&args);

    // ============================================================
    // Serialize the payload into the runtime format and execute it
    // ============================================================
    runtime.run(&serialize_payload(&payload))
}
