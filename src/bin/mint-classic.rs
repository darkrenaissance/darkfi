use bellman::gadgets::multipack;
use bellman::groth16;
use blake2s_simd::Params as Blake2sParams;
use bls12_381::Bls12;
use ff::Field;
use group::{Curve, Group, GroupEncoding};

use sapvi::crypto::{save_params, load_params, setup_mint_prover, create_mint_proof, verify_mint_proof};

fn main() {
    use rand::rngs::OsRng;
    use std::time::Instant;

    let public = jubjub::SubgroupPoint::random(&mut OsRng);

    let value = 110;
    let randomness_value: jubjub::Fr = jubjub::Fr::random(&mut OsRng);

    let serial: jubjub::Fr = jubjub::Fr::random(&mut OsRng);
    let randomness_coin: jubjub::Fr = jubjub::Fr::random(&mut OsRng);

    {
        let params = setup_mint_prover();
        save_params("mint.params", &params);
    }
    let (params, pvk) = load_params("mint.params").expect("params should load");

    let (proof, revealed) = create_mint_proof(&params, value, randomness_value, serial, randomness_coin,
                                              public);

    assert!(verify_mint_proof(&pvk, &proof, &revealed));
}
