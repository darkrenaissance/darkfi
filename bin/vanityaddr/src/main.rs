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
    process::{exit, ExitCode},
    sync::{mpsc::channel, Arc},
    thread::available_parallelism,
};

use arg::Args;
use darkfi::{util::cli::ProgressInc, ANSI_LOGO};
use darkfi_money_contract::{model::TokenId, MoneyFunction};
use darkfi_sdk::crypto::{
    contract_id::MONEY_CONTRACT_ID,
    keypair::{Address, Network, StandardAddress},
    poseidon_hash, BaseBlind, ContractId, FuncRef, PublicKey, SecretKey,
};
use rand::rngs::OsRng;
use rayon::iter::ParallelIterator;

const ABOUT: &str =
    concat!("vanityaddr ", env!("CARGO_PKG_VERSION"), '\n', env!("CARGO_PKG_DESCRIPTION"));

const USAGE: &str = r#"
Usage: vanityaddr [OPTIONS] <PREFIX> <PREFIX> ...

Arguments:
  <PREFIX>    Prefixes to search

Options:
  -c             Make the search case-sensitive
  -t             Number of threads to use (defaults to number of available CPUs)
  -A             Search for an address
  -C             Search for a Contract ID
  -T             Search for a Token ID
  -n <network>   Network to search (mainnet/testnet, default=mainnet)
"#;

fn usage() {
    print!("{ANSI_LOGO}{ABOUT}\n{USAGE}");
}

struct DrkAddr {
    pub address: Address,
    pub _public: PublicKey,
    pub secret: SecretKey,
}

struct DrkToken {
    pub token_id: TokenId,
    pub secret: SecretKey,
    pub blind: BaseBlind,
}

struct DrkContract {
    pub contract_id: ContractId,
    pub secret: SecretKey,
}

trait Prefixable {
    fn new(network: Network) -> Self;
    fn to_string(&self) -> String;
    fn _get_secret(&self) -> SecretKey;

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
    fn new(network: Network) -> Self {
        let secret = SecretKey::random(&mut OsRng);
        let public = PublicKey::from_secret(secret);
        let address = StandardAddress::from_public(network, public).into();
        Self { address, _public: public, secret }
    }

    fn to_string(&self) -> String {
        let mut a = self.address.to_string();
        a.remove(0);
        a.to_string()
    }

    fn _get_secret(&self) -> SecretKey {
        self.secret
    }
}

impl Prefixable for DrkToken {
    fn new(_network: Network) -> Self {
        // Generate the mint authority secret key and blind
        let secret = SecretKey::random(&mut OsRng);
        let blind = BaseBlind::random(&mut OsRng);

        // Create the Auth FuncID
        let func_id = FuncRef {
            contract_id: *MONEY_CONTRACT_ID,
            func_code: MoneyFunction::AuthTokenMintV1 as u8,
        }
        .to_func_id();

        // Grab the mint authority user data
        let (auth_x, auth_y) = PublicKey::from_secret(secret).xy();
        let user_data = poseidon_hash([auth_x, auth_y]);

        // Derive the Token ID
        let token_id = TokenId::derive_from(func_id.inner(), user_data, blind.inner());

        Self { token_id, secret, blind }
    }

    fn to_string(&self) -> String {
        self.token_id.to_string()
    }

    fn _get_secret(&self) -> SecretKey {
        self.secret
    }
}

impl Prefixable for DrkContract {
    fn new(_network: Network) -> Self {
        let secret = SecretKey::random(&mut OsRng);
        let contract_id = ContractId::derive(secret);
        Self { contract_id, secret }
    }

    fn to_string(&self) -> String {
        self.contract_id.to_string()
    }

    fn _get_secret(&self) -> SecretKey {
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
    let mut nflag = false;
    let mut nvalue = "mainnet".to_string();

    let mut n_threads = available_parallelism().unwrap().get();

    {
        let mut args = Args::new().with_cb(|args, flag| match flag {
            'c' => cflag = true,
            'A' => addrflag = true,
            'T' => toknflag = true,
            'C' => ctrcflag = true,
            't' => n_threads = args.eargf().parse::<usize>().unwrap(),
            'n' => {
                nflag = true;
                nvalue = args.eargf().to_string();
            }
            _ => hflag = true,
        });

        argv = args.parse();
    }

    if hflag || argv.is_empty() {
        usage();
        return ExitCode::FAILURE
    }

    let network = match nvalue.as_str() {
        "mainnet" => Network::Mainnet,
        "testnet" => Network::Testnet,
        _ => {
            eprintln!("Invalid network. Use 'testnet' or 'mainnet'.");
            return ExitCode::FAILURE
        }
    };

    if (addrflag as u8 + toknflag as u8 + ctrcflag as u8) != 1 {
        eprintln!("The search flags are mutually exclusive. Use only one of -A/-C/-T.");
        return ExitCode::FAILURE
    }

    // Validate search prefixes
    for (idx, prefix) in argv.iter().enumerate() {
        match bs58::decode(prefix).into_vec() {
            Ok(_) => {}
            Err(e) => {
                eprintln!("Error: Invalid base58 for prefix #{idx}: {e}");
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
                .map(|create| create(network))
                .find_any(|address| address.starts_with_any(&argv, cflag))
                .expect("Failed to find an address match");

            // The above will keep running until it finds a match or until
            // the program terminates. Only if a match is found shall the
            // following code be executed and the program exit successfully:
            let attempts = progress_.position();
            progress_.finish_and_clear();

            println!(
                "{{\"address\":\"{}\",\"attempts\":{attempts},\"secret\":\"{}\"}}",
                addr.address, addr.secret,
            );
        }

        if toknflag {
            let tid = rayon::iter::repeat(DrkToken::new)
                .inspect(|_| progress_.inc(1))
                .map(|create| create(network))
                .find_any(|token_id| token_id.starts_with_any(&argv, cflag))
                .expect("Failed to find a token ID match");

            let attempts = progress_.position();
            progress_.finish_and_clear();

            println!(
                "{{\"token_id\":\"{}\",\"attempts\":{attempts},\"secret\":\"{}\",\"blind\":\"{}\"}}",
                tid.token_id, tid.secret, tid.blind
            );
        }

        if ctrcflag {
            let cid = rayon::iter::repeat(DrkContract::new)
                .inspect(|_| progress_.inc(1))
                .map(|create| create(network))
                .find_any(|contract_id| contract_id.starts_with_any(&argv, cflag))
                .expect("Failed to find a contract ID match");

            let attempts = progress_.position();
            progress_.finish_and_clear();

            println!(
                "{{\"contract_id\":\"{attempts}\",\"attempts\":{},\"secret\":\"{}\"}}",
                cid.contract_id, cid.secret,
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
