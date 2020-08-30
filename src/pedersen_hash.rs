use bellman::{
    gadgets::{
        boolean::{AllocatedBit, Boolean},
        multipack,
    },
    groth16, Circuit, ConstraintSystem, SynthesisError,
};
use bls12_381::Bls12;
use group::Curve;
use rand::rngs::OsRng;

pub const CRH_IVK_PERSONALIZATION: &[u8; 8] = b"Zcashivk";

struct MyCircuit {
    /// The input to SHA-256d we are proving that we know. Set to `None` when we
    /// are verifying a proof (and do not have the witness data).
    preimage: Option<[u8; 80]>,
}

impl Circuit<bls12_381::Scalar> for MyCircuit {
    fn synthesize<CS: ConstraintSystem<bls12_381::Scalar>>(
        self,
        cs: &mut CS,
    ) -> Result<(), SynthesisError> {
        // Compute the values for the bits of the preimage. If we are verifying a proof,
        // we still need to create the same constraints, so we return an equivalent-size
        // Vec of None (indicating that the value of each bit is unknown).
        let bit_values = if let Some(preimage) = self.preimage {
            preimage
                .iter()
                .map(|byte| (0..8).map(move |i| (byte >> i) & 1u8 == 1u8))
                .flatten()
                .map(|b| Some(b))
                .collect()
        } else {
            vec![None; 80 * 8]
        };
        assert_eq!(bit_values.len(), 80 * 8);

        // Witness the bits of the preimage.
        let preimage_bits = bit_values
            .into_iter()
            .enumerate()
            // Allocate each bit.
            .map(|(i, b)| AllocatedBit::alloc(cs.namespace(|| format!("preimage bit {}", i)), b))
            // Convert the AllocatedBits into Booleans (required for the sha256 gadget).
            .map(|b| b.map(Boolean::from))
            .collect::<Result<Vec<_>, _>>()?;

        let hash = zcash_proofs::circuit::pedersen_hash::pedersen_hash(
            cs.namespace(|| "computation of ivk"),
            zcash_primitives::pedersen_hash::Personalization::MerkleTree(0),
            &preimage_bits,
        )?;

        hash.get_u().inputize(cs.namespace(|| "commitment"))?;

        Ok(())
    }
}

fn main() {
    use std::time::Instant;

    let start = Instant::now();
    println!("Starting...");
    // Create parameters for our circuit. In a production deployment these would
    // be generated securely using a multiparty computation.
    let params = {
        let c = MyCircuit { preimage: None };
        groth16::generate_random_parameters::<Bls12, _, _>(c, &mut OsRng).unwrap()
    };
    println!("Generated random params. [{:?}]", start.elapsed());

    let start = Instant::now();
    // Prepare the verification key (for proof verification).
    let pvk = groth16::prepare_verifying_key(&params.vk);
    println!("Prepared verify key [{:?}]", start.elapsed());

    let start = Instant::now();
    // Pick a preimage and compute its hash.
    let preimage = [42; 80];
    //let hash = Sha256::digest(&Sha256::digest(&preimage));
    println!(
        "Computed pedersen_hash(preimage) witness data [{:?}]",
        start.elapsed()
    );

    // Create an instance of our circuit (with the preimage as a witness).
    let c = MyCircuit {
        preimage: Some(preimage),
    };

    let start = Instant::now();
    // Create a Groth16 proof with our parameters.
    let proof = groth16::create_random_proof(c, &params, &mut OsRng).unwrap();
    println!("Generated random proof [{:?}]", start.elapsed());

    let start = Instant::now();

    let input_bools: Vec<bool> = preimage
        .iter()
        .map(|byte| (0..8).map(move |i| (byte >> i) & 1u8 == 1u8))
        .flatten()
        .collect();
    let hash_result = jubjub::ExtendedPoint::from(zcash_primitives::pedersen_hash::pedersen_hash(
        zcash_primitives::pedersen_hash::Personalization::MerkleTree(0),
        input_bools.into_iter(),
    ));

    let mut public_input = [bls12_381::Scalar::zero(); 1];
    {
        let affine = hash_result.to_affine();
        //let (u, v) = (affine.get_u(), affine.get_v());
        let u = affine.get_u();
        public_input[0] = u;
    }

    // Pack the hash as inputs for proof verification.

    println!("Packed data and verifying proof... [{:?}]", start.elapsed());

    let start = Instant::now();
    // Check the proof!
    assert!(groth16::verify_proof(&pvk, &proof, &public_input).is_ok());
    println!("Done! [{:?}]", start.elapsed());
}
