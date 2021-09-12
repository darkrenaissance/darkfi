//use super::cli_config::DrkCliConfig;
use crate::Result;

use crate::serial::{deserialize, serialize};
use blake2b_simd::Params;
use clap::{App, Arg};
use serde::{Deserialize, Serialize};

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
    pub asset_id: Vec<u8>,
    pub pub_key: String,
    pub amount: f64,
}

impl TransferParams {
    pub fn new(asset_id: Vec<u8>, pub_key: String, amount: f64) -> Self {
        Self {
            asset_id,
            pub_key,
            amount,
        }
    }
}

pub struct Deposit {
    pub asset_id: Vec<u8>,
}

impl Deposit {
    pub fn new(asset_id: Vec<u8>) -> Self {
        Self { asset_id }
    }
}

#[derive(Deserialize, Debug)]
pub struct WithdrawParams {
    pub asset_id: Vec<u8>,
    pub pub_key: String,
    pub amount: f64,
}

impl WithdrawParams {
    pub fn new(asset_id: Vec<u8>, pub_key: String, amount: f64) -> Self {
        Self {
            asset_id,
            pub_key,
            amount,
        }
    }
}

#[derive(Deserialize, Serialize, Debug)]
pub struct Asset {
    pub name: String,
    pub id: Vec<u8>,
}

impl Asset {
    pub fn new(name: String) -> Self {
        let id = Self::id_hash(&name);
        Self { name, id }
    }
    pub fn id_hash(name: &String) -> Vec<u8> {
        let mut hasher = Params::new().hash_length(64).to_state();
        hasher.update(name.as_bytes());
        let result = hasher.finalize();
        let hash = jubjub::Fr::from_bytes_wide(result.as_array());
        let id = serialize(&hash);
        id
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
                let asset_value = deposit_sub.value_of("asset").unwrap();
                let asset = Asset::new(asset_value.to_string());
                let dep = Deposit::new(asset.id.clone());
                deposit = Some(dep);
                let id: jubjub::Fr = deserialize(&asset.id)?;
                println!(
                    "deposit request for asset: {}, asset ID: {:?}",
                    asset_value, id
                );
            }
            None => {}
        }

        let mut transfer = None;
        match app.subcommand_matches("transfer") {
            Some(transfer_sub) => {
                let asset_value = transfer_sub.value_of("asset").unwrap().to_string();
                let asset = Asset::new(asset_value.clone());
                let address = transfer_sub.value_of("address").unwrap().to_string();
                let amount = transfer_sub
                    .value_of("amount")
                    .unwrap()
                    .parse()
                    .expect("Convert the amount to f64");
                let trn = TransferParams::new(asset.id.clone(), address, amount);
                transfer = Some(trn);
                let id: jubjub::Fr = deserialize(&asset.id)?;
                println!(
                    "transfer request for asset: {}, amount: {}, asset ID: {:?}",
                    asset_value, amount, id
                );
            }
            None => {}
        }

        let mut withdraw = None;
        match app.subcommand_matches("withdraw") {
            Some(withdraw_sub) => {
                let asset_value = withdraw_sub.value_of("asset").unwrap().to_string();
                let asset = Asset::new(asset_value.clone());
                let address = withdraw_sub.value_of("address").unwrap().to_string();
                let amount = withdraw_sub
                    .value_of("amount")
                    .unwrap()
                    .parse()
                    .expect("Convert the amount to f64");
                let wdraw = WithdrawParams::new(asset.id.clone(), address, amount);
                withdraw = Some(wdraw);
                let id: jubjub::Fr = deserialize(&asset.id)?;
                println!(
                    "withdraw request for asset: {}, amount: {}, asset ID: {:?}",
                    asset_value, amount, id
                );
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
