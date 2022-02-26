use std::{path::PathBuf, str::FromStr};

use clap::{AppSettings, IntoApp, Parser, Subcommand};
use log::{debug, error};
use prettytable::{cell, format, row, Table};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use simplelog::{ColorChoice, TermLogger, TerminalMode};
use url::Url;

use darkfi::{
    rpc::{jsonrpc, jsonrpc::JsonResult},
    util::{
        cli::{log_config, spawn_config, Config, UrlConfig},
        join_config_path,
        path::expand_path,
        NetworkName,
    },
    Error, Result,
};

/// The configuration for drk
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct DrkConfig {
    /// The URL where darkfid RPC is listening on
    pub darkfid_rpc_url: UrlConfig,
    /// Socks5 server url. eg. `socks5://127.0.0.1:9050` used for tor and nym protocols
    pub socks_url: UrlConfig,
}

#[derive(Subcommand)]
pub enum CliDrkSubCommands {
    /// Say hello to the RPC
    Hello {},
    /// Show what features the cashier supports
    Features {},
    /// Wallet operations
    Wallet {
        /// Initialize a new wallet
        #[clap(long)]
        create: bool,
        /// Generate wallet keypair
        #[clap(long)]
        keygen: bool,
        /// Get default wallet address
        #[clap(long)]
        address: bool,
        /// Get wallet addresses
        #[clap(long)]
        addresses: bool,
        /// Set default address
        #[clap(long, value_name = "ADDRESS")]
        set_default_address: Option<String>,
        /// Export default address
        #[clap(long, value_name = "PATH")]
        export_keypair: Option<String>,
        /// Import address
        #[clap(long, value_name = "PATH")]
        import_keypair: Option<String>,
        /// Get wallet balances
        #[clap(long)]
        balances: bool,
    },
    /// Get hexidecimal ID for token symbol
    Id {
        /// Which network to use (bitcoin/solana/...)
        #[clap(long)]
        network: String,
        /// Which token to query (btc/sol/usdc/...)
        #[clap(parse(try_from_str))]
        token: String,
    },
    /// Withdraw Dark tokens for clear tokens
    Withdraw {
        /// Which network to use (bitcoin/solana/...)
        #[clap(long)]
        network: String,
        /// Which token to receive (btc/sol/usdc/...)
        #[clap(parse(try_from_str))]
        token_sym: String,
        /// Recipient address
        #[clap(parse(try_from_str))]
        address: String,
        /// Amount to withdraw
        #[clap(parse(try_from_str))]
        amount: u64,
    },
    /// Transfer Dark tokens to address
    Transfer {
        /// Which network to use (bitcoin/solana/...)
        #[clap(long)]
        network: String,
        /// Which token to transfer (btc/sol/usdc/...)
        #[clap(parse(try_from_str))]
        token_sym: String,
        /// Recipient address
        #[clap(parse(try_from_str))]
        address: String,
        /// Amount to transfer
        #[clap(parse(try_from_str))]
        amount: f64,
    },
    /// Deposit clear tokens for Dark tokens
    Deposit {
        /// Which network to use (bitcoin/solana/...)
        #[clap(long)]
        network: String,
        /// Which token to deposit (btc/sol/usdc/...)
        #[clap(parse(try_from_str))]
        token_sym: String,
    },
}

/// Drk cli
#[derive(Parser)]
#[clap(name = "drk")]
#[clap(author, version, about)]
#[clap(global_setting(AppSettings::PropagateVersion))]
#[clap(global_setting(AppSettings::UseLongFormatForHelpSubcommand))]
#[clap(setting(AppSettings::SubcommandRequiredElseHelp))]
pub struct CliDrk {
    /// Sets a custom config file
    #[clap(short, long)]
    pub config: Option<String>,
    /// Increase verbosity
    #[clap(short, parse(from_occurrences))]
    pub verbose: u8,
    #[clap(subcommand)]
    pub command: Option<CliDrkSubCommands>,
}

const CONFIG_FILE_CONTENTS: &[u8] = include_bytes!("../drk_config.toml");

struct Drk {
    url: Url,
    socks_url: Url,
}

impl Drk {
    pub fn new(url: Url, socks_url: Url) -> Self {
        Self { url, socks_url }
    }

    // Retrieve cashier features and error if they
    // don't support the network
    async fn check_network(&self, network: &NetworkName) -> Result<()> {
        let features = self.features().await?;

        if features.as_object().is_none() &&
            features.as_object().unwrap()["networks"].as_array().is_none() &&
            features.as_object().unwrap()["networks"].as_array().unwrap().is_empty()
        {
            return Err(Error::NotSupportedNetwork)
        }

        for nets in features.as_object().unwrap()["networks"].as_array().unwrap() {
            for (net, _) in nets.as_object().unwrap() {
                if network == &NetworkName::from_str(net.as_str())? {
                    return Ok(())
                }
            }
        }

        Err(Error::NotSupportedNetwork)
    }

    async fn request(&self, r: jsonrpc::JsonRequest) -> Result<Value> {
        let reply: JsonResult =
            match jsonrpc::send_request(&self.url, json!(r), Some(self.socks_url.clone())).await {
                Ok(v) => v,
                Err(e) => return Err(e),
            };

        match reply {
            JsonResult::Resp(r) => {
                debug!(target: "RPC", "<-- {}", serde_json::to_string(&r)?);
                Ok(r.result)
            }

            JsonResult::Err(e) => {
                debug!(target: "RPC", "<-- {}", serde_json::to_string(&e)?);
                Err(Error::JsonRpcError(e.error.message.to_string()))
            }

            JsonResult::Notif(n) => {
                debug!(target: "RPC", "<-- {}", serde_json::to_string(&n)?);
                Err(Error::JsonRpcError("Unexpected reply".to_string()))
            }
        }
    }

    // --> {"jsonrpc": "2.0", "method": "say_hello", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": "hello world", "id": 42}
    async fn say_hello(&self) -> Result<Value> {
        let req = jsonrpc::request(json!("say_hello"), json!([]));
        Ok(self.request(req).await?)
    }

    // --> {"jsonrpc": "2.0", "method": "create_wallet", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 42}
    async fn create_wallet(&self) -> Result<Value> {
        let req = jsonrpc::request(json!("create_wallet"), json!([]));
        Ok(self.request(req).await?)
    }

    // --> {"jsonrpc": "2.0", "method": "key_gen", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 42}
    async fn key_gen(&self) -> Result<Value> {
        let req = jsonrpc::request(json!("key_gen"), json!([]));
        Ok(self.request(req).await?)
    }

    // --> {"jsonrpc": "2.0", "method": "get_key", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": "vdNS7oBj7KvsMWWmo9r96SV4SqATLrGsH2a3PGpCfJC", "id": 42}
    async fn get_key(&self) -> Result<Value> {
        let req = jsonrpc::request(json!("get_key"), json!([]));
        Ok(self.request(req).await?)
    }

    // --> {"jsonrpc": "2.0", "method": "get_keys", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": "[vdNS7oBj7KvsMWWmo9r96SV4SqATLrGsH2a3PGpCfJC, ...]", "id":
    // 42}
    async fn get_keys(&self) -> Result<Value> {
        let req = jsonrpc::request(json!("get_keys"), json!([]));
        Ok(self.request(req).await?)
    }

    // --> {"jsonrpc": "2.0", "method": "set_default_address", "params":
    // "[vdNS7oBj7KvsMWWmo9r96SV4SqATLrGsH2a3PGpCfJC]", "id": 42}
    // <-- {"jsonrpc": "2.0", "result":
    // true, "id": 42}
    async fn set_default_address(&self, address: &str) -> Result<Value> {
        let req = jsonrpc::request(json!("set_default_address"), json!([address]));
        Ok(self.request(req).await?)
    }

    // --> {"jsonrpc": "2.0", "method": "export_keypair", "params": "[path/]", "id": 42}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 42}
    async fn export_keypair(&self, path: &str) -> Result<Value> {
        let req = jsonrpc::request(json!("export_keypair"), json!([path]));
        Ok(self.request(req).await?)
    }

    // --> {"jsonrpc": "2.0", "method": "import_keypair", "params": "[path/]", "id": 42}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 42}
    async fn import_keypair(&self, path: &str) -> Result<Value> {
        let req = jsonrpc::request(json!("import_keypair"), json!([path]));
        Ok(self.request(req).await?)
    }

    // --> {"jsonrpc": "2.0", "method": "get_key", "params": ["solana", "usdc"], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": "vdNS7oBj7KvsMWWmo9r96SV4SqATLrGsH2a3PGpCfJC", "id": 42}
    async fn get_token_id(&self, network: &str, token: &str) -> Result<Value> {
        let req = jsonrpc::request(json!("get_token_id"), json!([network, token]));
        Ok(self.request(req).await?)
    }

    // --> {"method": "get_balances", "params": []}
    // <-- {"result": "get_balances": "[ {"btc": (value, network)}, .. ]"}
    async fn get_balances(&self) -> Result<Value> {
        let req = jsonrpc::request(json!("get_balances"), json!([]));
        Ok(self.request(req).await?)
    }

    // --> {"jsonrpc": "2.0", "method": "features", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": ["network": "btc", "sol"], "id": 42}
    async fn features(&self) -> Result<Value> {
        let req = jsonrpc::request(json!("features"), json!([]));
        Ok(self.request(req).await?)
    }

    // --> {"jsonrpc": "2.0", "method": "deposit", "params": ["solana", "usdc"], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": "Ht5G1RhkcKnpLVLMhqJc5aqZ4wYUEbxbtZwGCVbgU7DL", "id": 42}
    async fn deposit(&self, network: &str, token: &str) -> Result<Value> {
        let req = jsonrpc::request(json!("deposit"), json!([network, token]));
        Ok(self.request(req).await?)
    }

    // --> {"jsonrpc": "2.0", "method": "withdraw",
    //      "params": ["solana", "usdc", "Ht5G1RhkcKnpLVLMhqJc5aqZ4wYUEbxbtZwGCVbgU7DL", 13.37"],
    // "id": 42} <-- {"jsonrpc": "2.0", "result": "txID", "id": 42}
    async fn withdraw(
        &self,
        network: &str,
        token: &str,
        address: &str,
        amount: &str,
    ) -> Result<Value> {
        let req = jsonrpc::request(json!("withdraw"), json!([network, token, address, amount]));
        Ok(self.request(req).await?)
    }

    // --> {"jsonrpc": "2.0", "method": "transfer",
    //      "params": ["dusdc", "vdNS7oBj7KvsMWWmo9r96SV4SqATLrGsH2a3PGpCfJC", 13.37], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": "txID", "id": 42}
    async fn transfer(
        &self,
        network: &str,
        token: &str,
        address: &str,
        amount: &str,
    ) -> Result<Value> {
        let req = jsonrpc::request(json!("transfer"), json!([network, token, address, amount]));
        Ok(self.request(req).await?)
    }
}

async fn start(config: &DrkConfig, options: CliDrk) -> Result<()> {
    let client = Drk::new(
        Url::try_from(config.darkfid_rpc_url.clone())?,
        Url::try_from(config.socks_url.clone())?,
    );

    match options.command {
        Some(CliDrkSubCommands::Hello {}) => {
            let reply = client.say_hello().await?;
            println!("Server replied: {}", &reply.to_string());
            return Ok(())
        }
        Some(CliDrkSubCommands::Features {}) => {
            let reply = client.features().await?;
            println!("Features: {}", &reply.to_string());
            return Ok(())
        }
        Some(CliDrkSubCommands::Wallet {
            create,
            keygen,
            address,
            balances,
            addresses,
            export_keypair,
            import_keypair,
            set_default_address,
        }) => {
            if create {
                let reply = client.create_wallet().await?;
                if reply.as_bool().unwrap() {
                    println!("Wallet created successfully.")
                } else {
                    println!("Server replied: {}", &reply.to_string());
                }
                return Ok(())
            }

            if keygen {
                let reply = client.key_gen().await?;
                if reply.as_bool().unwrap() {
                    println!("Key generation successful.")
                } else {
                    println!("Server replied: {}", &reply.to_string());
                }
                return Ok(())
            }

            if address {
                let reply = client.get_key().await?;
                println!("Wallet address: {}", &reply.to_string());
                return Ok(())
            }

            if addresses {
                let reply = client.get_keys().await?;
                println!("Wallet addresses: ");
                if reply.as_array().is_some() {
                    for (i, address) in reply.as_array().unwrap().iter().enumerate() {
                        if i == 0 {
                            println!("- [X] {}", address);
                        } else {
                            println!("- [ ] {}", address);
                        }
                    }
                } else {
                    println!("Empty!!",);
                }
                return Ok(())
            }

            if balances {
                let reply = client.get_balances().await?;

                if reply.as_object().is_some() && !reply.as_object().unwrap().is_empty() {
                    let mut table = Table::new();
                    table.set_format(*format::consts::FORMAT_NO_LINESEP_WITH_TITLE);
                    table.set_titles(row!["token", "amount", "network"]);

                    for (tkn, data) in reply.as_object().unwrap() {
                        table.add_row(row![
                            tkn,
                            data[0].as_str().unwrap(),
                            data[1].as_str().unwrap()
                        ]);
                    }

                    table.printstd();
                } else {
                    println!("Balances: {}", 0);
                }

                return Ok(())
            }

            if set_default_address.is_some() {
                let default_address = set_default_address.unwrap();
                client.set_default_address(&default_address).await?;
                return Ok(())
            }

            if export_keypair.is_some() {
                let path = export_keypair.unwrap();
                client.export_keypair(&path).await?;
                return Ok(())
            }

            if import_keypair.is_some() {
                let path = import_keypair.unwrap();
                client.import_keypair(&path).await?;
                return Ok(())
            }
        }
        Some(CliDrkSubCommands::Id { network, token }) => {
            let network = network.to_lowercase();
            client.check_network(&NetworkName::from_str(&network)?).await?;

            let reply = client.get_token_id(&network, &token).await?;

            println!("Token ID: {}", &reply.to_string());
            return Ok(())
        }
        Some(CliDrkSubCommands::Deposit { network, token_sym }) => {
            let network = network.to_lowercase();

            client.check_network(&NetworkName::from_str(&network)?).await?;

            let reply = client.deposit(&network, &token_sym).await?;

            println!("Deposit your coins to the following address: {}", &reply.to_string());

            return Ok(())
        }
        Some(CliDrkSubCommands::Transfer { network, token_sym, address, amount }) => {
            let network = network.to_lowercase();

            client.check_network(&NetworkName::from_str(&network)?).await?;

            client.transfer(&network, &token_sym, &address, &amount.to_string()).await?;

            println!("{} {} Transfered successfully", amount, token_sym.to_uppercase(),);

            return Ok(())
        }

        Some(CliDrkSubCommands::Withdraw { network, token_sym, address, amount }) => {
            let network = network.to_lowercase();

            client.check_network(&NetworkName::from_str(&network)?).await?;

            let reply =
                client.withdraw(&network, &token_sym, &address, &amount.to_string()).await?;

            println!("{}", &reply.to_string());

            return Ok(())
        }
        None => {}
    }

    error!("Please run 'drk help' to see usage.");
    Err(Error::MissingParams)
}

#[async_std::main]
async fn main() -> Result<()> {
    let args = CliDrk::parse();
    let matches = CliDrk::into_app().get_matches();

    let config_path = if args.config.is_some() {
        expand_path(&args.config.clone().unwrap())?
    } else {
        join_config_path(&PathBuf::from("drk.toml"))?
    };

    // Spawn config file if it's not in place already.
    spawn_config(&config_path, CONFIG_FILE_CONTENTS)?;

    let verbosity_level = matches.occurrences_of("verbose");

    let (lvl, conf) = log_config(verbosity_level)?;

    TermLogger::init(lvl, conf, TerminalMode::Mixed, ColorChoice::Auto)?;

    let config = Config::<DrkConfig>::load(config_path)?;

    start(&config, args).await
}
