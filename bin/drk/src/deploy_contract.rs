use std::{
    env::set_current_dir,
    fs::{read, read_dir, read_to_string, File},
    io::Write,
    path::Path,
    process::exit,
    str::FromStr,
};

use pasta_curves::{arithmetic::CurveAffine, group::Curve, pallas};
use rand::RngCore;

use darkfi::{
    crypto::{
        keypair::{PublicKey, SecretKey},
        util::poseidon_hash,
    },
    runtime::vm_runtime::{Runtime, ENTRYPOINT},
    zkas::ZkBinary,
    Result,
};

// TODO: Move some of this generic stuff into the library

const DEPLOY_KEY_NAME: &str = "deploy.key";
const CIRCUIT_DIR_NAME: &str = "proof";
const CONTRACT_FILE_NAME: &str = "contract.wasm";

pub struct ContractDeploy {
    /// Secret key used for deploy authorization
    pub deploy_key: SecretKey,
    /// Public address of the contract, derived from the deploy key
    pub public: pallas::Base,
    /// Compiled smart contract wasm binary to be executed in the wasm vm runtime
    pub binary: Vec<u8>,
    /// Compiled zkas circuits used by the smart contract provers and verifiers
    pub circuits: Vec<Vec<u8>>,
}

/// Creates a new deploy key for deploying a private smart contract.
/// This key allows to update the wasm code on the blockchain by creating
/// a signature. When deployed, the contract can be accessed by requesting
/// the public counterpart of this secret key.
fn create_deploy_key(mut rng: impl RngCore, path: &Path) -> Result<()> {
    eprintln!("Creating a deploy key");
    let secret = SecretKey::random(&mut rng);
    let mut file = File::create(path)?;
    file.write_all(&bs58::encode(&secret.to_bytes()).into_string().as_bytes())?;
    eprintln!("Written deploy key to {}", path.display());
    Ok(())
}

/// Reads a deploy key from a file on the filesystem, and returns it,
/// along with its public counterpart.
/// TODO: Make a type for the public counterpart.
fn read_deploy_key(s: &str) -> Result<(SecretKey, pallas::Base)> {
    eprintln!("Reading deploy key from file: {}", s);
    let contents = read_to_string(s)?;
    let secret = SecretKey::from_str(&contents)?;
    let coords = PublicKey::from_secret(secret).0.to_affine().coordinates().unwrap();
    let public = poseidon_hash::<2>([*coords.x(), *coords.y()]);
    Ok((secret, public))
}

/// Deploys a given compiled smart contract on the network.
/// TODO: Implement storage/tx fees in ZK, linear to the size of the binary.
/// For consistency, we point this function to a directory where our smart
/// contract and the compiled circuits are contained. This gives us a uniform
/// approach to scm and gives a generic layout of a smart contract repository:
///
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
pub fn deploy_contract(path: &Path) -> Result<ContractDeploy> {
    // chdir into the contract directory
    if let Err(e) = set_current_dir(path) {
        eprintln!("Error changing directory to {}: {}", path.display(), e);
        exit(1);
    }

    let deploy_key = match read_deploy_key(DEPLOY_KEY_NAME) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error: Failed to read {}: {}", DEPLOY_KEY_NAME, e);
            exit(1);
        }
    };

    // Validate compiled circuits. Looks for files ending with `.zk.bin`.
    eprintln!("Validating compiled circuits in {}/", CIRCUIT_DIR_NAME);
    let mut circuits = vec![];
    let dir_iter = read_dir(CIRCUIT_DIR_NAME)?;
    for i in dir_iter {
        if let Err(e) = i {
            eprintln!("Error iterating over directory: {}", e);
            exit(1);
        }

        let f = i.unwrap();
        let fname = f.file_name();
        let fname = fname.to_str().unwrap();

        if fname.ends_with(".zk.bin") {
            // Validate that it can be decoded
            eprintln!("Found {}", f.path().display());
            let buf = read(f.path())?;
            if let Err(e) = ZkBinary::decode(&buf) {
                eprintln!("Error decoding zkas bincode in {}: {}", f.path().display(), e);
                exit(1);
            }

            eprintln!("{} is a valid zkas circuit", f.path().display());
            circuits.push(buf.clone());
        }
    }

    // Validate wasm binary.
    eprintln!("Reading wasm binary in {}", CONTRACT_FILE_NAME);
    let wasm_bytes = read(CONTRACT_FILE_NAME)?;
    eprintln!("Initializing mock wasm runtime to check validity");
    let runtime = match Runtime::new(&wasm_bytes) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error: Failed to initialize wasm runtime: {}", e);
            exit(1);
        }
    };

    eprintln!("Looking for entrypoint function");
    if let Err(e) = runtime.instance.exports.get_function(ENTRYPOINT) {
        eprintln!("Error: Did not find entrypoint function in the wasm: {}", e);
        exit(1);
    }

    let cd = ContractDeploy {
        deploy_key: deploy_key.0,
        public: deploy_key.1,
        binary: wasm_bytes,
        circuits,
    };

    Ok(cd)
}
