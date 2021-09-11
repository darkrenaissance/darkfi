//use super::cli_config::DrkCliConfig;
use crate::Result;

use blake2b_simd::Params;
use clap::{App, Arg};
use serde::Deserialize;

use crate::serial;
use std::path::PathBuf;

fn amount_f64(v: String) -> std::result::Result<(), String> {
    if v.parse::<f64>().is_ok() {
        Ok(())
    } else {
        Err(String::from("The value is not an integer of type u64"))
    }
}

#[derive(Deserialize, Debug)]
pub struct TransferParams {
    pub asset: Asset,
    pub pub_key: String,
    pub amount: f64,
}

impl TransferParams {
    pub fn new() -> Self {
        Self {
            asset: Asset::new(),
            pub_key: String::new(),
            amount: 0.0,
        }
    }
}

pub struct Deposit {
    pub asset: Asset,
}

impl Deposit {
    pub fn new() -> Self {
        Self {
            asset: Asset::new(),
        }
    }
}

#[derive(Deserialize, Debug)]
pub struct WithdrawParams {
    pub asset: Asset,
    pub pub_key: String,
    pub amount: f64,
}

impl WithdrawParams {
    pub fn new() -> Self {
        Self {
            asset: Asset::new(),
            pub_key: String::new(),
            amount: 0.0,
        }
    }
}

#[derive(Deserialize, Debug)]
pub struct Asset {
    pub ticker: String,
    pub id: Vec<u8>,
}

impl Asset {
    pub fn new() -> Self {
        Self {
            ticker: String::new(),
            id: Vec::new(),
        }
    }
    pub fn id_hash(&self, ticker: &String) -> Result<Vec<u8>> {
        let mut hasher = Params::new().hash_length(64).to_state();
        hasher.update(ticker.as_bytes());
        let result = hasher.finalize();
        let scalar = jubjub::Fr::from_bytes_wide(result.as_array());
        let id = serial::serialize(&scalar);
        Ok(id)
    }
}

pub struct DrkCli {
    pub verbose: bool,
    pub wallet: bool,
    pub key: bool,
    pub get_key: bool,
    pub info: bool,
    pub hello: bool,
    pub stop: bool,
    pub transfer: Option<TransferParams>,
    pub deposit: Option<Deposit>,
    pub withdraw: Option<WithdrawParams>,
    pub config: Box<Option<PathBuf>>,
}

impl DrkCli {
    pub fn load() -> Result<Self> {
        let app = App::new("Drk CLI")
            .version("0.1.0")
            .author("Dark Renaissance Technologies")
            .about("Run Drk Client")
            .arg(
                Arg::with_name("verbose")
                    .short("v")
                    .help("Increase verbosity")
                    .long("verbose")
                    .takes_value(false),
            )
            .arg(
                Arg::with_name("hello")
                    .long("hello")
                    .help("Say hello")
                    .takes_value(false),
            )
            .arg(
                Arg::with_name("key")
                    .short("k")
                    .long("key")
                    .help("Generate new keypair")
                    .takes_value(false),
            )
            .arg(
                Arg::with_name("getkey")
                    .long("getkey")
                    .help("Get public key as base-58 encoded string")
                    .takes_value(false),
            )
            .arg(
                Arg::with_name("wallet")
                    .short("w")
                    .long("wallet")
                    .help("Create a new wallet")
                    .takes_value(false),
            )
            .arg(
                Arg::with_name("info")
                    .short("i")
                    .long("info")
                    .help("Request info from daemon")
                    .takes_value(false),
            )
            .arg(
                Arg::with_name("stop")
                    .short("s")
                    .long("stop")
                    .help("Send a stop signal to the daemon")
                    .takes_value(false),
            )
            .arg(
                Arg::with_name("config")
                    .help("Path for config file")
                    .long("config")
                    .takes_value(true),
            )
            .subcommand(
                App::new("transfer")
                    .about("Transfer dark assets between users")
                    .arg(
                        Arg::with_name("asset")
                            .value_name("ASSET_TYPE")
                            .takes_value(true)
                            .index(1)
                            .help("Desired asset type")
                            .required(true),
                    )
                    .arg(
                        Arg::with_name("address")
                            .value_name("RECEIVE_ADDRESS")
                            .takes_value(true)
                            .index(2)
                            .help("Address of recipient")
                            .required(true),
                    )
                    .arg(
                        Arg::with_name("amount")
                            .value_name("AMOUNT")
                            .takes_value(true)
                            .index(3)
                            .validator(amount_f64)
                            .help("Amount to send")
                            .required(true),
                    ),
            )
            .subcommand(
                App::new("deposit")
                    .about("Deposit clear assets for dark assets")
                    .arg(
                        Arg::with_name("asset")
                            .value_name("ASSET_TYPE")
                            .takes_value(true)
                            .index(1)
                            .help("Desired asset type")
                            .required(true),
                    ),
            )
            .subcommand(
                App::new("withdraw")
                    .about("Withdraw dark assets for clear assets")
                    .arg(
                        Arg::with_name("asset")
                            .value_name("ASSET_TYPE")
                            .takes_value(true)
                            .index(1)
                            .help("Desired asset type")
                            .required(true),
                    )
                    .arg(
                        Arg::with_name("address")
                            .value_name("RECEIVE_ADDRESS")
                            .takes_value(true)
                            .index(2)
                            .help("Address of recipient")
                            .required(true),
                    )
                    .arg(
                        Arg::with_name("amount")
                            .value_name("AMOUNT")
                            .takes_value(true)
                            .index(3)
                            .validator(amount_f64)
                            .help("Amount to send")
                            .required(true),
                    ),
            )
            .get_matches();

        let verbose = app.is_present("verbose");
        let wallet = app.is_present("wallet");
        let key = app.is_present("key");
        let info = app.is_present("info");
        let hello = app.is_present("hello");
        let stop = app.is_present("stop");
        let get_key = app.is_present("getkey");

        let mut deposit = None;
        match app.subcommand_matches("deposit") {
            Some(deposit_sub) => {
                let mut dep = Deposit::new();
                if let Some(asset) = deposit_sub.value_of("asset") {
                    dep.asset.ticker = asset.to_string();
                    dep.asset.id = dep.asset.id_hash(&dep.asset.ticker)?;
                }
                deposit = Some(dep);
            }
            None => {}
        }

        let mut transfer = None;
        match app.subcommand_matches("transfer") {
            Some(transfer_sub) => {
                let mut trn = TransferParams::new();
                if let Some(asset) = transfer_sub.value_of("asset") {
                    trn.asset.ticker = asset.to_string();
                    trn.asset.id = trn.asset.id_hash(&trn.asset.ticker)?;
                }
                if let Some(address) = transfer_sub.value_of("address") {
                    trn.pub_key = address.to_string();
                }
                if let Some(amount) = transfer_sub.value_of("amount") {
                    trn.amount = amount.parse().expect("Convert the amount to f64");
                }
                transfer = Some(trn);
            }
            None => {}
        }

        let mut withdraw = None;
        match app.subcommand_matches("withdraw") {
            Some(withdraw_sub) => {
                let mut wdraw = WithdrawParams::new();
                if let Some(asset) = withdraw_sub.value_of("asset") {
                    wdraw.asset.ticker = asset.to_string();
                    wdraw.asset.id = wdraw.asset.id_hash(&wdraw.asset.ticker)?;
                }
                if let Some(address) = withdraw_sub.value_of("address") {
                    wdraw.pub_key = address.to_string();
                }
                if let Some(amount) = withdraw_sub.value_of("amount") {
                    wdraw.amount = amount.parse().expect("Convert the amount to f64");
                }
                withdraw = Some(wdraw);
            }
            None => {}
        }

        let config = Box::new(if let Some(config_path) = app.value_of("config") {
            Some(std::path::Path::new(config_path).to_path_buf())
        } else {
            None
        });

        Ok(Self {
            verbose,
            wallet,
            key,
            get_key,
            info,
            hello,
            stop,
            deposit,
            transfer,
            withdraw,
            config,
        })
    }
}
