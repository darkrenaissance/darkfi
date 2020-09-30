use bellman::{
    gadgets::{
        boolean::{AllocatedBit, Boolean},
        multipack, num, Assignment,
    },
    groth16, Circuit, ConstraintSystem, SynthesisError,
};
use bls12_381::Bls12;
use bls12_381::Scalar;
use ff::{Field, PrimeField};
use group::Curve;
use rand::rngs::OsRng;
use std::ops::{Neg, SubAssign, MulAssign};

pub const CRH_IVK_PERSONALIZATION: &[u8; 8] = b"Zcashivk";

struct MyCircuit {
    aux: Vec<Option<bls12_381::Scalar>>,
}

impl Circuit<bls12_381::Scalar> for MyCircuit {
    fn synthesize<CS: ConstraintSystem<bls12_381::Scalar>>(
        self,
        cs: &mut CS,
    ) -> Result<(), SynthesisError> {
        //let x = num::AllocatedNum::alloc(cs.namespace(|| "conditional anchor"), || {
        //    Ok(*self.aux_values[0].get()?)
        //})?;

        //let x2 = x.mul(cs.namespace(|| "x2"), &x)?;
        //let x3 = x.mul(cs.namespace(|| "x2"), &x2)?;
        //x3.inputize(cs.namespace(|| "pubx2"))?;

        // ------------------

        // x
        let x_var = cs.alloc(
            || "num",
            || {
                Ok(*self.aux[0].get()?)
            },
        )?;

        let coeff = bls12_381::Scalar::one();
        let lc0 = bellman::LinearCombination::zero() + (coeff, x_var);
        let lc1 = bellman::LinearCombination::zero() + (coeff, CS::one());
        let lc2 = bellman::LinearCombination::zero() + (coeff, x_var);

        cs.enforce(
            || "multiplication constraint",
            |_| lc0,
            |_| lc1,
            |_| lc2,
        );

        // x2 = x * x

        let x2_var = cs.alloc(
            || "product num",
            || {
                Ok(*self.aux[1].get()?)
            },
        )?;

        let coeff = bls12_381::Scalar::one();
        let lc0 = bellman::LinearCombination::zero() + (coeff, x_var);
        let lc1 = bellman::LinearCombination::zero() + (coeff, x_var);
        let lc2 = bellman::LinearCombination::zero() + (coeff, x2_var);

        cs.enforce(
            || "multiplication constraint",
            |_| lc0,
            |_| lc1,
            |_| lc2,
        );

        // x3 = x2 * x

        let x3_var = cs.alloc(
            || "product num",
            || {
                Ok(*self.aux[2].get()?)
            },
        )?;

        let coeff = bls12_381::Scalar::one();
        let lc0 = bellman::LinearCombination::zero() + (coeff, x2_var);
        let lc1 = bellman::LinearCombination::zero() + (coeff, x_var);
        let lc2 = bellman::LinearCombination::zero() + (coeff, x3_var);

        cs.enforce(
            || "multiplication constraint",
            |_| lc0,
            |_| lc1,
            |_| lc2,
        );

        // inputize values

        let input = cs.alloc_input(|| "input variable", || Ok(*self.aux[2].get()?))?;

        let coeff = bls12_381::Scalar::one();
        let lc0 = bellman::LinearCombination::zero() + (coeff, input);
        let lc1 = bellman::LinearCombination::zero() + (coeff, CS::one());
        let lc2 = bellman::LinearCombination::zero() + (coeff, x3_var);

        cs.enforce(
            || "enforce input is correct",
            |_| lc0,
            |_| lc1,
            |_| lc2,
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
        let c = MyCircuit {
            aux: vec![None],
        };
        groth16::generate_random_parameters::<Bls12, _, _>(c, &mut OsRng).unwrap()
    };
    println!("Setup: [{:?}]", start.elapsed());

    // Prepare the verification key (for proof verification).
    let pvk = groth16::prepare_verifying_key(&params.vk);

    // Pick a preimage and compute its hash.
    let quantity = bls12_381::Scalar::from(3);

    // Create an instance of our circuit (with the preimage as a witness).
    let c = MyCircuit {
        aux: vec![
            Some(quantity),
            Some(quantity * quantity),
            Some(quantity * quantity * quantity)
        ],
    };

    let start = Instant::now();
    // Create a Groth16 proof with our parameters.
    let proof = groth16::create_random_proof(c, &params, &mut OsRng).unwrap();
    println!("Prove: [{:?}]", start.elapsed());

    let start = Instant::now();

    let public_input = vec![bls12_381::Scalar::from(27)];

    let start = Instant::now();
    // Check the proof!
    assert!(groth16::verify_proof(&pvk, &proof, &public_input).is_ok());
    println!("Verify: [{:?}]", start.elapsed());
}
