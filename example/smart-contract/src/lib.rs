use borsh::{BorshDeserialize, BorshSerialize};
use drk_sdk::{
    entrypoint,
    error::{ContractError, ContractResult},
    msg,
};
use pasta_curves::pallas;

use darkfi::serial::{deserialize, SerialDecodable, SerialEncodable};

#[derive(BorshSerialize, BorshDeserialize)]
pub struct Args {
    pub a: pallas::Base,
    pub b: pallas::Base,
}

#[derive(SerialEncodable, SerialDecodable)]
pub struct Foo {
    pub a: u64,
    pub b: u64,
}

entrypoint!(process_instruction);
fn process_instruction(ix: &[u8]) -> ContractResult {
    //let args = Args::try_from_slice(ix)?;
    let args: Foo = deserialize(ix)?;

    if args.a < args.b {
        return Err(ContractError::Custom(69))
    }

    let sum = args.a + args.b;

    msg!("Hello from the VM runtime!");
    msg!("Sum: {:?}", sum);

    Ok(())
}
