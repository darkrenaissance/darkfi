use std::time::Instant;

#[allow(unused_imports)]
use halo2::{
    arithmetic::{CurveAffine, Field},
    dev::MockProver,
};
use halo2_gadgets::primitives::{
    poseidon,
    poseidon::{ConstantLength, P128Pow5T3},
};
use incrementalmerkletree::{bridgetree::BridgeTree, Frontier, Tree};
use log::info;
use pasta_curves::{group::Curve, pallas};
use rand::rngs::OsRng;
use simplelog::{ColorChoice, LevelFilter, TermLogger, TerminalMode};

use darkfi::{
    crypto::{
        keypair::{PublicKey, SecretKey},
        merkle_node::MerkleNode,
        mint_proof::MintRevealedValues,
        proof::{ProvingKey, VerifyingKey},
        spend_proof::SpendRevealedValues,
        Proof,
    },
    zk::vm::{Witness, ZkCircuit},
    zkas::decoder::ZkBinary,
    Result,
};

fn mint_proof() -> Result<()> {
    let bincode = include_bytes!("../proof/mint.zk.bin");
    let zkbin = ZkBinary::decode(bincode)?;

    // ======
    // Prover
    // ======
    let value = 42;
    let token_id = pallas::Base::from(22);
    let value_blind = pallas::Scalar::random(&mut OsRng);
    let token_blind = pallas::Scalar::random(&mut OsRng);
    let serial = pallas::Base::random(&mut OsRng);
    let coin_blind = pallas::Base::random(&mut OsRng);
    let public_key = PublicKey::random(&mut OsRng);
    let pk_coords = public_key.0.to_affine().coordinates().unwrap();

    let witnesses_prover = vec![
        Witness::Base(Some(*pk_coords.x())),
        Witness::Base(Some(*pk_coords.y())),
        Witness::Base(Some(pallas::Base::from(value))),
        Witness::Base(Some(token_id)),
        Witness::Base(Some(serial)),
        Witness::Base(Some(coin_blind)),
        Witness::Scalar(Some(value_blind)),
        Witness::Scalar(Some(token_blind)),
    ];

    let public_inputs = MintRevealedValues::compute(
        value,
        token_id,
        value_blind,
        token_blind,
        serial,
        coin_blind,
        public_key,
    )
    .make_outputs()
    .to_vec();

    let circuit = ZkCircuit::new(witnesses_prover, zkbin.clone());

    // let prover = MockProver::run(11, &circuit, vec![public_inputs.clone()]).unwrap();
    // assert_eq!(prover.verify(), Ok(()));

    let start = Instant::now();
    let proving_key = ProvingKey::build(11, circuit.clone());
    info!("Prover setup: [{:?}]", Instant::now() - start);

    let start = Instant::now();
    let proof = Proof::create(&proving_key, &[circuit], &public_inputs.clone())?;
    info!("Prover prove: [{:?}]", Instant::now() - start);

    // =======
    // Verifier
    // =======

    let witnesses_verifier = vec![
        Witness::Base(None),
        Witness::Base(None),
        Witness::Base(None),
        Witness::Base(None),
        Witness::Base(None),
        Witness::Base(None),
        Witness::Scalar(None),
        Witness::Scalar(None),
    ];

    let start = Instant::now();
    let circuit = ZkCircuit::new(witnesses_verifier, zkbin);
    let verifying_key = VerifyingKey::build(11, circuit);
    info!("Verifier setup: [{:?}]", Instant::now() - start);

    let start = Instant::now();
    proof.verify(&verifying_key, &public_inputs)?;
    info!("Verifier verify: [{:?}]", Instant::now() - start);

    Ok(())
}

fn fill_tree(coin2: pallas::Base) -> BridgeTree<MerkleNode, 32> {
    let mut tree = BridgeTree::<MerkleNode, 32>::new(100);
    let coin0 = pallas::Base::random(&mut OsRng);
    let coin1 = pallas::Base::random(&mut OsRng);
    let coin3 = pallas::Base::random(&mut OsRng);

    tree.append(&MerkleNode(coin0));
    tree.witness();

    tree.append(&MerkleNode(coin1));

    tree.append(&MerkleNode(coin2));
    tree.witness();

    tree.append(&MerkleNode(coin3));
    tree.witness();

    tree
}

fn burn_proof() -> Result<()> {
    let bincode = include_bytes!("../proof/burn.zk.bin");
    let zkbin = ZkBinary::decode(bincode)?;

    // ======
    // Prover
    // ======
    let value = 42;
    let token_id = pallas::Base::from(22);
    let value_blind = pallas::Scalar::random(&mut OsRng);
    let token_blind = pallas::Scalar::random(&mut OsRng);
    let serial = pallas::Base::random(&mut OsRng);
    let coin_blind = pallas::Base::random(&mut OsRng);
    let secret = SecretKey::random(&mut OsRng);
    let sig_secret = SecretKey::random(&mut OsRng);

    let coin = {
        let coords = PublicKey::from_secret(secret).0.to_affine().coordinates().unwrap();
        let messages =
            [*coords.x(), *coords.y(), pallas::Base::from(value), token_id, serial, coin_blind];

        poseidon::Hash::init(P128Pow5T3, ConstantLength::<6>).hash(messages)
    };

    let tree = fill_tree(coin);
    let (leaf_position, merkle_path) = tree.authentication_path(&MerkleNode(coin)).unwrap();

    // Why are these types not matched in halo2 gadgets?
    let leaf_pos: u64 = leaf_position.into();
    let leaf_pos = leaf_pos as u32;

    let witnesses_prover = vec![
        Witness::Base(Some(secret.0)),
        Witness::Base(Some(serial)),
        Witness::Base(Some(pallas::Base::from(value))),
        Witness::Base(Some(token_id)),
        Witness::Base(Some(coin_blind)),
        Witness::Scalar(Some(value_blind)),
        Witness::Scalar(Some(token_blind)),
        Witness::Uint32(Some(leaf_pos)),
        Witness::MerklePath(Some(merkle_path.clone())),
        Witness::Base(Some(sig_secret.0)),
    ];

    let public_inputs = SpendRevealedValues::compute(
        value,
        token_id,
        value_blind,
        token_blind,
        serial,
        coin_blind,
        secret,
        leaf_position,
        merkle_path,
        sig_secret,
    )
    .make_outputs()
    .to_vec();

    let circuit = ZkCircuit::new(witnesses_prover, zkbin.clone());

    // let prover = MockProver::run(11, &circuit, vec![public_inputs.clone()])?;
    // assert_eq!(prover.verify(), Ok(()));

    let start = Instant::now();
    let proving_key = ProvingKey::build(11, circuit.clone());
    info!("Prover setup: [{:?}]", Instant::now() - start);

    let start = Instant::now();
    let proof = Proof::create(&proving_key, &[circuit], &public_inputs)?;
    info!("Prover prove: [{:?}]", Instant::now() - start);

    // ========
    // Verifier
    // ========

    let witnesses_verifier = vec![
        Witness::Base(None),
        Witness::Base(None),
        Witness::Base(None),
        Witness::Base(None),
        Witness::Base(None),
        Witness::Scalar(None),
        Witness::Scalar(None),
        Witness::Uint32(None),
        Witness::MerklePath(None),
        Witness::Base(None),
    ];

    let start = Instant::now();
    let circuit = ZkCircuit::new(witnesses_verifier, zkbin);
    let verifying_key = VerifyingKey::build(11, circuit);
    info!("Verifier setup: [{:?}]", Instant::now() - start);

    let start = Instant::now();
    proof.verify(&verifying_key, &public_inputs)?;
    info!("Verifier verify: [{:?}]", Instant::now() - start);

    Ok(())
}

fn main() -> Result<()> {
    TermLogger::init(
        //LevelFilter::Debug,
        LevelFilter::Info,
        simplelog::Config::default(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    )?;

    info!("Executing Mint proof");
    mint_proof()?;

    info!("Executing Burn proof");
    burn_proof()?;

    Ok(())
}
