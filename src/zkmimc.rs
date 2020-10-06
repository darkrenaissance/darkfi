use bls12_381::Scalar;
use ff::{PrimeField, Field};
use std::ops::{Add, AddAssign, MulAssign, Neg, SubAssign};

mod zkmimc_contract;
mod vm;
use zkmimc_contract::load_zkvm;
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

fn main() -> std::result::Result<(), vm::ZKVMError> {
    use rand::rngs::OsRng;

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

    let mut vm = load_zkvm();

    vm.setup();

    let params = vec![
        (
            0,
            Scalar::from_raw([
                0xb981_9dc8_2d90_607e,
                0xa361_ee3f_d48f_df77,
                0x52a3_5a8c_1908_dd87,
                0x15a3_6d1f_0f39_0d88,
            ]),
        ),
        (
            1,
            Scalar::from_raw([
                0x7b0d_c53c_4ebf_1891,
                0x1f3a_beeb_98fa_d3e8,
                0xf789_1142_c001_d925,
                0x015d_8c7f_5b43_fe33,
            ]),
        ),
    ];
    vm.initialize(&params)?;

    let proof = vm.prove();

    let public = vm.public();

    let mimc_hash = mimc(params[0].1.clone(), params[1].1.clone(), &constants);
    assert_eq!(public.len(), 1);
    assert_eq!(public[0], mimc_hash);

    assert!(vm.verify(&proof, &public));
    Ok(())
}

