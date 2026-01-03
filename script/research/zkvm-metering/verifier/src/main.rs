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

use std::{
    env,
    fs::File,
    io::{Cursor, Read},
    path::Path,
};

use darkfi::{
    Result,
    zk::{Proof, VerifyingKey, ZkCircuit, empty_witnesses},
    zkas::ZkBinary,
};
use darkfi_sdk::pasta::pallas::Base;
use darkfi_serial::deserialize;

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        println!("Usage: ./verifier [ZK_BIN_FILE_PATH]");
        return Ok(())
    }

    let zk_bin_path = Path::new(&args[1]);

    if !zk_bin_path.is_file() || !zk_bin_path.exists() || !&args[1].ends_with(".zk.bin") {
        println!("Zk bin file path does not exist or is invalid");
        return Ok(())
    }

    //Load zkbin, proof, verifying key, public inputs from file
    let mut file = File::open(zk_bin_path)?;
    let mut bincode = vec![];
    file.read_to_end(&mut bincode)?;

    let proof_path = zk_bin_path.to_str().unwrap().replace(".zk.bin", ".proof.bin");
    let mut file = File::open(proof_path)?;
    let mut proof_bin = vec![];
    file.read_to_end(&mut proof_bin)?;

    let vks_path = zk_bin_path.to_str().unwrap().replace(".zk.bin", ".vks.bin");
    let mut file = File::open(vks_path)?;
    let mut vkbin = vec![];
    file.read_to_end(&mut vkbin)?;

    let pi_path = zk_bin_path.to_str().unwrap().replace(".zk.bin", ".pi.bin");
    let mut file = File::open(pi_path)?;
    let mut public_inputs_bin = vec![];
    file.read_to_end(&mut public_inputs_bin)?;

    // Deserialize and Verify
    let zkbin = ZkBinary::decode(&bincode)?;
    let verifier_witnesses = empty_witnesses(&zkbin)?;

    // Create the circuit
    let circuit = ZkCircuit::new(verifier_witnesses, &zkbin);

    let proof: Proof = deserialize(&proof_bin)?;
    let mut vk_buf = Cursor::new(vkbin);
    let vk = VerifyingKey::read::<Cursor<Vec<u8>>, ZkCircuit>(&mut vk_buf, circuit)?;
    //let vk = VerifyingKey::build(zkbin.k, &circuit);

    let public_inputs: Vec<Base> = deserialize(&public_inputs_bin)?;

    Ok(proof.verify(&vk, &public_inputs)?)
}
