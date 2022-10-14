use darkfi::serial::{deserialize, SerialDecodable, SerialEncodable};
use darkfi_sdk::{
    crypto::Nullifier,
    entrypoint,
    error::{ContractError, ContractResult},
    msg,
    pasta::pallas,
    state::nullifier_exists,
};

// An example of deserializing the payload into a struct
#[derive(SerialEncodable, SerialDecodable)]
pub struct Args {
    pub a: u64,
    pub b: u64,
}

// This is the main entrypoint function where the payload is fed.
// Through here, you can branch out into different functions inside
// this library.
entrypoint!(process_instruction);
fn process_instruction(ix: &[u8]) -> ContractResult {
    // Deserialize the payload into `Args`.
    let args: Args = deserialize(ix)?;

    if args.a < args.b {
        // Returning custom errors
        return Err(ContractError::Custom(69))
    }

    let sum = args.a + args.b;
    // Publicly logged messages
    msg!("Hello from the VM runtime!");
    msg!("Sum: {:?}", sum);

    // Querying of ledger state available from the VM host
    let nf = Nullifier::from(pallas::Base::from(0x10));
    msg!("Contract Nullifier: {:?}", nf);

    if nullifier_exists(&nf)? {
        msg!("Nullifier exists");
    } else {
        msg!("Nullifier doesn't exist");
    }

    Ok(())
}
