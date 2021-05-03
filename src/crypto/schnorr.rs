use ff::Field;
use group::{Group, GroupEncoding};
use rand::rngs::OsRng;

use super::util::hash_to_scalar;

#[test]
fn test_schnorr() {
    let secret = jubjub::Fr::random(&mut OsRng);
    let public = zcash_primitives::constants::SPENDING_KEY_GENERATOR * secret;

    let mask = jubjub::Fr::random(&mut OsRng);
    let commit = zcash_primitives::constants::SPENDING_KEY_GENERATOR * mask;

    let msg = b"Foo bar";
    let challenge = hash_to_scalar(b"DarkFi_Schnorr", &commit.to_bytes(), &msg[..]);

    let response = mask + challenge * secret;

    // Verify signature

    assert_eq!(
        zcash_primitives::constants::SPENDING_KEY_GENERATOR * response - public * challenge, commit);
}

