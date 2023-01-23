//! Rate-limit nullifiers, to be implemented in ircd for spam protection.
//!
//! For an application, each user maintains:
//! - User registration
//! - User interactions
//! - User removal
//!
//! # User registration
//! 1. Derive an identity commitment: poseidon_hash(secret_key)
//! 2. Register by providing the commitment
//! 3. Store the commitment in the Merkle tree of registered users
//!
//! # User interaction
//! For each interaction, the user must create a ZK proof which ensures
//! the other participants (verifiers) that they are a valid member of
//! the application and their identity commitment is part of the membership
//! Merkle tree.
//! The anti-spam rule is also introduced in the protocol, e.g.:
//!
//! > Users must not make more than X interactions per epoch.
//! In other words:
//! > Users must not send more than one message per second.
//!
//! The anti-spam rule is implemented with Shamir-Secret-Sharing Scheme.
//! In our case the secret is the user's secret key, and the shares are
//! parts of the secret key. In a 2/3 case, this means the user's secret
//! key can be reconstructed if they send two messages per epoch.
//! For these claims to hold true, the user's ZK proof must also include
//! shares of their secret key and the epoch. By not having any of these
//! fields included, the ZK proof will be treated as invalid.
//!
//! # User removal
//! In the case of spam, the secret key can be retrieved from the SSS
//! shares and a user can use this to remove the key from the set of
//! registered users, therefore disabling their ability to send future
//! messages and requiring them to register with a new key.

use darkfi::{
    zk::{
        empty_witnesses, halo2::Value, proof::VerifyingKey, Proof, ProvingKey, Witness, ZkCircuit,
    },
    zkas::ZkBinary,
};
use darkfi_sdk::{
    crypto::{pasta_prelude::*, poseidon_hash, MerkleNode, MerkleTree},
    incrementalmerkletree::Tree,
    pasta::{arithmetic::CurveExt, pallas},
};
use rand::rngs::OsRng;

fn main() {
    // This should be unique constant per application
    let rln_identifier = pallas::Base::from(42);
    let key_derivation_path = pallas::Base::from(0);
    let nf_derivation_path = pallas::Base::from(1);

    let epoch = pallas::Base::from(1674495551);
    let external_nullifier = poseidon_hash([epoch, rln_identifier]);

    // The identity commitment should be something that cannot be precalculated
    // for usage in the future, and possibly also has to be some kind of puzzle
    // that is costly to precalculate.
    // Alternatively, it could be economic stake of funds which could then be
    // lost if spam is detected and acted upon.
    let alice_secret_key = pallas::Base::random(&mut OsRng);
    let alice_identity_commitment = poseidon_hash([key_derivation_path, alice_secret_key]);

    // ============
    // Registration
    // ============
    let mut membership_tree = MerkleTree::new(100);
    membership_tree.append(&MerkleNode::from(alice_identity_commitment));
    let alice_identity_leafpos = membership_tree.witness().unwrap();

    // ==========
    // Signalling
    // ==========

    // Our secret-sharing polynomial will be A(x) = a_1*x + a_0, where:
    // a_0 = secret_key,
    // a_1 = Poseidon(a_0, external_nullifier)
    // To send a message, the user has to come up with a share - an (x, y) on the polynomial.
    // x = Poseidon(message), y = A(x)
    // Thus, if the same epoch user sends more than one message, their secret can be recovered.

    // TODO: I don't know a better way to do this:
    let message = b"hello i wanna spam";
    let hasher = pallas::Point::hash_to_curve("ircd_domain");
    let message_point = hasher(message);
    let message_coords = message_point.to_affine().coordinates().unwrap();
    let x = poseidon_hash([*message_coords.x(), *message_coords.y()]);
    let y = poseidon_hash([alice_secret_key, external_nullifier]) * x + alice_secret_key;

    let internal_nullifier =
        poseidon_hash([nf_derivation_path, poseidon_hash([alice_secret_key, external_nullifier])]);

    let identity_root = membership_tree.root(0).unwrap();
    let alice_identity_path =
        membership_tree.authentication_path(alice_identity_leafpos, &identity_root).unwrap();

    // NIZK stuff
    let zkbin = include_bytes!("../signal.zk.bin");
    let rln_zkbin = ZkBinary::decode(zkbin).unwrap();
    let rln_empty_circuit = ZkCircuit::new(empty_witnesses(&rln_zkbin), rln_zkbin.clone());

    println!("Building Proving key...");
    let rln_pk = ProvingKey::build(13, &rln_empty_circuit);
    println!("Building Verifying key...");
    let rln_vk = VerifyingKey::build(13, &rln_empty_circuit);

    // Alice has her witnesses and creates a NIZK proof
    let prover_witnesses = vec![
        Witness::Base(Value::known(alice_secret_key)),
        Witness::MerklePath(Value::known(alice_identity_path.clone().try_into().unwrap())),
        Witness::Uint32(Value::known(u64::from(alice_identity_leafpos).try_into().unwrap())),
        Witness::Base(Value::known(x)),
        Witness::Base(Value::known(epoch)),
        Witness::Base(Value::known(rln_identifier)),
    ];

    let rln_circuit = ZkCircuit::new(prover_witnesses, rln_zkbin.clone());
    let public_inputs = vec![
        epoch,
        rln_identifier,
        x, // <-- message hash
        identity_root.inner(),
        internal_nullifier,
        y,
    ];

    println!("Creating ZK proof...");
    let proof = Proof::create(&rln_pk, &[rln_circuit], &public_inputs, &mut OsRng).unwrap();

    // ============
    // Verification
    // ============
    println!("Verifying ZK proof...");
    assert!(proof.verify(&rln_vk, &public_inputs).is_ok());

    let mut alice_shares = vec![(public_inputs[2], public_inputs[5])];

    // Now if Alice sends another message in the same epoch, we should be able to
    // get their secret key and ban them
    let message = b"hello i'm spamming";
    let hasher = pallas::Point::hash_to_curve("ircd_domain");
    let message_point = hasher(message);
    let message_coords = message_point.to_affine().coordinates().unwrap();
    let x = poseidon_hash([*message_coords.x(), *message_coords.y()]);
    let y = poseidon_hash([alice_secret_key, external_nullifier]) * x + alice_secret_key;

    // Same epoch and account, different message
    let prover_witnesses = vec![
        Witness::Base(Value::known(alice_secret_key)),
        Witness::MerklePath(Value::known(alice_identity_path.try_into().unwrap())),
        Witness::Uint32(Value::known(u64::from(alice_identity_leafpos).try_into().unwrap())),
        Witness::Base(Value::known(x)),
        Witness::Base(Value::known(epoch)),
        Witness::Base(Value::known(rln_identifier)),
    ];

    let rln_circuit = ZkCircuit::new(prover_witnesses, rln_zkbin);

    let public_inputs = vec![
        epoch,
        rln_identifier,
        x, // <-- message hash
        identity_root.inner(),
        internal_nullifier,
        y,
    ];

    println!("Creating ZK proof...");
    let proof = Proof::create(&rln_pk, &[rln_circuit], &public_inputs, &mut OsRng).unwrap();

    println!("Verifying ZK proof...");
    assert!(proof.verify(&rln_vk, &public_inputs).is_ok());
    alice_shares.push((public_inputs[2], public_inputs[5]));

    // ========
    // Slashing
    // ========
    // We should be able to retrieve Alice's secret key because she sent two
    // messages in the same epoch.
    let mut secret = pallas::Base::zero();
    for (j, share_j) in alice_shares.iter().enumerate() {
        let mut prod = pallas::Base::one();
        for (i, share_i) in alice_shares.iter().enumerate() {
            if i != j {
                prod *= share_i.0 * (share_i.0 - share_j.0).invert().unwrap();
            }
        }

        prod *= share_j.1;
        secret += prod;
    }

    assert_eq!(secret, alice_secret_key);
    println!("u banned");
}
