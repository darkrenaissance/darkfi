use std::process::exit;

use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use rand::rngs::OsRng;
use rayon::prelude::*;
use serde_json::json;

use darkfi::crypto::{
    address::Address,
    keypair::{Keypair, SecretKey},
};

#[derive(Parser)]
#[clap(name = "vanityaddr", about, version)]
#[clap(arg_required_else_help(true))]
struct Args {
    /// Prefixes to search (must start with 1)
    prefix: Vec<String>,

    /// Should the search be case-sensitive
    #[clap(short)]
    case_sensitive: bool,

    /// Number of threads to use (defaults to number of available CPUs)
    #[clap(short, parse(try_from_str))]
    threads: Option<usize>,
}

struct DrkAddr {
    pub address: String,
    pub secret: SecretKey,
}

impl DrkAddr {
    pub fn new() -> Self {
        let kp = Keypair::random(&mut OsRng);
        let addr = Address::from(kp.public);

        Self { secret: kp.secret, address: format!("{}", addr) }
    }

    pub fn starts_with(&self, prefix: &str, case_sensitive: bool) -> bool {
        if case_sensitive {
            self.address.starts_with(prefix)
        } else {
            self.address.to_lowercase().starts_with(prefix.to_lowercase().as_str())
        }
    }

    pub fn starts_with_any(&self, prefixes: &[String], case_sensitive: bool) -> bool {
        for prefix in prefixes {
            if self.starts_with(prefix, case_sensitive) {
                return true
            }
        }
        false
    }
}

fn main() {
    let args = Args::parse();

    if args.prefix.is_empty() {
        eprintln!("Error: No prefix given to search.");
        exit(1);
    }

    for (idx, prefix) in args.prefix.iter().enumerate() {
        if !prefix.starts_with('1') {
            eprintln!("Error: Address prefix at index {} must start with \"1\".", idx);
            exit(1);
        }
    }

    // Check if prefixes are valid base58
    for (idx, prefix) in args.prefix.iter().enumerate() {
        match bs58::decode(prefix).into_vec() {
            Ok(_) => {}
            Err(e) => {
                eprintln!("Error: Invalid base58 for prefix {}: {}", idx, e);
                exit(1);
            }
        };
    }

    // Threadpool
    let num_threads = if args.threads.is_some() { args.threads.unwrap() } else { num_cpus::get() };
    let rayon_pool = rayon::ThreadPoolBuilder::new().num_threads(num_threads).build().unwrap();

    // Something fancy
    let progress = ProgressBar::new_spinner();
    progress.set_style(ProgressStyle::default_bar().template("[{elapsed_precise}] {pos} attempts"));
    progress.set_draw_rate(10);

    // Fire off the threadpool
    let addr = rayon_pool.install(|| {
        rayon::iter::repeat(DrkAddr::new)
            .inspect(|_| progress.inc(1))
            .map(|create| create())
            .find_any(|address| address.starts_with_any(&args.prefix, args.case_sensitive))
            .expect("Failed to find an address match")
    });

    let attempts = progress.position();
    progress.finish_and_clear();

    let result = json!({
        "address": addr.address,
        "secret": format!("{:?}", addr.secret.0),
        "attempts": attempts,
    });

    println!("{}", result);
}
