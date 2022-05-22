// ../zkas simple.zk

use darkfi::{
    crypto::{
        proof::{ProvingKey, VerifyingKey},
        util::pedersen_commitment_u64,
        Proof,
    },
    zk::{
        vm::{Witness, ZkCircuit},
        vm_stack::empty_witnesses,
    },
    zkas::decoder::ZkBinary,
    Result,
};
use pasta_curves::{
    arithmetic::CurveAffine,
    group::{ff::Field, Curve},
    pallas,
};
use rand::rngs::OsRng;

fn main() -> Result<()> {
    let bincode = include_bytes!("simple.zk.bin");
    let zkbin = ZkBinary::decode(bincode)?;

    // ======
    // Prover
    // ======
    // Bigger k = more rows, but slower circuit
    // Number of rows is 2^k
    let k = 13;

    // Witness values
    let value = 42;
    let value_blind = pallas::Scalar::random(&mut OsRng);

    let prover_witnesses =
        vec![Witness::Base(Some(pallas::Base::from(value))), Witness::Scalar(Some(value_blind))];

    // Create the public inputs
    let value_commit = pedersen_commitment_u64(value, value_blind);
    let value_coords = value_commit.to_affine().coordinates().unwrap();

    let public_inputs = vec![*value_coords.x(), *value_coords.y()];

    // Create the circuit
    let circuit = ZkCircuit::new(prover_witnesses, zkbin.clone());

    let now = std::time::Instant::now();
    let proving_key = ProvingKey::build(k, &circuit);
    println!("ProvingKey built [{} s]", now.elapsed().as_secs_f64());
    let now = std::time::Instant::now();
    let proof = Proof::create(&proving_key, &[circuit], &public_inputs, &mut OsRng)?;
    println!("Proof created [{} s]", now.elapsed().as_secs_f64());

    // ========
    // Verifier
    // ========

    // Construct empty witnesses
    let verifier_witnesses = empty_witnesses(&zkbin);

    // Create the circuit
    let circuit = ZkCircuit::new(verifier_witnesses, zkbin);

    let now = std::time::Instant::now();
    let verifying_key = VerifyingKey::build(k, &circuit);
    println!("VerifyingKey built [{} s]", now.elapsed().as_secs_f64());
    let now = std::time::Instant::now();
    proof.verify(&verifying_key, &public_inputs)?;
    println!("proof verify [{} s]", now.elapsed().as_secs_f64());

    Ok(())
}
