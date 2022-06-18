use darkfi::{
    crypto::{
        keypair::{PublicKey, SecretKey},
        merkle_node::MerkleNode,
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
use halo2_proofs::circuit::Value;
use incrementalmerkletree::{bridgetree::BridgeTree, Tree};
use pasta_curves::{
    arithmetic::CurveAffine,
    group::{ff::Field, Curve},
    pallas,
};
use rand::rngs::OsRng;

#[test]
fn burn_proof() -> Result<()> {
    /* ANCHOR: main */
    let bincode = include_bytes!("../proof/burn.zk.bin");
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

        poseidon::Hash::<_, poseidon::P128Pow5T3, poseidon::ConstantLength<6>, 3, 2>::init()
            .hash(messages)
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
    let leaf_pos = tree.witness().unwrap();
    tree.append(&MerkleNode(coin3));
    tree.witness();

    let root = tree.root(0).unwrap();
    let merkle_path = tree.authentication_path(leaf_pos, &root).unwrap();
    let leaf_pos: u64 = leaf_pos.into();

    let prover_witnesses = vec![
        Witness::Base(Value::known(secret.0)),
        Witness::Base(Value::known(serial)),
        Witness::Base(Value::known(pallas::Base::from(value))),
        Witness::Base(Value::known(token_id)),
        Witness::Base(Value::known(coin_blind)),
        Witness::Scalar(Value::known(value_blind)),
        Witness::Scalar(Value::known(token_blind)),
        Witness::Uint32(Value::known(leaf_pos.try_into().unwrap())),
        Witness::MerklePath(Value::known(merkle_path.try_into().unwrap())),
        Witness::Base(Value::known(sig_secret.0)),
    ];

    // Create the public inputs
    let nullifier = [secret.0, serial];
    let nullifier =
        poseidon::Hash::<_, poseidon::P128Pow5T3, poseidon::ConstantLength<2>, 3, 2>::init()
            .hash(nullifier);

    let value_commit = pedersen_commitment_u64(value, value_blind);
    let value_coords = value_commit.to_affine().coordinates().unwrap();

    let token_commit = pedersen_commitment_scalar(mod_r_p(token_id), token_blind);
    let token_coords = token_commit.to_affine().coordinates().unwrap();

    let sig_pubkey = PublicKey::from_secret(sig_secret);
    let sig_coords = sig_pubkey.0.to_affine().coordinates().unwrap();

    let merkle_root = tree.root(0).unwrap();

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
