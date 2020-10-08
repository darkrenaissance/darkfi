use bls12_381::Scalar;
use group::{Curve, Group, GroupEncoding};

mod mint2_contract;
mod vm;
use mint2_contract::load_zkvm;

fn main() -> std::result::Result<(), vm::ZKVMError> {
    use rand::rngs::OsRng;
    let public_point = jubjub::ExtendedPoint::from(jubjub::SubgroupPoint::random(&mut OsRng));
    let public_affine = public_point.to_affine();

    let mut vm = load_zkvm();

    vm.setup();

    let params = vec![(0, public_affine.get_u()), (1, public_affine.get_v())];
    vm.initialize(&params)?;

    let proof = vm.prove();

    let public = vm.public();

    assert_eq!(public.len(), 0);

    // Use this code for testing point doubling
    //let dbl = public_point.double().to_affine();
    //assert_eq!(public.len(), 2);
    //assert_eq!(public[0], dbl.get_u());
    //assert_eq!(public[1], dbl.get_v());
    //println!("{:?}", dbl.get_u());
    //println!("{:?}", public[0]);
    //println!("{:?}", dbl.get_v());
    //println!("{:?}", public[1]);

    assert!(vm.verify(&proof, &public));
    Ok(())
}
