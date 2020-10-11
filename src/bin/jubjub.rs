use sapvi::{BlsStringConversion, Decodable, ZKSupervisor};
use std::fs::File;
use std::time::Instant;

use bls12_381::Scalar;
use ff::{Field, PrimeField};
use group::{Curve, Group, GroupEncoding};
use rand::rngs::OsRng;

type Result<T> = std::result::Result<T, failure::Error>;

fn main() -> Result<()> {
    // Load the contract from file

    let start = Instant::now();
    let file = File::open("jubjub.zcd")?;
    let mut visor = ZKSupervisor::decode(file)?;
    println!("Loaded contract '{}': [{:?}]", visor.name, start.elapsed());

    println!("Stats:");
    println!("    Constants: {}", visor.vm.constants.len());
    println!("    Alloc: {}", visor.vm.alloc.len());
    println!("    Operations: {}", visor.vm.ops.len());
    println!(
        "    Constraint Instructions: {}",
        visor.vm.constraints.len()
    );

    // Do the trusted setup

    visor.setup();

    // Put in our input parameters

    visor.set_param(
        "x1",
        Scalar::from_string("15a36d1f0f390d8852a35a8c1908dd87a361ee3fd48fdf77b9819dc82d90607e"),
    )?;
    visor.set_param(
        "y1",
        Scalar::from_string("015d8c7f5b43fe33f7891142c001d9251f3abeeb98fad3e87b0dc53c4ebf1891"),
    )?;
    visor.set_param(
        "x2",
        Scalar::from_string("15a36d1f0f390d8852a35a8c1908dd87a361ee3fd48fdf77b9819dc82d90607e"),
    )?;
    visor.set_param(
        "y2",
        Scalar::from_string("015d8c7f5b43fe33f7891142c001d9251f3abeeb98fad3e87b0dc53c4ebf1891"),
    )?;

    // Generate the ZK proof

    let proof = visor.prove()?;

    // Test and show our output values

    assert_eq!(proof.public.len(), 2);
    // 0x66ced46f14e5616d12b993f60a6e66558d6b6afe4c321ed212e0b9cfbd81061a
    assert_eq!(
        *proof.public.get("x3").unwrap(),
        Scalar::from_string("66ced46f14e5616d12b993f60a6e66558d6b6afe4c321ed212e0b9cfbd81061a")
    );
    // 0x4731570fdd57cf280eadc8946fa00df81112502e44e497e794ab9a221f1bcca
    assert_eq!(
        *proof.public.get("y3").unwrap(),
        Scalar::from_string("04731570fdd57cf280eadc8946fa00df81112502e44e497e794ab9a221f1bcca")
    );
    println!("u = {:?}", proof.public.get("x3").unwrap());
    println!("v = {:?}", proof.public.get("y3").unwrap());

    // Verify the proof

    assert!(visor.verify(&proof));

    Ok(())
}
