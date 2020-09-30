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
use std::ops::{Neg, SubAssign};

pub const CRH_IVK_PERSONALIZATION: &[u8; 8] = b"Zcashivk";

struct MyCircuit {
    quantity: Option<bls12_381::Scalar>,
    multiplier: Option<bls12_381::Scalar>,
    entry_price: Option<bls12_381::Scalar>,
    exit_price: Option<bls12_381::Scalar>,
}

impl Circuit<bls12_381::Scalar> for MyCircuit {
    fn synthesize<CS: ConstraintSystem<bls12_381::Scalar>>(
        self,
        cs: &mut CS,
    ) -> Result<(), SynthesisError> {
        // Witness variables
        let quantity = num::AllocatedNum::alloc(cs.namespace(|| "conditional anchor"), || {
            Ok(*self.quantity.get()?)
        })?;
        let multiplier = num::AllocatedNum::alloc(cs.namespace(|| "conditional anchor"), || {
            Ok(*self.multiplier.get()?)
        })?;
        let entry_price = num::AllocatedNum::alloc(cs.namespace(|| "conditional anchor"), || {
            Ok(*self.entry_price.get()?)
        })?;
        let exit_price = num::AllocatedNum::alloc(cs.namespace(|| "conditional anchor"), || {
            Ok(*self.exit_price.get()?)
        })?;

        // P = mN (1 - 1/R)
        //   = mN - mN/R
        //   = mN - mN * S_0 * S_T^-1

        // initial_margin = mN
        let initial_margin = multiplier.mul(cs.namespace(|| "initial margin"), &quantity)?;

        // S_T_inv = S_T^-1
        let exit_price_inv =
            num::AllocatedNum::alloc(cs.namespace(|| "exit price inverse"), || {
                let tmp = *exit_price.get_value().get()?;

                if tmp.is_zero() {
                    Err(SynthesisError::DivisionByZero)
                } else {
                    let inv = tmp.invert().unwrap();
                    Ok(inv)
                }
            })?;

        // assert S_T * S_T_inv = 1
        cs.enforce(
            || "constraint inverse exit price",
            |lc| lc + exit_price.get_variable(),
            |lc| lc + exit_price_inv.get_variable(),
            |lc| lc + CS::one(),
        );

        // ungained = initial_margin * S_0 * S_T_inv
        let ungained = initial_margin.mul(cs.namespace(|| "ungained 1"), &entry_price)?;
        let ungained = ungained.mul(cs.namespace(|| "ungained 2"), &exit_price_inv)?;

        // pnl = initial_margin - ungained
        let pnl = num::AllocatedNum::alloc(cs.namespace(|| "exit price inverse"), || {
            let mut tmp = *initial_margin.get_value().get()?;

            tmp.sub_assign(ungained.get_value().get()?);

            Ok(tmp)
        })?;

        cs.enforce(
            || "constraint pnl calc",
            |lc| lc + initial_margin.get_variable() - ungained.get_variable(),
            |lc| lc + CS::one(),
            |lc| lc + pnl.get_variable(),
        );

        // Apply clamp:
        //
        //   if pnl < -initial_margin:
        //       pnl = -initial_margin
        //   if pnl > initial_margin:
        //       pnl = initial_margin

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
            quantity: None,
            multiplier: None,
            entry_price: None,
            exit_price: None,
        };
        groth16::generate_random_parameters::<Bls12, _, _>(c, &mut OsRng).unwrap()
    };
    println!("Setup: [{:?}]", start.elapsed());

    // Prepare the verification key (for proof verification).
    let pvk = groth16::prepare_verifying_key(&params.vk);

    // Pick a preimage and compute its hash.
    let quantity = bls12_381::Scalar::from(1);
    let multiplier = bls12_381::Scalar::from(1);
    let entry_price = bls12_381::Scalar::from(100);
    let exit_price = bls12_381::Scalar::from(200);

    // Create an instance of our circuit (with the preimage as a witness).
    let c = MyCircuit {
        quantity: Some(quantity),
        multiplier: Some(multiplier),
        entry_price: Some(entry_price),
        exit_price: Some(exit_price),
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
