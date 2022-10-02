use borsh::BorshSerialize;
use darkfi::{
    runtime::{util::serialize_payload, vm_runtime::Runtime},
    serial::serialize,
    Result,
};
use pasta_curves::pallas;

use smart_contract::{Args, Foo};

#[test]
fn run_contract() -> Result<()> {
    simplelog::TermLogger::init(
        simplelog::LevelFilter::Debug,
        simplelog::Config::default(),
        simplelog::TerminalMode::Mixed,
        simplelog::ColorChoice::Auto,
    )?;

    let wasm_bytes = std::fs::read("smart_contract.wasm")?;
    let mut runtime = Runtime::new(&wasm_bytes)?;

    let _args = Args { a: pallas::Base::from(777), b: pallas::Base::from(666) };
    let _payload = _args.try_to_vec()?;

    let args = Foo { a: 777, b: 666 };
    let payload = serialize(&args);

    let input = serialize_payload(&payload);

    runtime.run(&input)
}
