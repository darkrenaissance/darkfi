/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

use std::{collections::HashMap, env, fs, process};

use darkfi_serial::{deserialize, serialize};

const COMMITMENTS_PATH: &str = "darkirc_rln_commits.bin";
const GENESIS_USER_MSG_LIMIT: u64 = 100;

type Commitment = [u8; 32];
type Secret = [u8; 32];
type IdentityRecord = (Secret, Secret, bool);
type Commitments = HashMap<Commitment, IdentityRecord>;

fn main() {
    if let Err(err) = run() {
        eprintln!("error: {err}");
        process::exit(1);
    }
}

/// Dispatch the requested command.
fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let Some(command) = args.next() else {
        usage();
        return Err("missing command".to_string());
    };

    if args.next().is_some() {
        usage();
        return Err("too many arguments".to_string());
    }

    match command.as_str() {
        "reset" => reset(),
        "produce" => produce(),
        "help" | "-h" | "--help" => {
            usage();
            Ok(())
        }
        _ => {
            usage();
            Err(format!("unknown command `{command}`"))
        }
    }
}

/// Reset every generated identity so it can be produced again.
fn reset() -> Result<(), String> {
    let mut commitments = load_commitments()?;
    for (_, _, used) in commitments.values_mut() {
        *used = false;
    }

    let count = commitments.len();
    write_commitments(&commitments)?;
    println!("reset {count} commitments");
    Ok(())
}

/// Mark the first available identity as used and print its registration command.
fn produce() -> Result<(), String> {
    let mut commitments = load_commitments()?;
    let mut available = commitments
        .iter()
        .filter_map(|(commitment, (_, _, used))| (!*used).then_some(*commitment))
        .collect::<Vec<_>>();
    available.sort_unstable();

    let Some(commitment) = available.into_iter().next() else {
        return Err("no unused commitments available".to_string());
    };

    let (nullifier, trapdoor) = {
        let record = commitments
            .get_mut(&commitment)
            .ok_or_else(|| "selected commitment disappeared".to_string())?;
        record.2 = true;
        (record.0, record.1)
    };

    write_commitments(&commitments)?;
    print_credentials(nullifier, trapdoor);
    Ok(())
}

/// Load the commitments map from the DarkIRC genesis identity file.
fn load_commitments() -> Result<Commitments, String> {
    let bytes = fs::read(COMMITMENTS_PATH)
        .map_err(|err| format!("failed to read `{COMMITMENTS_PATH}`: {err}"))?;
    deserialize(&bytes).map_err(|err| format!("failed to deserialize `{COMMITMENTS_PATH}`: {err}"))
}

/// Write the commitments map back to the DarkIRC genesis identity file.
fn write_commitments(commitments: &Commitments) -> Result<(), String> {
    fs::write(COMMITMENTS_PATH, serialize(commitments))
        .map_err(|err| format!("failed to write `{COMMITMENTS_PATH}`: {err}"))
}

/// Print the fields needed by DarkIRC NickServ registration.
fn print_credentials(nullifier: Secret, trapdoor: Secret) {
    let nullifier = bs58::encode(nullifier).into_string();
    let trapdoor = bs58::encode(trapdoor).into_string();

    println!("nullifier = {nullifier}");
    println!("trapdoor = {trapdoor}");
    println!("user_msg_limit = {GENESIS_USER_MSG_LIMIT}");
    println!();
    println!(
        "/msg NickServ REGISTER <account_name> {nullifier} {trapdoor} {GENESIS_USER_MSG_LIMIT}"
    );
}

/// Print command usage.
fn usage() {
    println!("Usage: rln-fau <reset|produce>");
}
