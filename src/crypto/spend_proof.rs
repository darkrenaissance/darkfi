use bellman::gadgets::multipack;
use bellman::groth16;
use bitvec::{order::Lsb0, view::AsBits};
use blake2s_simd::Params as Blake2sParams;
use bls12_381::Bls12;
use ff::{Field, PrimeField};
use group::{Curve, GroupEncoding};
use rand::rngs::OsRng;
use std::io;
use std::time::Instant;

use super::coin::merkle_hash;
use crate::circuit::spend_contract::SpendContract;
use crate::error::{Error, Result};
use crate::serial::{Decodable, Encodable};

pub struct SpendRevealedValues {
    pub value_commit: jubjub::SubgroupPoint,
    pub nullifier: [u8; 32],
    // This should not be here, we just have it for debugging
    //coin: [u8; 32],
    pub merkle_root: bls12_381::Scalar,
    pub signature_public: jubjub::SubgroupPoint,
}

impl SpendRevealedValues {
    fn compute(
        value: u64,
        randomness_value: &jubjub::Fr,
        serial: &jubjub::Fr,
        randomness_coin: &jubjub::Fr,
        secret: &jubjub::Fr,
        merkle_path: &[(bls12_381::Scalar, bool)],
        signature_secret: &jubjub::Fr,
    ) -> Self {
        let value_commit = (zcash_primitives::constants::VALUE_COMMITMENT_VALUE_GENERATOR
            * jubjub::Fr::from(value))
            + (zcash_primitives::constants::VALUE_COMMITMENT_RANDOMNESS_GENERATOR
                * randomness_value);

        let mut nullifier = [0; 32];
        nullifier.copy_from_slice(
            Blake2sParams::new()
                .hash_length(32)
                .personal(zcash_primitives::constants::PRF_NF_PERSONALIZATION)
                .to_state()
                .update(&secret.to_bytes())
                .update(&serial.to_bytes())
                .finalize()
                .as_bytes(),
        );

        let public = zcash_primitives::constants::SPENDING_KEY_GENERATOR * secret;
        let signature_public =
            zcash_primitives::constants::SPENDING_KEY_GENERATOR * signature_secret;

        let mut coin = [0; 32];
        coin.copy_from_slice(
            Blake2sParams::new()
                .hash_length(32)
                .personal(zcash_primitives::constants::CRH_IVK_PERSONALIZATION)
                .to_state()
                .update(&public.to_bytes())
                .update(&value.to_le_bytes())
                .update(&serial.to_bytes())
                .update(&randomness_coin.to_bytes())
                .finalize()
                .as_bytes(),
        );

        let merkle_root =
            jubjub::ExtendedPoint::from(zcash_primitives::pedersen_hash::pedersen_hash(
                zcash_primitives::pedersen_hash::Personalization::NoteCommitment,
                multipack::bytes_to_bits_le(&coin),
            ));
        let affine = merkle_root.to_affine();
        let mut merkle_root = affine.get_u();

        for (i, (right, is_right)) in merkle_path.iter().enumerate() {
            if *is_right {
                merkle_root = merkle_hash(i, &right.to_repr(), &merkle_root.to_repr());
            } else {
                merkle_root = merkle_hash(i, &merkle_root.to_repr(), &right.to_repr());
            }
        }

        SpendRevealedValues {
            value_commit,
            nullifier,
            merkle_root,
            signature_public,
        }
    }

    fn make_outputs(&self) -> [bls12_381::Scalar; 7] {
        let mut public_input = [bls12_381::Scalar::zero(); 7];

        // CV
        {
            let result = jubjub::ExtendedPoint::from(self.value_commit);
            let affine = result.to_affine();
            //let (u, v) = (affine.get_u(), affine.get_v());
            let u = affine.get_u();
            let v = affine.get_v();
            public_input[0] = u;
            public_input[1] = v;
        }

        // NF
        {
            // Pack the hash as inputs for proof verification.
            let hash = multipack::bytes_to_bits_le(&self.nullifier);
            let hash = multipack::compute_multipacking(&hash);

            // There are 2 chunks for a blake hash
            assert_eq!(hash.len(), 2);

            public_input[2] = hash[0];
            public_input[3] = hash[1];
        }

        // Not revealed. We leave this code here for debug
        // Coin
        /*{
            // Pack the hash as inputs for proof verification.
            let hash = multipack::bytes_to_bits_le(&self.coin);
            let hash = multipack::compute_multipacking(&hash);

            // There are 2 chunks for a blake hash
            assert_eq!(hash.len(), 2);

            public_input[4] = hash[0];
            public_input[5] = hash[1];
        }*/

        public_input[4] = self.merkle_root;

        {
            let result = jubjub::ExtendedPoint::from(self.signature_public);
            let affine = result.to_affine();
            //let (u, v) = (affine.get_u(), affine.get_v());
            let u = affine.get_u();
            let v = affine.get_v();
            public_input[5] = u;
            public_input[6] = v;
        }

        public_input
    }
}

impl Encodable for SpendRevealedValues {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += self.value_commit.encode(&mut s)?;
        len += self.nullifier.encode(&mut s)?;
        len += self.merkle_root.encode(&mut s)?;
        len += self.signature_public.encode(s)?;
        Ok(len)
    }
}

impl Decodable for SpendRevealedValues {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        Ok(Self {
            value_commit: Decodable::decode(&mut d)?,
            nullifier: Decodable::decode(&mut d)?,
            merkle_root: Decodable::decode(&mut d)?,
            signature_public: Decodable::decode(d)?,
        })
    }
}

pub fn setup_spend_prover() -> groth16::Parameters<Bls12> {
    println!("Making random params...");
    let start = Instant::now();
    let params = {
        let c = SpendContract {
            value: None,
            randomness_value: None,
            serial: None,
            randomness_coin: None,
            secret: None,

            branch_0: None,
            is_right_0: None,
            branch_1: None,
            is_right_1: None,
            branch_2: None,
            is_right_2: None,
            branch_3: None,
            is_right_3: None,

            signature_secret: None,
        };
        groth16::generate_random_parameters::<Bls12, _, _>(c, &mut OsRng).unwrap()
    };
    println!("Setup: [{:?}]", start.elapsed());
    params
}

pub fn create_spend_proof(
    params: &groth16::Parameters<Bls12>,
    value: u64,
    randomness_value: jubjub::Fr,
    serial: jubjub::Fr,
    randomness_coin: jubjub::Fr,
    secret: jubjub::Fr,
    merkle_path: Vec<(bls12_381::Scalar, bool)>,
    signature_secret: jubjub::Fr,
) -> (groth16::Proof<Bls12>, SpendRevealedValues) {
    assert_eq!(merkle_path.len(), 4);
    assert_eq!(
        merkle_path.len(),
        super::coin::SAPLING_COMMITMENT_TREE_DEPTH
    );
    let c = SpendContract {
        value: Some(value),
        randomness_value: Some(randomness_value),
        serial: Some(serial),
        randomness_coin: Some(randomness_coin),
        secret: Some(secret),

        branch_0: Some(merkle_path[0].0),
        is_right_0: Some(merkle_path[0].1),
        branch_1: Some(merkle_path[1].0),
        is_right_1: Some(merkle_path[1].1),
        branch_2: Some(merkle_path[2].0),
        is_right_2: Some(merkle_path[2].1),
        branch_3: Some(merkle_path[3].0),
        is_right_3: Some(merkle_path[3].1),
        signature_secret: Some(signature_secret),
    };

    let start = Instant::now();
    let proof = groth16::create_random_proof(c, params, &mut OsRng).unwrap();
    println!("Prove: [{:?}]", start.elapsed());

    let revealed = SpendRevealedValues::compute(
        value,
        &randomness_value,
        &serial,
        &randomness_coin,
        &secret,
        &merkle_path,
        &signature_secret,
    );

    (proof, revealed)
}

pub fn verify_spend_proof(
    pvk: &groth16::PreparedVerifyingKey<Bls12>,
    proof: &groth16::Proof<Bls12>,
    revealed: &SpendRevealedValues,
) -> bool {
    let public_input = revealed.make_outputs();

    let start = Instant::now();
    let result = groth16::verify_proof(pvk, proof, &public_input).is_ok();
    println!("Verify: [{:?}]", start.elapsed());
    result
}
