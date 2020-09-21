use bellman::groth16;
use bellman::gadgets::multipack;
use bls12_381::Bls12;
use ff::Field;
use group::{Curve, Group, GroupEncoding};
use blake2s_simd::Params as Blake2sParams;

mod simple_circuit;
use simple_circuit::InputSpend;

fn main() {
    use rand::rngs::OsRng;

    let ak = jubjub::SubgroupPoint::random(&mut OsRng);

    let secret: jubjub::Fr = jubjub::Fr::random(&mut OsRng);
    let public = zcash_primitives::constants::SPENDING_KEY_GENERATOR * secret + ak;

    let params = {
        let c = InputSpend {
            secret: None,
            ak: None,
            value: None,
        };
        groth16::generate_random_parameters::<Bls12, _, _>(c, &mut OsRng).unwrap()
    };
    let pvk = groth16::prepare_verifying_key(&params.vk);

    let c = InputSpend {
        secret: Some(secret),
        ak: Some(ak),
        value: Some(110),
    };

    let proof = groth16::create_random_proof(c, &params, &mut OsRng).unwrap();

    let mut public_input = [bls12_381::Scalar::zero(); 4];
    {
        let result = jubjub::ExtendedPoint::from(public);
        let affine = result.to_affine();
        //let (u, v) = (affine.get_u(), affine.get_v());
        let u = affine.get_u();
        let v = affine.get_v();
        public_input[0] = u;
        public_input[1] = v;
    }

    {
        const CRH_IVK_PERSONALIZATION: &[u8; 8] = b"Zcashivk";
        let preimage = [42; 80];
        let hash_result = {
            let mut hash = [0; 32];
            hash.copy_from_slice(
                Blake2sParams::new()
                .hash_length(32)
                .personal(CRH_IVK_PERSONALIZATION)
                .to_state()
                .update(&ak.to_bytes())
                .finalize()
                .as_bytes()
            );
            hash
        };

        // Pack the hash as inputs for proof verification.
        let hash = multipack::bytes_to_bits_le(&hash_result);
        let hash = multipack::compute_multipacking(&hash);

        // There are 2 chunks for a blake hash
        assert_eq!(hash.len(), 2);

        public_input[2] = hash[0];
        public_input[3] = hash[1];
    }

    assert!(groth16::verify_proof(&pvk, &proof, &public_input).is_ok());
}
