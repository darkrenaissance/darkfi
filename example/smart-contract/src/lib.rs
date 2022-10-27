/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2022 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use darkfi_sdk::{
    crypto::Nullifier,
    entrypoint,
    error::{ContractError, ContractResult},
    msg,
    pasta::pallas,
    state::nullifier_exists,
};
use darkfi_serial::{deserialize, SerialDecodable, SerialEncodable};

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
fn process_instruction(/*_state: &[u8], */ ix: &[u8]) -> ContractResult {
    msg!("Hello from the VM runtime!");
    // Deserialize the payload into `Args`.
    let args: Args = deserialize(ix)?;
    msg!("deserializing payload worked");

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
