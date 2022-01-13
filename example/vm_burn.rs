use std::iter;

use halo2::dev::MockProver;
use halo2_gadgets::{
    ecc::FixedPoints,
    primitives,
    primitives::{
        poseidon::{ConstantLength, P128Pow5T3},
        sinsemilla::S_PERSONALIZATION,
    },
};
use pasta_curves::{
    arithmetic::{CurveAffine, Field},
    group::{ff::PrimeFieldBits, Curve},
    pallas,
};
use rand::rngs::OsRng;
use std::{collections::HashMap, fs::File, time::Instant};

use darkfi::{
    crypto::{
        constants::{
            sinsemilla::{i2lebsp, MERKLE_CRH_PERSONALIZATION},
            OrchardFixedBases,
        },
        proof::{Proof, ProvingKey, VerifyingKey},
        util::pedersen_commitment_u64,
    },
    util::serial::Decodable,
    zk::vm,
    Error,
};

fn root(path: [pallas::Base; 32], leaf_pos: u32, leaf: pallas::Base) -> pallas::Base {
    let domain = primitives::sinsemilla::HashDomain::new(MERKLE_CRH_PERSONALIZATION);

    let pos_bool = i2lebsp::<32>(leaf_pos as u64);

    let mut node = leaf;
    for (l, (sibling, pos)) in path.iter().zip(pos_bool.iter()).enumerate() {
        let (left, right) = if *pos { (*sibling, node) } else { (node, *sibling) };

        let l_star = i2lebsp::<10>(l as u64);
        let left: Vec<_> = left.to_le_bits().iter().by_val().take(255).collect();
        let right: Vec<_> = right.to_le_bits().iter().by_val().take(255).collect();

        let mut message = l_star.to_vec();
        message.extend_from_slice(&left);
        message.extend_from_slice(&right);

        node = domain.hash(message.into_iter()).unwrap();
    }
    node
}

fn main() -> std::result::Result<(), Error> {
    // The number of rows in our circuit cannot exceed 2^k
    let k: u32 = 11;

    let start = Instant::now();
    let file = File::open("../proof/burn.zk.bin")?;
    let zkbin = vm::ZkBinary::decode(file)?;
    for contract_name in zkbin.contracts.keys() {
        println!("Loaded '{}' contract.", contract_name);
    }
    println!("Load time: [{:?}]", start.elapsed());

    let contract = &zkbin.contracts["Burn"];

    //contract.witness_base(...);
    //contract.witness_base(...);
    //contract.witness_base(...);

    let secret = pallas::Scalar::random(&mut OsRng);
    let serial = pallas::Base::random(&mut OsRng);

    let value = 110;
    let asset = 1;

    // Nullifier = poseidon(sinsemilla(secret_key), serial)
    let domain = primitives::sinsemilla::HashDomain::new(S_PERSONALIZATION);
    let bits_secretkey: Vec<bool> = secret.to_le_bits().iter().by_val().collect();
    let hashed_secret_key = domain.hash(iter::empty().chain(bits_secretkey)).unwrap();

    let nullifier = [hashed_secret_key, serial];
    let nullifier =
        primitives::poseidon::Hash::init(P128Pow5T3, ConstantLength::<2>).hash(nullifier);

    // Public key derivation
    let public_key = OrchardFixedBases::SpendAuthG.generator() * secret;
    let coords = public_key.to_affine().coordinates().unwrap();

    // Construct Coin
    let mut coin = pallas::Base::zero();
    let coin_blind = pallas::Base::random(&mut OsRng);
    let messages = [
        [*coords.x(), *coords.y()],
        [pallas::Base::from(value), pallas::Base::from(asset)],
        [serial, coin_blind],
    ];

    for msg in messages.iter() {
        let hash = primitives::poseidon::Hash::init(P128Pow5T3, ConstantLength::<2>).hash(*msg);
        coin += hash;
    }

    // Merkle root
    let leaf = pallas::Base::random(&mut OsRng);
    let leaf_pos = rand::random::<u32>();
    let path: Vec<_> = (0..32).map(|_| pallas::Base::random(&mut OsRng)).collect();
    let merkle_root = root(path.clone().try_into().unwrap(), leaf_pos, leaf);

    // Value and asset commitments
    let value_blind = pallas::Scalar::random(&mut OsRng);
    let asset_blind = pallas::Scalar::random(&mut OsRng);
    let value_commit = pedersen_commitment_u64(value, value_blind);
    let asset_commit = pedersen_commitment_u64(asset, asset_blind);

    let value_coords = value_commit.to_affine().coordinates().unwrap();
    let asset_coords = asset_commit.to_affine().coordinates().unwrap();

    // Derive signature public key from signature secret key
    let sig_secret = pallas::Scalar::random(&mut OsRng);
    let sig_pubkey = OrchardFixedBases::SpendAuthG.generator() * sig_secret;
    let sig_coords = sig_pubkey.to_affine().coordinates().unwrap();

    let public_inputs = vec![
        nullifier,
        merkle_root,
        *value_coords.x(),
        *value_coords.y(),
        *asset_coords.x(),
        *asset_coords.y(),
        *sig_coords.x(),
        *sig_coords.y(),
    ];

    //

    let mut const_fixed_points = HashMap::new();
    const_fixed_points.insert("VALUE_COMMIT_VALUE".to_string(), OrchardFixedBases::ValueCommitV);
    const_fixed_points.insert("VALUE_COMMIT_RANDOM".to_string(), OrchardFixedBases::ValueCommitR);
    const_fixed_points.insert("SPEND_AUTH_G".to_string(), OrchardFixedBases::SpendAuthG);

    let mut circuit = vm::ZkCircuit::new(const_fixed_points, &zkbin.constants, contract);
    let empty_circuit = circuit.clone();

    circuit.witness_base("secret", hashed_secret_key)?;
    circuit.witness_base("serial", serial)?;
    circuit.witness_merkle_path("path", leaf_pos, path.try_into().unwrap())?;
    circuit.witness_base("leaf", leaf)?;
    circuit.witness_base("value", pallas::Base::from(value))?;
    circuit.witness_base("asset", pallas::Base::from(asset))?;
    circuit.witness_scalar("value_blind", value_blind)?;
    circuit.witness_scalar("asset_blind", asset_blind)?;
    circuit.witness_scalar("sig_secret", sig_secret)?;

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
