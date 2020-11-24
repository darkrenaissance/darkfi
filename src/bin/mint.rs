use sapvi::{Decodable, ZKContract};
use std::fs::File;
use std::time::Instant;

use bls12_381::Scalar;
use ff::{Field, PrimeField};
use group::{Curve, Group};
use rand::rngs::OsRng;

type Result<T> = std::result::Result<T, failure::Error>;

// Unpack a value (such as jubjub::Fr) into 256 Scalar binary digits
fn unpack<F: PrimeField>(value: F) -> Vec<Scalar> {
    let mut bits = Vec::new();
    print!("Unpack: ");
    for (_i, bit) in value.to_le_bits().into_iter().cloned().enumerate() {
        match bit {
            true => bits.push(Scalar::one()),
            false => bits.push(Scalar::zero()),
        }
        print!("{}", if bit { 1 } else { 0 });
    }
    println!("");
    bits
}

// Unpack a u64 value in 64 Scalar binary digits
fn unpack_u64(value: u64) -> Vec<Scalar> {
    let mut result = Vec::with_capacity(64);

    for i in 0..64 {
        if (value >> i) & 1 == 1 {
            result.push(Scalar::one());
        } else {
            result.push(Scalar::zero());
        }
    }

    result
}

fn main() -> Result<()> {
    let start = Instant::now();
    let file = File::open("mint.zcd")?;
    let mut visor = ZKContract::decode(file)?;
    println!("{}", visor.name);
    //ZKContract::load_contract(bytes);
    println!("Loaded contract: [{:?}]", start.elapsed());

    println!("Stats:");
    println!("    Constants: {}", visor.vm.constants.len());
    println!("    Alloc: {}", visor.vm.alloc.len());
    println!("    Operations: {}", visor.vm.ops.len());
    println!(
        "    Constraint Instructions: {}",
        visor.vm.constraints.len()
    );

    visor.setup("mint.zts");

    // We use the ExtendedPoint in calculations because it's faster
    let public_point = jubjub::ExtendedPoint::from(jubjub::SubgroupPoint::random(&mut OsRng));
    // But to serialize we need to convert to affine (which has the (u, v) values)
    let public_affine = public_point.to_affine();

    let randomness_value: jubjub::Fr = jubjub::Fr::random(&mut OsRng);

    for param in visor.param_names() {
        println!("Param name: {}", param);
    }

    visor.set_param("public_u", public_affine.get_u())?;
    visor.set_param("public_v", public_affine.get_v())?;
    for (i, param_bit) in unpack(randomness_value).into_iter().enumerate() {
        visor.set_param(&format!("vc_randomness_{}", i), param_bit)?;
    }

    let proof = visor.prove()?;

    assert_eq!(proof.public.len(), 2);
    for (name, value) in &proof.public {
        println!("Public {} = {:?}", name, value);
    }

    assert!(visor.verify(&proof));

    Ok(())
}
