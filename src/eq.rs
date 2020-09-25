use bellman::{
    gadgets::{
        boolean::{AllocatedBit, Boolean},
        multipack,
        Assignment,
        num
    },
    groth16, Circuit, ConstraintSystem, SynthesisError,
};
use bls12_381::Bls12;
use group::Curve;
use rand::rngs::OsRng;

pub const CRH_IVK_PERSONALIZATION: &[u8; 8] = b"Zcashivk";

struct MyCircuit {
    value: Option<bls12_381::Scalar>,
}

impl Circuit<bls12_381::Scalar> for MyCircuit {
    fn synthesize<CS: ConstraintSystem<bls12_381::Scalar>>(
        self,
        cs: &mut CS,
    ) -> Result<(), SynthesisError> {
        let x = num::AllocatedNum::alloc(cs.namespace(|| "conditional anchor"), || {
            Ok(*self.value.get()?)
        })?;

        cs.enforce(
            || "conditionally enforce correct root",
            |lc| lc + x.get_variable(),
            |lc| lc + CS::one(),
            |lc| lc + x.get_variable(),
        );

        Ok(())
    }
}

fn main() {
    use std::time::Instant;

    let start = Instant::now();
    // Create parameters for our circuit. In a production deployment these would
    // be generated securely using a multiparty computation.
    let params = {
        let c = MyCircuit { value: None };
        groth16::generate_random_parameters::<Bls12, _, _>(c, &mut OsRng).unwrap()
    };
    println!("Setup: [{:?}]", start.elapsed());

    // Prepare the verification key (for proof verification).
    let pvk = groth16::prepare_verifying_key(&params.vk);

    // Pick a preimage and compute its hash.
    let value = bls12_381::Scalar::one();

    // Create an instance of our circuit (with the preimage as a witness).
    let c = MyCircuit {
        value: Some(value),
    };

    let start = Instant::now();
    // Create a Groth16 proof with our parameters.
    let proof = groth16::create_random_proof(c, &params, &mut OsRng).unwrap();
    println!("Prove: [{:?}]", start.elapsed());

    let start = Instant::now();

    let mut public_input = [bls12_381::Scalar::zero(); 0];

    let start = Instant::now();
    // Check the proof!
    assert!(groth16::verify_proof(&pvk, &proof, &public_input).is_ok());
    println!("Verify: [{:?}]", start.elapsed());
}
