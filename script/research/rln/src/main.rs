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

use darkfi_sdk::{
    crypto::{pasta_prelude::*, poseidon_hash, MerkleNode, MerkleTree, SecretKey},
    incrementalmerkletree::Tree,
    pasta::pallas,
};
use rand::rngs::OsRng;

const IDENTITY_COMMITMENT_PREFIX: u64 = 42;

#[derive(Copy, Clone, Debug)]
struct ShamirPoint {
    pub x: pallas::Base,
    pub y: pallas::Base,
}

fn sss_share(secret: pallas::Base, n_shares: usize, threshold: usize) -> Vec<ShamirPoint> {
    assert!(threshold > 2 && n_shares > threshold);

    let mut coefficients = vec![secret];
    for _ in 0..threshold - 1 {
        coefficients.push(pallas::Base::random(&mut OsRng));
    }

    let mut shares = Vec::with_capacity(n_shares);

    for x in 1..n_shares + 1 {
        let x = pallas::Base::from(x as u64);
        let mut y = pallas::Base::zero();
        for coeff in coefficients.iter().rev() {
            y *= x;
            y += coeff;
        }

        shares.push(ShamirPoint { x, y })
    }

    shares
}

fn sss_recover(shares: &[ShamirPoint]) -> pallas::Base {
    assert!(shares.len() > 1);

    let mut secret = pallas::Base::zero();

    for (j, share_j) in shares.iter().enumerate() {
        let mut prod = pallas::Base::one();

        for (i, share_i) in shares.iter().enumerate() {
            if i != j {
                prod *= share_i.x * (share_i.x - share_j.x).invert().unwrap();
            }
        }

        prod *= share_j.y;
        secret += prod;
    }

    secret
}

fn main() {
    let mut membership_tree = MerkleTree::new(100);

    // The identity commitment should be something that cannot be precalculated
    // for usage in the future, and possibly also has to be some kind of puzzle
    // that is costly to precalculate.
    // Alternatively, it could be economic stake of funds which could then be
    // lost if spam is detected and acted upon.
    let alice_secret_key = SecretKey::random(&mut OsRng);
    let alice_identity_commitment =
        poseidon_hash([pallas::Base::from(IDENTITY_COMMITMENT_PREFIX), alice_secret_key.inner()]);

    // The user registers, and their identity commitment gets added into the
    // membership tree:
    membership_tree.append(&MerkleNode::from(alice_identity_commitment));
    let alice_identity_leaf_pos = membership_tree.witness().unwrap();
}
