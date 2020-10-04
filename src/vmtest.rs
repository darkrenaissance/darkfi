use bls12_381::Scalar;

mod vm;
mod vm_load;
use vm_load::load_zkvm;

fn main() {
    let mut vm = load_zkvm();

    vm.setup();

    let params = vec![
        (0, Scalar::from(3))
    ];
    vm.initialize(&params);

    let proof = vm.prove();

    let public = vm.public();
    assert!(vm.verify(&proof, &public));
}
