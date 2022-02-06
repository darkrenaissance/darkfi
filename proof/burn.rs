use darkfi::{
    crypto::{
        keypair::{PublicKey, SecretKey},
        merkle_node::MerkleNode,
        proof::{ProvingKey, VerifyingKey},
        util::{mod_r_p, pedersen_commitment_scalar, pedersen_commitment_u64},
        Proof,
    },
    zk::vm::{Witness, ZkCircuit},
    zkas::decoder::ZkBinary,
    Result,
};
use halo2_gadgets::primitives::{
    poseidon,
    poseidon::{ConstantLength, P128Pow5T3},
};
use incrementalmerkletree::{bridgetree::BridgeTree, Frontier, Tree};
use log::info;
use pasta_curves::{
    arithmetic::{CurveAffine, Field},
    group::Curve,
    pallas,
};
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
    let bincode = include_bytes!("burn.zk.bin");
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
    let secret = SecretKey::random(&mut OsRng);
    let sig_secret = SecretKey::random(&mut OsRng);

    // Build the coin
    let coin2 = {
        let coords = PublicKey::from_secret(secret).0.to_affine().coordinates().unwrap();
        let messages =
            [*coords.x(), *coords.y(), pallas::Base::from(value), token_id, serial, coin_blind];

        poseidon::Hash::init(P128Pow5T3, ConstantLength::<6>).hash(messages)
    };

    // Fill the merkle tree with some random coins that we want to witness,
    // and also add the above coin.
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

    let (leaf_pos, merkle_path) = tree.authentication_path(&MerkleNode(coin2)).unwrap();
    let leaf_pos: u64 = leaf_pos.into();
    let leaf_pos = leaf_pos as u32;

    let prover_witnesses = vec![
        Witness::Base(Some(secret.0)),
        Witness::Base(Some(serial)),
        Witness::Base(Some(pallas::Base::from(value))),
        Witness::Base(Some(token_id)),
        Witness::Base(Some(coin_blind)),
        Witness::Scalar(Some(value_blind)),
        Witness::Scalar(Some(token_blind)),
        Witness::Uint32(Some(leaf_pos)),
        Witness::MerklePath(Some(merkle_path.try_into().unwrap())),
        Witness::Base(Some(sig_secret.0)),
    ];

    // Create the public inputs
    let nullifier = [secret.0, serial];
    let nullifier = poseidon::Hash::init(P128Pow5T3, ConstantLength::<2>).hash(nullifier);

    let value_commit = pedersen_commitment_u64(value, value_blind);
    let value_coords = value_commit.to_affine().coordinates().unwrap();

    let token_commit = pedersen_commitment_scalar(mod_r_p(token_id), token_blind);
    let token_coords = token_commit.to_affine().coordinates().unwrap();

    let sig_pubkey = PublicKey::from_secret(sig_secret);
    let sig_coords = sig_pubkey.0.to_affine().coordinates().unwrap();

    let merkle_root = tree.root();

    let public_inputs = vec![
        nullifier,
        *value_coords.x(),
        *value_coords.y(),
        *token_coords.x(),
        *token_coords.y(),
        merkle_root.0,
        *sig_coords.x(),
        *sig_coords.y(),
    ];

    // Create the circuit
    let circuit = ZkCircuit::new(prover_witnesses, zkbin.clone());

    info!(target: "PROVER", "Building proving key and creating the zero-knowledge proof");
    let proving_key = ProvingKey::build(11, &circuit);
    let proof = Proof::create(&proving_key, &[circuit], &public_inputs)?;

    // ========
    // Verifier
    // ========

    // Construct empty witnesses
    let verifier_witnesses = vec![
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

    // Create the circuit
    let circuit = ZkCircuit::new(verifier_witnesses, zkbin);

    info!(target: "VERIFIER", "Building verifying key and verifying the zero-knowledge proof");
    let verifying_key = VerifyingKey::build(11, &circuit);
    proof.verify(&verifying_key, &public_inputs)?;
    /* ANCHOR_END: main */

    Ok(())
}
