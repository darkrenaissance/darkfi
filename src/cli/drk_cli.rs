//use super::cli_config::DrkCliConfig;
use crate::Result;

use clap::{App, Arg};
use serde::Deserialize;

use std::path::PathBuf;

fn amount_f64<'a>(v: &'a str) -> std::result::Result<(), String> {
    if v.parse::<f64>().is_ok() {
        Ok(())
    } else {
        Err(String::from("The value is not an integer of type u64"))
    }
}

#[derive(Deserialize, Debug)]
pub struct TransferParams {
    pub pub_key: String,
    pub amount: f64,
}

impl TransferParams {
    pub fn new() -> Self {
        Self {
            pub_key: String::new(),
            amount: 0.0,
        }
    }
}

pub struct Deposit {
    pub asset: String,
}

impl Deposit {
    pub fn new() -> Self {
        Self {
            asset: String::new(),
        }
    }
}

#[derive(Deserialize, Debug)]
pub struct WithdrawParams {
    pub pub_key: String,
    pub amount: f64,
}

impl WithdrawParams {
    pub fn new() -> Self {
        Self {
            pub_key: String::new(),
            amount: 0.0,
        }
    }
}

pub struct DrkCli {
    pub verbose: bool,
    pub cashier: bool,
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
            .author("Amir Taaki <amir@dyne.org>")
            .about("Run Drk Client")
            .arg(
                Arg::new("verbose")
                    .short('v')
                    .help_heading(Some("Increase verbosity"))
                    .long("verbose")
                    .takes_value(false),
            )
            .arg(
                Arg::new("hello")
                    .long("hello")
                    .help_heading(Some("Say hello"))
                    .takes_value(false),
            )
            .arg(
                Arg::new("cashier")
                    .short('c')
                    .long("cashier")
                    .help_heading(Some("Create a cashier wallet"))
                    .takes_value(false),
            )
            .arg(
                Arg::new("key")
                    .short('k')
                    .long("key")
                    .help_heading(Some("Test key"))
                    .takes_value(false),
            )
            .arg(
                Arg::new("getkey")
                    .long("getkey")
                    .help_heading(Some("Get public key as base-58 encoded string"))
                    .takes_value(false),
            )
            .arg(
                Arg::new("wallet")
                    .short('w')
                    .long("wallet")
                    .help_heading(Some("Create a new wallet"))
                    .takes_value(false),
            )
            .arg(
                Arg::new("info")
                    .short('i')
                    .long("info")
                    .help_heading(Some("Request info from daemon"))
                    .takes_value(false),
            )
            .arg(
                Arg::new("stop")
                    .short('s')
                    .long("stop")
                    .help_heading(Some("Send a stop signal to the daemon"))
                    .takes_value(false),
            )
            .arg(
                Arg::new("config")
                    .help_heading(Some("Path for config file"))
                    .long("config")
                    .takes_value(true),
            )
            .subcommand(
                App::new("transfer")
                    .about("Transfer DBTC between users")
                    .arg(
                        Arg::new("address")
                            .value_name("RECEIVE_ADDRESS")
                            .takes_value(true)
                            .index(1)
                            .help_heading(Some("Address of recipient"))
                            .required(true),
                    )
                    .arg(
                        Arg::new("amount")
                            .value_name("AMOUNT")
                            .takes_value(true)
                            .index(2)
                            .validator(amount_f64)
                            .help_heading(Some("Amount to send, in DBTC"))
                            .required(true),
                    ),
            )
            .subcommand(App::new("deposit").about("Deposit BTC for dBTC"))
            .subcommand(
                App::new("withdraw")
                    .about("Withdraw BTC for dBTC")
                    .arg(
                        Arg::new("address")
                            .value_name("RECEIVE_ADDRESS")
                            .takes_value(true)
                            .index(1)
                            .help_heading(Some("Address of recipient"))
                            .required(true),
                    )
                    .arg(
                        Arg::new("amount")
                            .value_name("AMOUNT")
                            .takes_value(true)
                            .index(2)
                            .validator(amount_f64)
                            .help_heading(Some("Amount to send, in BTC"))
                            .required(true),
                    ),
            )
            .subcommand(App::new("deposit").about("Deposit BTC for dBTC"))
            .get_matches();

        let verbose = app.is_present("verbose");
        let cashier = app.is_present("cashier");
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
                    dep.asset = asset.to_string();
                }
                deposit = Some(dep);
            }
            None => {}
        }

        let mut transfer = None;
        match app.subcommand_matches("transfer") {
            Some(transfer_sub) => {
                let mut trn = TransferParams::new();
                if let Some(address) = transfer_sub.value_of("address") {
                    trn.pub_key = address.to_string();
                }
                if let Some(amount) = transfer_sub.value_of("amount") {
                    trn.amount = amount.parse().expect("couldn't convert the amount to f64");
;
                }
                transfer = Some(trn);
            }
            None => {}
        }

        let mut withdraw = None;
        match app.subcommand_matches("withdraw") {
            Some(withdraw_sub) => {
                let mut wdraw = WithdrawParams::new();
                if let Some(address) = withdraw_sub.value_of("address") {
                    wdraw.pub_key = address.to_string();
                }
                if let Some(amount) = withdraw_sub.value_of("amount") {
                    wdraw.amount = amount.parse().expect("couldn't convert the amount to f64");
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
            cashier,
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
