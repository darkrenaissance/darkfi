use darkfi::serial::{deserialize, SerialDecodable, SerialEncodable};
use darkfi_sdk::{
    crypto::Nullifier,
    entrypoint,
    error::{ContractError, ContractResult},
    msg,
    state::nullifier_exists,
};
use pasta_curves::pallas;

#[derive(SerialEncodable, SerialDecodable)]
pub struct Args {
    pub a: u64,
    pub b: u64,
}

entrypoint!(process_instruction);
fn process_instruction(ix: &[u8]) -> ContractResult {
    let args: Args = deserialize(ix)?;

    if args.a < args.b {
        return Err(ContractError::Custom(69))
    }

    let sum = args.a + args.b;

    msg!("Hello from the VM runtime!");
    msg!("Sum: {:?}", sum);

    let nf = Nullifier::from(pallas::Base::from(0x10));
    msg!("Contract nf: {:?}", nf);

    if nullifier_exists(&nf)? {
        msg!("Nullifier exists");
    } else {
        msg!("Nullifier doesn't exist");
    }

    Ok(())
}
