use darkfi::{
    crypto::nullifier::Nullifier,
    node::{MemoryState, State},
    runtime::{util::serialize_payload, vm_runtime::Runtime},
    serial::serialize,
    Result,
};
use darkfi_sdk::pasta::pallas;

use smart_contract::Args;

#[test]
fn run_contract() -> Result<()> {
    // Debug log configuration
    let mut cfg = simplelog::ConfigBuilder::new();
    cfg.add_filter_ignore("sled".to_string());
    simplelog::TermLogger::init(
        simplelog::LevelFilter::Debug,
        cfg.build(),
        simplelog::TerminalMode::Mixed,
        simplelog::ColorChoice::Auto,
    )?;

    // =============================================================
    // Build a ledger state so the runtime has something to work on
    // =============================================================
    let state_machine = State::dummy()?;

    // Add a nullifier to the nullifier set. (This is checked by the contract)
    state_machine.nullifiers.insert(&[Nullifier::from(pallas::Base::from(0x10))])?;

    // ================================================================
    // Load the wasm binary into memory and create an execution runtime
    // ================================================================
    let wasm_bytes = std::fs::read("contract.wasm")?;
    let mut runtime = Runtime::new(&wasm_bytes, MemoryState::new(state_machine))?;

    // =============================================
    // Build some kind of payload to show an example
    // =============================================
    let args = Args { a: 777, b: 666 };
    let payload = serialize(&args);

    // ============================================================
    // Serialize the payload into the runtime format and execute it
    // ============================================================
    runtime.run(&serialize_payload(&payload))
}
