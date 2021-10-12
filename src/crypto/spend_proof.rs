use bellman::gadgets::multipack;
use bellman::groth16;
use blake2s_simd::Params as Blake2sParams;
use bls12_381::Bls12;
use ff::PrimeField;
use group::{Curve, GroupEncoding};
use rand::rngs::OsRng;
use std::io;
use std::time::Instant;

use super::merkle_node::{merkle_hash, MerkleNode, SAPLING_COMMITMENT_TREE_DEPTH};
use super::nullifier::Nullifier;
use crate::circuit::spend_contract::SpendContract;
use crate::error::Result;
use crate::serial::{Decodable, Encodable};

pub struct SpendRevealedValues {
    pub value_commit: jubjub::SubgroupPoint,
    pub asset_commit: jubjub::SubgroupPoint,
    pub nullifier: Nullifier,
    // This should not be here, we just have it for debugging
    //coin: [u8; 32],
    pub merkle_root: MerkleNode,
    pub signature_public: jubjub::SubgroupPoint,
}

impl SpendRevealedValues {
    #[allow(clippy::too_many_arguments)]
    fn compute(
        value: u64,
        token_id: jubjub::Fr,
        randomness_value: &jubjub::Fr,
        randomness_asset: &jubjub::Fr,
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

        let asset_commit = (zcash_primitives::constants::VALUE_COMMITMENT_VALUE_GENERATOR
            * token_id)
            + (zcash_primitives::constants::VALUE_COMMITMENT_RANDOMNESS_GENERATOR
                * randomness_asset);

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
        let nullifier = Nullifier::new(nullifier);

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
                .update(&token_id.to_bytes())
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

        let merkle_root = MerkleNode::new(merkle_root.to_repr());

        SpendRevealedValues {
            value_commit,
            asset_commit,
            nullifier,
            merkle_root,
            signature_public,
        }
    }

    fn make_outputs(&self) -> [bls12_381::Scalar; 9] {
        let mut public_input = [bls12_381::Scalar::zero(); 9];

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

        // CA
        {
            let result = jubjub::ExtendedPoint::from(self.asset_commit);
            let affine = result.to_affine();
            //let (u, v) = (affine.get_u(), affine.get_v());
            let u = affine.get_u();
            let v = affine.get_v();
            public_input[2] = u;
            public_input[3] = v;
        }

        // NF
        {
            // Pack the hash as inputs for proof verification.
            let hash = multipack::bytes_to_bits_le(&self.nullifier.repr);
            let hash = multipack::compute_multipacking(&hash);

            // There are 2 chunks for a blake hash
            assert_eq!(hash.len(), 2);

            public_input[4] = hash[0];
            public_input[5] = hash[1];
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

        public_input[6] = self.merkle_root.into();

        {
            let result = jubjub::ExtendedPoint::from(self.signature_public);
            let affine = result.to_affine();
            //let (u, v) = (affine.get_u(), affine.get_v());
            let u = affine.get_u();
            let v = affine.get_v();
            public_input[7] = u;
            public_input[8] = v;
        }

        public_input
    }
}

impl Encodable for SpendRevealedValues {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += self.value_commit.encode(&mut s)?;
        len += self.asset_commit.encode(&mut s)?;
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
            asset_commit: Decodable::decode(&mut d)?,
            nullifier: Decodable::decode(&mut d)?,
            merkle_root: Decodable::decode(&mut d)?,
            signature_public: Decodable::decode(d)?,
        })
    }
}

pub fn setup_spend_prover() -> groth16::Parameters<Bls12> {
    println!("Spend: Making random params...");
    let start = Instant::now();
    let params = {
        let c = SpendContract {
            value: None,
            token_id: None,
            randomness_value: None,
            randomness_asset: None,
            serial: None,
            randomness_coin: None,
            secret: None,

            branch: [None; SAPLING_COMMITMENT_TREE_DEPTH],
            is_right: [None; SAPLING_COMMITMENT_TREE_DEPTH],

            signature_secret: None,
        };
        groth16::generate_random_parameters::<Bls12, _, _>(c, &mut OsRng).unwrap()
    };
    println!("Setup: [{:?}]", start.elapsed());
    params
}

#[allow(clippy::too_many_arguments)]
pub fn create_spend_proof(
    params: &groth16::Parameters<Bls12>,
    value: u64,
    token_id: jubjub::Fr,
    randomness_value: jubjub::Fr,
    randomness_asset: jubjub::Fr,
    serial: jubjub::Fr,
    randomness_coin: jubjub::Fr,
    secret: jubjub::Fr,
    merkle_path: Vec<(bls12_381::Scalar, bool)>,
    signature_secret: jubjub::Fr,
) -> (groth16::Proof<Bls12>, SpendRevealedValues) {
    assert_eq!(merkle_path.len(), SAPLING_COMMITMENT_TREE_DEPTH);
    let mut branch: [_; SAPLING_COMMITMENT_TREE_DEPTH] = Default::default();
    let mut is_right: [_; SAPLING_COMMITMENT_TREE_DEPTH] = Default::default();
    for (i, (branch_i, is_right_i)) in merkle_path.iter().enumerate() {
        branch[i] = Some(*branch_i);
        is_right[i] = Some(*is_right_i);
    }
    let c = SpendContract {
        value: Some(value),
        token_id: Some(token_id),
        randomness_value: Some(randomness_value),
        randomness_asset: Some(randomness_asset),
        serial: Some(serial),
        randomness_coin: Some(randomness_coin),
        secret: Some(secret),

        branch,
        is_right,

        signature_secret: Some(signature_secret),
    };

    let start = Instant::now();
    let proof = groth16::create_random_proof(c, params, &mut OsRng).unwrap();
    println!("Prove: [{:?}]", start.elapsed());

    let revealed = SpendRevealedValues::compute(
        value,
        token_id,
        &randomness_value,
        &randomness_asset,
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
