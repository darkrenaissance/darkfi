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
    process::{exit, ExitCode},
    sync::{mpsc::channel, Arc},
    thread::available_parallelism,
};

use arg::Args;
use darkfi::{util::cli::ProgressInc, ANSI_LOGO};
use darkfi_sdk::crypto::{ContractId, PublicKey, SecretKey, TokenId};
use rand::rngs::OsRng;
use rayon::iter::ParallelIterator;

const ABOUT: &str =
    concat!("vanityaddr ", env!("CARGO_PKG_VERSION"), '\n', env!("CARGO_PKG_DESCRIPTION"));

const USAGE: &str = r#"
Usage: vanityaddr [OPTIONS] <PREFIX> <PREFIX> ...

Arguments:
  <PREFIX>    Prefixes to search

Options:
  -c    Make the search case-sensitive
  -t    Number of threads to use (defaults to number of available CPUs)
  -A    Search for an address
  -C    Search for a Contract ID
  -T    Search for a Token ID
"#;

fn usage() {
    print!("{}{}\n{}", ANSI_LOGO, ABOUT, USAGE);
}

struct DrkAddr {
    pub public: PublicKey,
    pub secret: SecretKey,
}

struct DrkToken {
    pub token_id: TokenId,
    pub secret: SecretKey,
}

struct DrkContract {
    pub contract_id: ContractId,
    pub secret: SecretKey,
}

trait Prefixable {
    fn new() -> Self;
    fn to_string(&self) -> String;
    fn get_secret(&self) -> SecretKey;

    fn starts_with(&self, prefix: &str, case_sensitive: bool) -> bool {
        if case_sensitive {
            self.to_string().starts_with(prefix)
        } else {
            self.to_string().to_lowercase().starts_with(prefix.to_lowercase().as_str())
        }
    }

    fn starts_with_any(&self, prefixes: &[String], case_sensitive: bool) -> bool {
        prefixes.iter().any(|prefix| self.starts_with(prefix, case_sensitive))
    }
}

impl Prefixable for DrkAddr {
    fn new() -> Self {
        let secret = SecretKey::random(&mut OsRng);
        let public = PublicKey::from_secret(secret);
        Self { public, secret }
    }

    fn to_string(&self) -> String {
        self.public.to_string()
    }

    fn get_secret(&self) -> SecretKey {
        self.secret
    }
}

impl Prefixable for DrkToken {
    fn new() -> Self {
        let secret = SecretKey::random(&mut OsRng);
        let token_id = TokenId::derive(secret);
        Self { token_id, secret }
    }

    fn to_string(&self) -> String {
        self.token_id.to_string()
    }

    fn get_secret(&self) -> SecretKey {
        self.secret
    }
}

impl Prefixable for DrkContract {
    fn new() -> Self {
        let secret = SecretKey::random(&mut OsRng);
        let contract_id = ContractId::derive(secret);
        Self { contract_id, secret }
    }

    fn to_string(&self) -> String {
        self.contract_id.to_string()
    }

    fn get_secret(&self) -> SecretKey {
        self.secret
    }
}

fn main() -> ExitCode {
    let argv;
    let mut hflag = false;
    let mut cflag = false;
    let mut addrflag = false;
    let mut toknflag = false;
    let mut ctrcflag = false;

    let mut n_threads = available_parallelism().unwrap().get();

    {
        let mut args = Args::new().with_cb(|args, flag| match flag {
            'c' => cflag = true,
            'A' => addrflag = true,
            'T' => toknflag = true,
            'C' => ctrcflag = true,
            't' => n_threads = args.eargf().parse::<usize>().unwrap(),
            _ => hflag = true,
        });

        argv = args.parse();
    }

    if hflag || argv.is_empty() {
        usage();
        return ExitCode::FAILURE
    }

    if (addrflag as u8 + toknflag as u8 + ctrcflag as u8) != 1 {
        eprintln!("The search flags are mutually exclusive. Use only one of -A/-C/-T.");
        return ExitCode::FAILURE
    }

    // Validate search prefixes
    for (idx, prefix) in argv.iter().enumerate() {
        match bs58::decode(prefix).into_vec() {
            Ok(_) => {}
            Err(e) => {
                eprintln!("Error: Invalid base58 for prefix #{}: {}", idx, e);
                return ExitCode::FAILURE
            }
        }
    }

    // Handle SIGINT
    let (tx, rx) = channel();
    ctrlc::set_handler(move || tx.send(()).expect("Could not send signal on channel"))
        .expect("Error setting SIGINT handler");

    // Something fancy
    let progress = Arc::new(ProgressInc::new());

    // Threadpool
    let progress_ = progress.clone();
    let rayon_pool = rayon::ThreadPoolBuilder::new().num_threads(n_threads).build().unwrap();
    rayon_pool.spawn(move || {
        if addrflag {
            let addr = rayon::iter::repeat(DrkAddr::new)
                .inspect(|_| progress_.inc(1))
                .map(|create| create())
                .find_any(|address| address.starts_with_any(&argv, cflag))
                .expect("Failed to find an address match");

            // The above will keep running until it finds a match or until
            // the program terminates. Only if a match is found shall the
            // following code be executed and the program exit successfully:
            let attempts = progress_.position();
            progress_.finish_and_clear();

            println!(
                "{{\"address\":\"{}\",\"attempts\":{},\"secret\":\"{}\"}}",
                addr.public, attempts, addr.secret,
            );
        }

        if toknflag {
            let tid = rayon::iter::repeat(DrkToken::new)
                .inspect(|_| progress_.inc(1))
                .map(|create| create())
                .find_any(|token_id| token_id.starts_with_any(&argv, cflag))
                .expect("Failed to find a token ID match");

            let attempts = progress_.position();
            progress_.finish_and_clear();

            println!(
                "{{\"token_id\":\"{}\",\"attempts\":{},\"secret\":\"{}\"}}",
                tid.token_id, attempts, tid.secret,
            );
        }

        if ctrcflag {
            let cid = rayon::iter::repeat(DrkContract::new)
                .inspect(|_| progress_.inc(1))
                .map(|create| create())
                .find_any(|contract_id| contract_id.starts_with_any(&argv, cflag))
                .expect("Failed to find a contract ID match");

            let attempts = progress_.position();
            progress_.finish_and_clear();

            println!(
                "{{\"contract_id\":\"{}\",\"attempts\":{},\"secret\":\"{}\"}}",
                cid.contract_id, attempts, cid.secret,
            );
        }

        exit(0);
    });

    // This now blocks and lets our threadpool execute in the background.
    rx.recv().expect("Could not receive from channel");
    progress.finish_and_clear();
    eprintln!("\r\x1b[2KCaught SIGINT, exiting...");
    ExitCode::FAILURE
}
