use bellman::{
    gadgets::{
        boolean,
        boolean::{AllocatedBit, Boolean},
        multipack,
    },
    groth16, Circuit, ConstraintSystem, SynthesisError,
};
use bls12_381::Bls12;
use ff::{PrimeField, Field};
use group::Curve;
use rand::rngs::OsRng;

use zcash_proofs::constants::{
    SPENDING_KEY_GENERATOR
};

//pub const CRH_IVK_PERSONALIZATION: &[u8; 8] = b"Zcashivk";

struct MyCircuit {
    secret: Option<jubjub::Fr>
}

impl Circuit<bls12_381::Scalar> for MyCircuit {
    fn synthesize<CS: ConstraintSystem<bls12_381::Scalar>>(
        self, cs: &mut CS) -> Result<(), SynthesisError> {

        let secret = boolean::field_into_boolean_vec_le(cs.namespace(|| "secret"), self.secret)?;

        let public = zcash_proofs::circuit::ecc::fixed_base_multiplication(
            cs.namespace(|| "public"),
            &SPENDING_KEY_GENERATOR,
            &secret,
        )?;

        public.inputize(cs.namespace(|| "public"))
    }
}

fn main() {
    use jubjub::*;
    use jubjub::SubgroupPoint;
    use core::ops::{MulAssign, Mul};
    use ff::PrimeField;
    use group::{Group, GroupEncoding};
    //let ak = jubjub::SubgroupPoint::random(&mut OsRng);

    let secret: jubjub::Fr = jubjub::Fr::random(&mut OsRng);
    let public = zcash_primitives::constants::SPENDING_KEY_GENERATOR * secret;

    let params = {
        let c = MyCircuit { secret: None };
        groth16::generate_random_parameters::<Bls12, _, _>(c, &mut OsRng).unwrap()
    };
    let pvk = groth16::prepare_verifying_key(&params.vk);

    let c = MyCircuit {
        secret: Some(secret),
    };

    let proof = groth16::create_random_proof(c, &params, &mut OsRng).unwrap();

    let mut public_input = [bls12_381::Scalar::zero(); 2];
    {
        let result = jubjub::ExtendedPoint::from(public);
        let affine = result.to_affine();
        //let (u, v) = (affine.get_u(), affine.get_v());
        let u = affine.get_u();
        let v = affine.get_v();
        public_input[0] = u;
        public_input[1] = v;
    }

    assert!(groth16::verify_proof(&pvk, &proof, &public_input).is_ok());
}
