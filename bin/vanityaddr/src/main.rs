use clap::Parser;
use darkfi::{
    crypto::{
        address::Address,
        keypair::{Keypair, SecretKey},
    },
    Error, Result,
};
use rand::rngs::OsRng;
use rayon::prelude::*;
use serde_json::json;

#[derive(Parser)]
#[clap(version)]
struct Args {
    /// Prefix to search (must start with 1)
    prefix: String,
}

struct DrkAddr {
    pub secret: SecretKey,
    pub address: String,
}

impl DrkAddr {
    pub fn new() -> Self {
        let kp = Keypair::random(&mut OsRng);
        let addr = Address::from(kp.public);

        Self { secret: kp.secret, address: format!("{}", addr) }
    }

    pub fn starts_with(&self, prefix: &str, is_case_sensitive: bool) -> bool {
        if is_case_sensitive {
            self.address.starts_with(prefix)
        } else {
            self.address.to_lowercase().starts_with(prefix.to_lowercase().as_str())
        }
    }
}

fn main() -> Result<()> {
    let args = Args::parse();

    if !args.prefix.starts_with('1') {
        return Err(Error::ParseFailed("Address prefix must start with '1'"))
    }

    let is_case_sensitive = false;

    // Check if prefix is valid base58
    match bs58::decode(args.prefix.clone()).into_vec() {
        Ok(_) => {}
        Err(_) => return Err(Error::ParseFailed("Invalid base58 for prefix")),
    };

    // Threadpool
    let num_threads = num_cpus::get();
    let rayon_pool = rayon::ThreadPoolBuilder::new()
        .num_threads(num_threads)
        .build()
        .expect("Unable to create threadpool");

    let drkaddr: DrkAddr = rayon_pool.install(|| {
        rayon::iter::repeat(DrkAddr::new)
            .map(|create| create())
            .find_any(|address| address.starts_with(&args.prefix, is_case_sensitive))
            .expect("Failed to find an address match")
    });

    let result = json!({
        "secret_key": format!("{:?}", drkaddr.secret.0),
        "address": drkaddr.address,
    });

    println!("{}", result);

    Ok(())
}
