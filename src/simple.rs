use bellman::groth16;
use bls12_381::Bls12;
use ff::Field;
use group::Curve;
mod simple_circuit;
use simple_circuit::InputSpend;

fn main() {
    use rand::rngs::OsRng;
    //let ak = jubjub::SubgroupPoint::random(&mut OsRng);

    let secret: jubjub::Fr = jubjub::Fr::random(&mut OsRng);
    let public = zcash_primitives::constants::SPENDING_KEY_GENERATOR * secret;

    let params = {
        let c = InputSpend { secret: None };
        groth16::generate_random_parameters::<Bls12, _, _>(c, &mut OsRng).unwrap()
    };
    let pvk = groth16::prepare_verifying_key(&params.vk);

    let c = InputSpend {
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
