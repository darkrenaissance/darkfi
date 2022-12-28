/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
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
    env::set_current_dir,
    fs::{read, read_dir, read_to_string, File},
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
    str::FromStr,
};

use rand::{rngs::OsRng, RngCore};

use darkfi::{
    crypto::keypair::SecretKey,
    node::{MemoryState, State},
    runtime::vm_runtime::Runtime,
    util::cli::{fg_green, fg_red},
    zkas::ZkBinary,
    Error, Result,
};

const CIRCUIT_DIR_NAME: &str = "proof";
const CONTRACT_FILE_NAME: &str = "contract.wasm";
const DEPLOY_KEY_NAME: &str = "deploy.key";

/// Creates a new deploy key used for deploying private smart contracts.
/// This key allows to update the wasm code and the zk circuits on chain
/// by creating a signature. When deployed, the contract can be accessed
/// by requesting the public counterpart of this secret key.
pub fn create_deploy_key(mut rng: impl RngCore, path: &Path) -> Result<SecretKey> {
    let secret = SecretKey::random(&mut rng);
    let mut file = File::create(path)?;
    file.write_all(bs58::encode(&secret.to_bytes()).into_string().as_bytes())?;
    Ok(secret)
}

/// Reads a deploy key from a file on the filesystem and returns it.
fn read_deploy_key(s: &Path) -> core::result::Result<SecretKey, std::io::Error> {
    eprintln!("Trying to read deploy key from file: {:?}", s);
    let contents = read_to_string(s)?;
    let secret = SecretKey::from_str(&contents).unwrap();
    Ok(secret)
}

/// Creates necessary data to deploy a given smart contract on the network.
/// For consistency, we point this function to a directory where our smart
/// contract and the compiled circuits are contained. This is going to give
/// us a uniform approach to scm and gives a generic layout of the source:
/// ```text
/// smart-contract
/// ├── Cargo.toml
/// ├── deploy.key
/// ├── Makefile
/// ├── proof
/// │   ├── circuit0.zk
/// │   ├── circuit0.zk.bin
/// │   ├── circuit1.zk
/// │   └── circuit1.zk.bin
/// ├── contract.wasm
/// ├── src
/// │   └── lib.rs
/// └── tests
/// ```
//pub fn create_deploy_data(path: &Path) -> Result<ContractDeploy> {
pub fn create_deploy_data(path: &Path) -> Result<()> {
    // Try to chdir into the contract directory
    if let Err(e) = set_current_dir(path) {
        eprintln!("Failed to chdir into {:?}", path);
        return Err(e.into())
    }

    let deploy_key: SecretKey;

    let deploy_key = match read_deploy_key(&PathBuf::from(DEPLOY_KEY_NAME)) {
        Ok(v) => deploy_key = v,
        Err(e) => {
            if e.kind() == ErrorKind::NotFound {
                // We didn't find a deploy key, generate a new one.
                eprintln!("Did not find an existing key, creating a new one.");
                match create_deploy_key(&mut OsRng, &PathBuf::from(DEPLOY_KEY_NAME)) {
                    Ok(v) => {
                        eprintln!("Created new deploy key in \"{}\".", DEPLOY_KEY_NAME);
                        deploy_key = v;
                    }
                    Err(e) => {
                        eprintln!("Failed to create new deploy key");
                        return Err(e)
                    }
                }
            }
            eprintln!("Failed to read deploy key");
            return Err(e.into())
        }
    };

    // Search for ZK circuits in the directory. If none are found, we'll bail.
    // The logic searches for `.zk.bin` files created by zkas.
    eprintln!("Searching for compiled ZK circuits in \"{}\" ...", CIRCUIT_DIR_NAME);
    let mut circuits = vec![];
    for i in read_dir(CIRCUIT_DIR_NAME)? {
        if let Err(e) = i {
            eprintln!("Error iterating over \"{}\" directory", CIRCUIT_DIR_NAME);
            return Err(e.into())
        }

        let f = i.unwrap();
        let fname = f.file_name();
        let fname = fname.to_str().unwrap();

        if fname.ends_with(".zk.bin") {
            // Validate that the files can be properly decoded
            eprintln!("{} {}", fg_green("Found:"), f.path().display());
            let buf = read(f.path())?;
            if let Err(e) = ZkBinary::decode(&buf) {
                eprintln!("{} Failed to decode zkas bincode in {:?}", fg_red("Error:"), f.path());
                return Err(e)
            }

            circuits.push(buf.clone());
        }
    }

    if circuits.is_empty() {
        return Err(Error::Custom("Found no valid ZK circuits".to_string()))
    }

    /* FIXME
    // Validate wasm binary. We inspect the bincode and try to load it into
    // the wasm runtime. If loaded, we then look for the `ENTRYPOINT` function
    // which we hardcode into our sdk and runtime and is the canonical way to
    // run wasm binaries on chain.
    eprintln!("Inspecting wasm binary in \"{}\"", CONTRACT_FILE_NAME);
    let wasm_bytes = read(CONTRACT_FILE_NAME)?;
    eprintln!("Initializing moch wasm runtime to check validity");
    let runtime = match Runtime::new(&wasm_bytes, MemoryState::new(State::dummy()?)) {
        Ok(v) => {
            eprintln!("Found {} wasm binary", fg_green("valid"));
            v
        }
        Err(e) => {
            eprintln!("Failed to initialize wasm runtime");
            return Err(e)
        }
    };

    eprintln!("Looking for entrypoint function inside the wasm");
    let cs = ContractSection::Exec;
    if let Err(e) = runtime.instance.exports.get_function(cs.name()) {
        eprintln!("{} Could not find entrypoint function", fg_red("Error:"));
        return Err(e.into())
    }

    // TODO: Create a ZK proof enforcing the deploy key relations with their public
    // counterparts (public key and contract address)
    let mut total_bytes = 0;
    total_bytes += wasm_bytes.len();
    for circuit in circuits {
        total_bytes += circuit.len();
    }
    */

    // TODO: Return the data back to the main function, and work further in creating
    // a transaction and broadcasting it.
    Ok(())
}
