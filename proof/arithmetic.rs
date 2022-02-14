use darkfi::{
    crypto::{
        proof::{ProvingKey, VerifyingKey},
        Proof,
    },
    zk::vm::{Witness, ZkCircuit},
    zkas::decoder::ZkBinary,
    Result,
};
use log::info;
use pasta_curves::pallas;
use rand::rngs::OsRng;
use simplelog::{ColorChoice::Auto, Config, LevelFilter, TermLogger, TerminalMode::Mixed};

fn main() -> Result<()> {
    let loglevel = match option_env!("RUST_LOG") {
        Some("debug") => LevelFilter::Debug,
        Some("trace") => LevelFilter::Trace,
        Some(_) | None => LevelFilter::Info,
    };
    TermLogger::init(loglevel, Config::default(), Mixed, Auto)?;

    /* ANCHOR: main */
    let bincode = include_bytes!("arithmetic.zk.bin");
    let zkbin = ZkBinary::decode(bincode)?;

    // ======
    // Prover
    // ======

    // Witness values
    let a = pallas::Base::from(42);
    let b = pallas::Base::from(69);

    let prover_witnesses = vec![Witness::Base(Some(a)), Witness::Base(Some(b))];

    // Create the public inputs
    let sum = a + b;
    let product = a * b;
    let difference = a - b;

    let public_inputs = vec![sum, product, difference];

    // Create the circuit
    let circuit = ZkCircuit::new(prover_witnesses, zkbin.clone());

    info!(target: "PROVER", "Building proving key and creating the zero-knowledge proof");
    let proving_key = ProvingKey::build(11, &circuit);
    let proof = Proof::create(&proving_key, &[circuit], &public_inputs, &mut OsRng)?;

    // ========
    // Verifier
    // ========

    // Construct empty witnesses
    let verifier_witnesses = vec![Witness::Base(None), Witness::Base(None)];

    // Create the circuit
    let circuit = ZkCircuit::new(verifier_witnesses, zkbin);

    info!(target: "VERIFIER", "Building verifying key and verifying the zero-knowledge proof");
    let verifying_key = VerifyingKey::build(11, &circuit);
    proof.verify(&verifying_key, &public_inputs)?;
    /* ANCHOR_END: main */

    Ok(())
}
