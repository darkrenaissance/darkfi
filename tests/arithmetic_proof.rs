use darkfi::{
    crypto::{
        proof::{ProvingKey, VerifyingKey},
        Proof,
    },
    zk::{
        vm::{Witness, ZkCircuit},
        vm_stack::empty_witnesses,
    },
    zkas::decoder::ZkBinary,
    Result,
};
use pasta_curves::pallas;
use rand::rngs::OsRng;

#[test]
fn arithmetic_proof() -> Result<()> {
    /* ANCHOR: main */
    let bincode = include_bytes!("../proof/arithmetic.zk.bin");
    let zkbin = ZkBinary::decode(bincode)?;

    // ======
    // Prover
    // ======

    // Witness values
    let a = pallas::Base::from(42);
    let b = pallas::Base::from(69);
    let y_0 = pallas::Base::from(0); // Here we will compare a > b, which is false (0)
    let y_1 = pallas::Base::from(1); // Here we will compare b > a, which is true (1)

    let prover_witnesses = vec![Witness::Base(Some(a)), Witness::Base(Some(b))];

    // Create the public inputs
    let sum = a + b;
    let product = a * b;
    let difference = a - b;

    let public_inputs = vec![sum, product, difference, y_0, y_1];

    // Create the circuit
    let circuit = ZkCircuit::new(prover_witnesses, zkbin.clone());

    let proving_key = ProvingKey::build(13, &circuit);
    let proof = Proof::create(&proving_key, &[circuit], &public_inputs, &mut OsRng)?;

    // ========
    // Verifier
    // ========

    // Construct empty witnesses
    let verifier_witnesses = empty_witnesses(&zkbin);

    // Create the circuit
    let circuit = ZkCircuit::new(verifier_witnesses, zkbin);

    let verifying_key = VerifyingKey::build(13, &circuit);
    proof.verify(&verifying_key, &public_inputs)?;
    /* ANCHOR_END: main */

    Ok(())
}
