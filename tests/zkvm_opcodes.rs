use darkfi_sdk::crypto::MerkleNode;
use halo2_gadgets::poseidon::{
    primitives as poseidon,
    primitives::{ConstantLength, P128Pow5T3},
};
use halo2_proofs::{
    arithmetic::{CurveAffine, Field},
    circuit::Value,
    pasta::{group::Curve, pallas},
};
use incrementalmerkletree::{bridgetree::BridgeTree, Tree};
use rand::rngs::OsRng;
use simplelog::{ColorChoice, Config, LevelFilter, TermLogger, TerminalMode};

use darkfi::{
    crypto::{
        keypair::{PublicKey, SecretKey},
        proof::{ProvingKey, VerifyingKey},
        util::pedersen_commitment_u64,
        Proof,
    },
    zk::{
        vm::ZkCircuit,
        vm_stack::{empty_witnesses, Witness},
    },
    zkas::ZkBinary,
    Result,
};

#[test]
fn zkvm_opcodes() -> Result<()> {
    TermLogger::init(LevelFilter::Debug, Config::default(), TerminalMode::Mixed, ColorChoice::Auto)
        .unwrap();

    let bincode = include_bytes!("../proof/opcodes.zk.bin");
    let zkbin = ZkBinary::decode(bincode)?;

    // Values for the proof
    let value = 666_u64;
    let value_blind = pallas::Scalar::random(&mut OsRng);
    let blind = pallas::Base::random(&mut OsRng);
    let secret = pallas::Base::random(&mut OsRng);
    let a = pallas::Base::from(42);
    let b = pallas::Base::from(69);

    let mut tree = BridgeTree::<MerkleNode, 32>::new(100);
    let c0 = pallas::Base::random(&mut OsRng);
    let c1 = pallas::Base::random(&mut OsRng);
    let c3 = pallas::Base::random(&mut OsRng);
    let c2 = {
        let messages = [pallas::Base::one(), blind];
        poseidon::Hash::<_, P128Pow5T3, ConstantLength<2>, 3, 2>::init().hash(messages)
    };

    tree.append(&MerkleNode::from(c0));
    tree.witness();
    tree.append(&MerkleNode::from(c1));
    tree.append(&MerkleNode::from(c2));
    let leaf_pos = tree.witness().unwrap();
    tree.append(&MerkleNode::from(c3));
    tree.witness();

    let root = tree.root(0).unwrap();
    let merkle_path = tree.authentication_path(leaf_pos, &root).unwrap();
    let leaf_pos: u64 = leaf_pos.into();

    let prover_witnesses = vec![
        Witness::Base(Value::known(pallas::Base::from(value))),
        Witness::Scalar(Value::known(value_blind)),
        Witness::Base(Value::known(blind)),
        Witness::Base(Value::known(a)),
        Witness::Base(Value::known(b)),
        Witness::Base(Value::known(secret)),
        Witness::Uint32(Value::known(leaf_pos.try_into().unwrap())),
        Witness::MerklePath(Value::known(merkle_path.try_into().unwrap())),
    ];

    let value_commit = pedersen_commitment_u64(value, value_blind);
    let value_coords = value_commit.to_affine().coordinates().unwrap();

    let d_m = [pallas::Base::one(), blind, *value_coords.x(), *value_coords.y()];
    let d = poseidon::Hash::<_, P128Pow5T3, ConstantLength<4>, 3, 2>::init().hash(d_m);

    let public = PublicKey::from_secret(SecretKey::from(secret));
    let public_coords = public.0.to_affine().coordinates().unwrap();

    let public_inputs = vec![
        *value_coords.x(),
        *value_coords.y(),
        c2,
        d,
        root.inner(),
        *public_coords.x(),
        *public_coords.y(),
    ];

    let circuit = ZkCircuit::new(prover_witnesses, zkbin.clone());
    let proving_key = ProvingKey::build(13, &circuit);
    let proof = Proof::create(&proving_key, &[circuit], &public_inputs, &mut OsRng)?;

    let verifier_witnesses = empty_witnesses(&zkbin);
    let circuit = ZkCircuit::new(verifier_witnesses, zkbin);
    let verifying_key = VerifyingKey::build(13, &circuit);
    proof.verify(&verifying_key, &public_inputs)?;

    Ok(())
}
