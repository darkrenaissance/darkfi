use bls12_381::Scalar;
use ff::{PrimeField, Field};
use group::{Curve, Group, GroupEncoding};

mod mint2_contract;
mod vm;
use mint2_contract::load_zkvm;

fn unpack<F: PrimeField>(value: F) -> Vec<Scalar> {
    let mut bits = Vec::new();
    for (i, bit) in value.to_le_bits().into_iter().cloned().enumerate() {
        match bit {
            true => bits.push(Scalar::one()),
            false => bits.push(Scalar::zero()),
        }
    }
    bits
}

fn do_vcr_test(value: &jubjub::Fr) {
    let mut curbase = zcash_primitives::constants::VALUE_COMMITMENT_RANDOMNESS_GENERATOR;
    let mut result = curbase.clone();
    for (i, bit) in value.to_le_bits().into_iter().cloned().enumerate() {
        let thisbase = if bit {
            curbase.clone()
        } else {
            jubjub::SubgroupPoint::identity()
        };
        result += thisbase;
        curbase = curbase.double();
    }
    let result = jubjub::ExtendedPoint::from(result).to_affine();
    println!("cvr1: {:?}", result);
    let randomness_commit =
        zcash_primitives::constants::VALUE_COMMITMENT_RANDOMNESS_GENERATOR * value;
    let randomness_commit = jubjub::ExtendedPoint::from(result).to_affine();
    println!("cvr2: {:?}", randomness_commit);
}

fn main() -> std::result::Result<(), vm::ZKVMError> {
    use rand::rngs::OsRng;
    let public_point = jubjub::ExtendedPoint::from(jubjub::SubgroupPoint::random(&mut OsRng));
    let public_affine = public_point.to_affine();

    let value = 110;
    let randomness_value: jubjub::Fr = jubjub::Fr::random(&mut OsRng);
    let value_commit = (zcash_primitives::constants::VALUE_COMMITMENT_VALUE_GENERATOR
        * jubjub::Fr::from(value))
        + (zcash_primitives::constants::VALUE_COMMITMENT_RANDOMNESS_GENERATOR * randomness_value);

    /////
    let randomness_commit =
        zcash_primitives::constants::VALUE_COMMITMENT_RANDOMNESS_GENERATOR * randomness_value;
    /////
    do_vcr_test(&randomness_value);

    let mut vm = load_zkvm();

    vm.setup();

    let mut params = vec![
        (0, public_affine.get_u()),
        (1, public_affine.get_v()),
        //(2, randomness_value),
    ];
    for x in unpack(randomness_value) {
        params.push((params.len(), x));
    }
    println!("Size of params: {}", params.len());
    vm.initialize(&params)?;

    let proof = vm.prove();

    let public = vm.public();

    assert_eq!(public.len(), 2);

    // Use this code for testing point doubling
    let dbl = public_point.double().to_affine();
    println!("{:?}", dbl.get_u());
    println!("{:?}", public[0]);
    println!("{:?}", dbl.get_v());
    println!("{:?}", public[1]);
    //assert_eq!(public.len(), 2);
    //assert_eq!(public[0], dbl.get_u());
    //assert_eq!(public[1], dbl.get_v());

    assert!(vm.verify(&proof, &public));
    Ok(())
}
