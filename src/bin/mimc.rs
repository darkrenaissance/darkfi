use bls12_381::Scalar;
use sapvi::{BlsStringConversion, Decodable, ZKContract};
use std::fs::File;
use std::time::Instant;
use ff::{Field, PrimeField};
use std::ops::{Add, AddAssign, MulAssign, Neg, SubAssign};

type Result<T> = std::result::Result<T, failure::Error>;

mod mimc_constants;
use mimc_constants::mimc_constants;

const MIMC_ROUNDS: usize = 322;

fn mimc(mut xl: Scalar, mut xr: Scalar, constants: &[Scalar]) -> Scalar {
    assert_eq!(constants.len(), MIMC_ROUNDS);

    for i in 0..MIMC_ROUNDS {
        let mut tmp1 = xl;
        tmp1.add_assign(&constants[i]);
        let mut tmp2 = tmp1.square();
        tmp2.mul_assign(&tmp1);
        tmp2.add_assign(&xr);
        xr = xl;
        xl = tmp2;
    }

    xl
}

macro_rules! from_slice {
    ($data:expr, $len:literal) => {{
        let mut array = [0; $len];
        // panics if not enough data
        let bytes = &$data[..array.len()];
        assert_eq!(bytes.len(), array.len());
        for (a, b) in array.iter_mut().rev().zip(bytes.iter()) {
            *a = *b;
        }
        //array.copy_from_slice(bytes.iter().rev());
        array
    }};
}

fn main() -> Result<()> {
    /////////////////////////////////
    // Initialize our MiMC constants
    let mut constants = Vec::new();
    for const_str in mimc_constants() {
        let bytes = from_slice!(&hex::decode(const_str).unwrap(), 32);
        assert_eq!(bytes.len(), 32);
        let constant = Scalar::from_bytes(&bytes).unwrap();

        constants.push(constant);
    }
    /////////////////////////////////

    // Load the contract from file

    let start = Instant::now();
    let file = File::open("mimc.zcd")?;
    let mut contract = ZKContract::decode(file)?;
    println!("Loaded contract '{}': [{:?}]", contract.name, start.elapsed());

    println!("Stats:");
    println!("    Constants: {}", contract.vm.constants.len());
    println!("    Alloc: {}", contract.vm.alloc.len());
    println!("    Operations: {}", contract.vm.ops.len());
    println!(
        "    Constraint Instructions: {}",
        contract.vm.constraints.len()
    );

    // Do the trusted setup

    contract.setup();

    // Put in our input parameters

    let left =
            Scalar::from_raw([
                0xb981_9dc8_2d90_607e,
                0xa361_ee3f_d48f_df77,
                0x52a3_5a8c_1908_dd87,
                0x15a3_6d1f_0f39_0d88,
            ]);
            let right = Scalar::from_raw([
                0x7b0d_c53c_4ebf_1891,
                0x1f3a_beeb_98fa_d3e8,
                0xf789_1142_c001_d925,
                0x015d_8c7f_5b43_fe33,
            ]);

    contract.set_param(
        "left_0", left.clone()
    )?;
    contract.set_param(
        "right", right.clone()
    )?;

    // Generate the ZK proof

    let proof = contract.prove()?;

    // Test and show our output values

    let mimc_hash = mimc(left,right, &constants);
    assert_eq!(proof.public.len(), 1);
    // 0x66ced46f14e5616d12b993f60a6e66558d6b6afe4c321ed212e0b9cfbd81061a
    assert_eq!(
        *proof.public.get("hash_result").unwrap(),
        mimc_hash
    );
    println!("hash result = {:?}", proof.public.get("hash_result").unwrap());

    // Verify the proof

    assert!(contract.verify(&proof));

    Ok(())
}
