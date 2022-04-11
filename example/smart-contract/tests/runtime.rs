use borsh::BorshSerialize;
use darkfi::{
    runtime::{util::serialize_payload, vm_runtime::Runtime},
    Result,
};
use pasta_curves::pallas;

use smart_contract::Args;

#[test]
fn run_contract() -> Result<()> {
    let wasm_bytes = std::fs::read("smart_contract.wasm")?;
    let mut runtime = Runtime::new(&wasm_bytes)?;

    let args = Args { a: pallas::Base::from(777), b: pallas::Base::from(666) };
    let payload = args.try_to_vec()?;

    let input = serialize_payload(&payload);

    runtime.run(&input)
}
