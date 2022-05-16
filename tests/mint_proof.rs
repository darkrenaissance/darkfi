use darkfi::{
    crypto::{
        keypair::PublicKey,
        proof::{ProvingKey, VerifyingKey},
        util::{mod_r_p, pedersen_commitment_scalar, pedersen_commitment_u64},
        Proof,
    },
    zk::{
        vm::{Witness, ZkCircuit},
        vm_stack::empty_witnesses,
    },
    zkas::decoder::ZkBinary,
    Result,
};
use halo2_gadgets::poseidon::primitives as poseidon;
use pasta_curves::{
    arithmetic::CurveAffine,
    group::{ff::Field, Curve},
    pallas,
};
use rand::rngs::OsRng;

#[test]
fn mint_proof() -> Result<()> {
    /* ANCHOR: main */
    let bincode = include_bytes!("../proof/mint.zk.bin");
    let zkbin = ZkBinary::decode(bincode)?;

    // ======
    // Prover
    // ======

    // Witness values
    let value = 42;
    let token_id = pallas::Base::from(22);
    let value_blind = pallas::Scalar::random(&mut OsRng);
    let token_blind = pallas::Scalar::random(&mut OsRng);
    let serial = pallas::Base::random(&mut OsRng);
    let coin_blind = pallas::Base::random(&mut OsRng);
    let public_key = PublicKey::random(&mut OsRng);
    let coords = public_key.0.to_affine().coordinates().unwrap();

    let prover_witnesses = vec![
        Witness::Base(Some(*coords.x())),
        Witness::Base(Some(*coords.y())),
        Witness::Base(Some(pallas::Base::from(value))),
        Witness::Base(Some(token_id)),
        Witness::Base(Some(serial)),
        Witness::Base(Some(coin_blind)),
        Witness::Scalar(Some(value_blind)),
        Witness::Scalar(Some(token_blind)),
    ];

    // Create the public inputs
    let msgs = [*coords.x(), *coords.y(), pallas::Base::from(value), token_id, serial, coin_blind];
    let coin = poseidon::Hash::<_, poseidon::P128Pow5T3, poseidon::ConstantLength<6>, 3, 2>::init()
        .hash(msgs);

    let value_commit = pedersen_commitment_u64(value, value_blind);
    let value_coords = value_commit.to_affine().coordinates().unwrap();

    let token_commit = pedersen_commitment_scalar(mod_r_p(token_id), token_blind);
    let token_coords = token_commit.to_affine().coordinates().unwrap();

    let public_inputs =
        vec![coin, *value_coords.x(), *value_coords.y(), *token_coords.x(), *token_coords.y()];

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
