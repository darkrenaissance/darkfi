use bellman::gadgets::multipack;
use bitvec::{order::Lsb0, view::AsBits};
use blake2s_simd::Params as Blake2sParams;
use ff::{Field, PrimeField};
use group::{Curve, GroupEncoding};

use drk::crypto::{
    create_spend_proof, load_params, save_params, setup_spend_prover, verify_spend_proof,
};

// This thing is nasty lol
pub fn merkle_hash(
    depth: usize,
    lhs: &bls12_381::Scalar,
    rhs: &bls12_381::Scalar,
) -> bls12_381::Scalar {
    let lhs = {
        let mut tmp = [false; 256];
        for (a, b) in tmp.iter_mut().zip(lhs.to_repr().as_bits::<Lsb0>()) {
            *a = *b;
        }
        tmp
    };

    let rhs = {
        let mut tmp = [false; 256];
        for (a, b) in tmp.iter_mut().zip(rhs.to_repr().as_bits::<Lsb0>()) {
            *a = *b;
        }
        tmp
    };

    jubjub::ExtendedPoint::from(zcash_primitives::pedersen_hash::pedersen_hash(
        zcash_primitives::pedersen_hash::Personalization::MerkleTree(depth),
        lhs.iter()
            .copied()
            .take(bls12_381::Scalar::NUM_BITS as usize)
            .chain(
                rhs.iter()
                    .copied()
                    .take(bls12_381::Scalar::NUM_BITS as usize),
            ),
    ))
    .to_affine()
    .get_u()
}

struct SpendRevealedValues {
    value_commit: jubjub::SubgroupPoint,
    nullifier: [u8; 32],
    // This should not be here, we just have it for debugging
    //coin: [u8; 32],
    merkle_root: bls12_381::Scalar,
}

impl SpendRevealedValues {
    fn compute(
        value: u64,
        randomness_value: &jubjub::Fr,
        serial: &jubjub::Fr,
        randomness_coin: &jubjub::Fr,
        secret: &jubjub::Fr,
        merkle_path: &[(bls12_381::Scalar, bool)],
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
                merkle_root = merkle_hash(i, &right, &merkle_root);
            } else {
                merkle_root = merkle_hash(i, &merkle_root, &right);
            }
        }

        SpendRevealedValues {
            value_commit,
            nullifier,
            merkle_root,
        }
    }

    fn make_outputs(&self) -> [bls12_381::Scalar; 5] {
        let mut public_input = [bls12_381::Scalar::zero(); 5];

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

        public_input
    }
}

fn main() {
    use rand::rngs::OsRng;

    let value = 110;
    let randomness_value: jubjub::Fr = jubjub::Fr::random(&mut OsRng);

    let serial: jubjub::Fr = jubjub::Fr::random(&mut OsRng);
    let randomness_coin: jubjub::Fr = jubjub::Fr::random(&mut OsRng);
    let secret: jubjub::Fr = jubjub::Fr::random(&mut OsRng);
    let signature_secret: jubjub::Fr = jubjub::Fr::random(&mut OsRng);

    let merkle_path = vec![
        (bls12_381::Scalar::random(&mut OsRng), true),
        (bls12_381::Scalar::random(&mut OsRng), false),
        (bls12_381::Scalar::random(&mut OsRng), true),
        (bls12_381::Scalar::random(&mut OsRng), true),
    ];

    {
        let params = setup_spend_prover();
        save_params("spend.params", &params);
    }
    let (params, pvk) = load_params("spend.params").expect("params should load");

    let signature_public = zcash_primitives::constants::SPENDING_KEY_GENERATOR * signature_secret;

    let (proof, revealed) = create_spend_proof(
        &params,
        value,
        randomness_value,
        serial,
        randomness_coin,
        secret,
        merkle_path,
        signature_secret,
    );

    assert!(verify_spend_proof(&pvk, &proof, &revealed));
    assert_eq!(revealed.signature_public, signature_public);
}
