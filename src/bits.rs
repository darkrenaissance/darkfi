use bls12_381::Scalar;

mod bits_contract;
mod vm;
use bits_contract::load_zkvm;

fn main() -> std::result::Result<(), vm::ZKVMError> {
    let mut vm = load_zkvm();

    vm.setup();

    let params = vec![(
        0,
        Scalar::from_raw([
            0xb981_9dc8_2d90_607e,
            0xa361_ee3f_d48f_df77,
            0x52a3_5a8c_1908_dd87,
            0x15a3_6d1f_0f39_0d88,
        ]),
    )];
    vm.initialize(&params)?;

    let proof = vm.prove();

    let public = vm.public();

    assert_eq!(public.len(), 0);

    assert!(vm.verify(&proof, &public));
    Ok(())
}
