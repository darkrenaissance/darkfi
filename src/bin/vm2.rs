use halo2::dev::MockProver;
use halo2_gadgets::{
    primitives,
    primitives::poseidon::{ConstantLength, P128Pow5T3},
};
use pasta_curves::{
    arithmetic::{CurveAffine, Field},
    group::{Curve, Group},
    pallas,
};
use rand::rngs::OsRng;
use std::{collections::HashMap, fs::File, time::Instant};

use drk::{
    crypto::{
        constants::OrchardFixedBases,
        proof::{Proof, ProvingKey, VerifyingKey},
        util::pedersen_commitment_u64,
    },
    serial::Decodable,
    vm,
};

fn main() -> std::result::Result<(), failure::Error> {
    // The number of rows in our circuit cannot exceed 2^k
    let k: u32 = 11;

    let start = Instant::now();
    let file = File::open("proof/mint.zk.bin")?;
    let zkbin = vm::ZkBinary::decode(file)?;
    for contract_name in zkbin.contracts.keys() {
        println!("Loaded '{}' contract.", contract_name);
    }
    println!("Load time: [{:?}]", start.elapsed());

    let contract = &zkbin.contracts["Mint"];

    //contract.witness_base(...);
    //contract.witness_base(...);
    //contract.witness_base(...);

    let pubkey = pallas::Point::random(&mut OsRng);
    let coords = pubkey.to_affine().coordinates().unwrap();

    let value = 110;
    let asset = 1;

    let value_blind = pallas::Scalar::random(&mut OsRng);
    let asset_blind = pallas::Scalar::random(&mut OsRng);

    let serial = pallas::Base::random(&mut OsRng);
    let coin_blind = pallas::Base::random(&mut OsRng);

    let mut coin = pallas::Base::zero();

    let messages = [
        [*coords.x(), *coords.y()],
        [pallas::Base::from(value), pallas::Base::from(asset)],
        [serial, coin_blind],
    ];

    for msg in messages.iter() {
        coin += primitives::poseidon::Hash::init(P128Pow5T3, ConstantLength::<2>).hash(*msg);
    }

    let _coin2 = primitives::poseidon::Hash::init(P128Pow5T3, ConstantLength::<2>)
        .hash([*coords.x(), *coords.y()]);

    let value_commit = pedersen_commitment_u64(value, value_blind);
    let value_coords = value_commit.to_affine().coordinates().unwrap();

    let asset_commit = pedersen_commitment_u64(asset, asset_blind);
    let asset_coords = asset_commit.to_affine().coordinates().unwrap();

    let public_inputs =
        vec![coin, *value_coords.x(), *value_coords.y(), *asset_coords.x(), *asset_coords.y()];

    let mut const_fixed_points = HashMap::new();
    const_fixed_points.insert("VALUE_COMMIT_VALUE".to_string(), OrchardFixedBases::ValueCommitV);
    const_fixed_points.insert("VALUE_COMMIT_RANDOM".to_string(), OrchardFixedBases::ValueCommitR);

    let mut circuit = vm::ZkCircuit::new(const_fixed_points, &zkbin.constants, contract);
    let empty_circuit = circuit.clone();

    circuit.witness_base("pub_x", *coords.x())?;
    circuit.witness_base("pub_y", *coords.y())?;
    circuit.witness_base("value", pallas::Base::from(value))?;
    circuit.witness_base("asset", pallas::Base::from(asset))?;
    circuit.witness_base("serial", serial)?;
    circuit.witness_base("coin_blind", coin_blind)?;
    circuit.witness_scalar("value_blind", value_blind)?;
    circuit.witness_scalar("asset_blind", asset_blind)?;

    // Valid MockProver
    let prover = MockProver::run(k, &circuit, vec![public_inputs.clone()]).unwrap();
    assert_eq!(prover.verify(), Ok(()));

    // Actual ZK proof
    let start = Instant::now();
    let vk = VerifyingKey::build(k, empty_circuit.clone());
    let pk = ProvingKey::build(k, empty_circuit.clone());
    println!("\nSetup: [{:?}]", start.elapsed());

    let start = Instant::now();
    let proof = Proof::create(&pk, &[circuit], &public_inputs).unwrap();
    println!("Prove: [{:?}]", start.elapsed());

    let start = Instant::now();
    assert!(proof.verify(&vk, &public_inputs).is_ok());
    println!("Verify: [{:?}]", start.elapsed());

    Ok(())
}
