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

use std::{process::exit, sync::mpsc::channel};

use clap::Parser;
use darkfi_sdk::crypto::{Keypair, SecretKey};
use indicatif::{ProgressBar, ProgressStyle};
use rand::rngs::OsRng;
use rayon::prelude::*;

use darkfi::cli_desc;

#[derive(Parser)]
#[clap(name = "vanityaddr", about = cli_desc!(), version)]
#[clap(arg_required_else_help(true))]
struct Args {
    /// Prefixes to search
    prefix: Vec<String>,

    /// Should the search be case-sensitive
    #[clap(short)]
    case_sensitive: bool,

    /// Number of threads to use (defaults to number of available CPUs)
    #[clap(short)]
    threads: Option<usize>,
}

struct DrkAddr {
    pub address: String,
    pub secret: SecretKey,
}

impl DrkAddr {
    pub fn new() -> Self {
        let kp = Keypair::random(&mut OsRng);

        Self { secret: kp.secret, address: format!("{}", kp.public) }
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

    // Handle SIGINT
    let (tx, rx) = channel();
    ctrlc::set_handler(move || tx.send(()).expect("Could not send signal on channel"))
        .expect("Error setting SIGINT handler");

    // Something fancy
    let progress = ProgressBar::new_spinner();
    let template =
        ProgressStyle::default_bar().template("[{elapsed_precise}] {pos} attempts").unwrap();
    progress.set_style(template);

    // Fire off the threadpool
    rayon_pool.spawn(move || {
        let addr = rayon::iter::repeat(DrkAddr::new)
            .inspect(|_| progress.inc(1))
            .map(|create| create())
            .find_any(|address| address.starts_with_any(&args.prefix, args.case_sensitive))
            .expect("Failed to find an address match");

        // The above will keep running until it finds a match or until the
        // program terminates. Only if a match is found shall the following
        // code be executed and the program exit successfully:
        let attempts = progress.position();
        progress.finish_and_clear();

        println!(
            "{{\"address\":\"{}\",\"attempts\":{},\"secret\":\"{:?}\"}}",
            addr.address,
            attempts,
            addr.secret.inner()
        );

        exit(0);
    });

    // This now blocks and lets our threadpool execute in the background.
    rx.recv().expect("Could not receive from channel");
    eprintln!("\rCaught SIGINT, exiting...");
    exit(127);
}
