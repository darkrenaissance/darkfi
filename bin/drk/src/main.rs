use std::{process::exit, str::FromStr, time::Instant};

use clap::{Parser, Subcommand};
use log::{debug, error};
use serde_json::{json, Value};
use simplelog::{ColorChoice, TermLogger, TerminalMode};
use url::Url;

use darkfi::{
    cli_desc,
    crypto::address::Address,
    rpc::{
        jsonrpc,
        jsonrpc::{JsonRequest, JsonResult},
    },
    util::cli::log_config,
    Error::JsonRpcError,
    Result,
};

#[derive(Parser)]
#[clap(name = "drk", about = cli_desc!(), version)]
#[clap(arg_required_else_help(true))]
struct Args {
    #[clap(short, parse(from_occurrences))]
    /// Increase verbosity (-vvv supported)
    verbose: u8,

    #[clap(short, long, default_value = "tcp://127.0.0.1:8340")]
    /// darkfid JSON-RPC endpoint
    endpoint: Url,

    #[clap(subcommand)]
    command: DrkSubcommand,
}

#[derive(Subcommand)]
enum DrkSubcommand {
    /// Send a ping request to the RPC
    Ping,

    /// Send an airdrop request to the faucet
    Airdrop {
        #[clap(long, parse(try_from_str))]
        /// Address where the airdrop should be requested
        /// (default is darkfid's wallet default)
        address: Option<Address>,

        #[clap(long)]
        /// JSON-RPC endpoint of the faucet
        endpoint: Url,

        /// f64 amount requested for airdrop
        amount: f64,
    },

    /// Wallet operations
    Wallet {
        #[clap(long)]
        /// Generate a new keypair in the wallet
        keygen: bool,

        #[clap(long)]
        /// Query the wallet for known balances
        balance: bool,

        #[clap(long)]
        /// Get the default address in the wallet
        address: bool,

        #[clap(long)]
        /// Get all addresses in the wallet
        all_addresses: bool,
    },
}

struct Drk {
    pub rpc_endpoint: Url,
}

impl Drk {
    async fn request(&self, r: JsonRequest, endpoint: Option<Url>) -> Result<Value> {
        debug!(target: "rpc", "--> {}", serde_json::to_string(&r)?);

        let ep =
            if endpoint.is_some() { endpoint.unwrap().clone() } else { self.rpc_endpoint.clone() };

        let reply = match jsonrpc::send_request(&ep, json!(r), None).await {
            Ok(v) => v,
            Err(e) => return Err(e),
        };

        match reply {
            JsonResult::Resp(r) => {
                debug!(target: "rpc", "<-- {}", serde_json::to_string(&r)?);
                Ok(r.result)
            }

            JsonResult::Err(e) => {
                debug!(target: "rpc", "<-- {}", serde_json::to_string(&e)?);
                Err(JsonRpcError(e.error.message.to_string()))
            }

            JsonResult::Notif(n) => {
                debug!(target: "rpc", "<-- {}", serde_json::to_string(&n)?);
                Err(JsonRpcError("Unexpected reply".to_string()))
            }
        }
    }

    async fn ping(&self) -> Result<()> {
        let start = Instant::now();

        let req = jsonrpc::request(json!("ping"), json!([]));
        let rep = match self.request(req, None).await {
            Ok(v) => v,
            Err(e) => {
                error!("Got an error: {}", e);
                return Err(e)
            }
        };

        let latency = Instant::now() - start;
        println!("Got reply: {}", rep);
        println!("Latency: {:?}", latency);
        Ok(())
    }

    async fn airdrop(&self, address: Option<Address>, endpoint: Url, amount: f64) -> Result<()> {
        let addr = if address.is_some() {
            address.unwrap()
        } else {
            let req = jsonrpc::request(json!("wallet.get_key"), json!([0_i64]));
            let rep = match self.request(req, None).await {
                Ok(v) => v,
                Err(e) => {
                    error!("Error while fetching default key from wallet: {}", e);
                    return Err(e)
                }
            };

            Address::from_str(rep.as_array().unwrap()[0].as_str().unwrap())?
        };

        println!("Requesting airdrop for {}", addr);
        let req = jsonrpc::request(json!("airdrop"), json!([json!(addr.to_string()), amount]));
        let rep = match self.request(req, Some(endpoint)).await {
            Ok(v) => v,
            Err(e) => {
                error!("Failed requesting airdrop: {}", e);
                return Err(e)
            }
        };

        println!("Success! Transaction ID: {}", rep);
        Ok(())
    }

    async fn wallet_keygen(&self) -> Result<()> {
        let req = jsonrpc::request(json!("wallet.keygen"), json!([]));
        let rep = match self.request(req, None).await {
            Ok(v) => v,
            Err(e) => {
                error!("Error while generating new key in wallet: {}", e);
                return Err(e)
            }
        };

        println!("New address: {}", rep);
        Ok(())
    }

    async fn wallet_balance(&self) -> Result<()> {
        let req = jsonrpc::request(json!("wallet.get_balances"), json!([]));
        let rep = match self.request(req, None).await {
            Ok(v) => v,
            Err(e) => {
                error!("Error fetching balances from wallet: {}", e);
                return Err(e)
            }
        };

        // TODO: Better representation
        println!("Balances:\n{:#?}", rep);
        Ok(())
    }

    async fn wallet_address(&self) -> Result<()> {
        let req = jsonrpc::request(json!("wallet.get_key"), json!([0_i64]));
        let rep = match self.request(req, None).await {
            Ok(v) => v,
            Err(e) => {
                error!("Error fetching default keypair from wallet: {}", e);
                return Err(e)
            }
        };

        println!("Default wallet address: {}", rep);
        Ok(())
    }

    async fn wallet_all_addresses(&self) -> Result<()> {
        let req = jsonrpc::request(json!("wallet.get_key"), json!([-1]));
        let rep = match self.request(req, None).await {
            Ok(v) => v,
            Err(e) => {
                error!("Error fetching keypairs from wallet: {}", e);
                return Err(e)
            }
        };

        println!("Wallet addresses:\n{:#?}", rep);
        Ok(())
    }
}

#[async_std::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let (lvl, conf) = log_config(args.verbose.into())?;
    TermLogger::init(lvl, conf, TerminalMode::Mixed, ColorChoice::Auto)?;

    let drk = Drk { rpc_endpoint: args.endpoint };

    match args.command {
        DrkSubcommand::Ping => drk.ping().await,

        DrkSubcommand::Airdrop { address, endpoint, amount } => {
            drk.airdrop(address, endpoint, amount).await
        }

        DrkSubcommand::Wallet { keygen, balance, address, all_addresses } => {
            if keygen {
                return drk.wallet_keygen().await
            }

            if balance {
                return drk.wallet_balance().await
            }

            if address {
                return drk.wallet_address().await
            }

            if all_addresses {
                return drk.wallet_all_addresses().await
            }

            eprintln!("Run 'drk wallet -h' to see the subcommand usage.");
            exit(2);
        }
    }
}
