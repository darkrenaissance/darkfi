use clap::{IntoApp, Parser, Subcommand};
use serde_json::{json, Value};
use url::Url;

use darkfi::{
    rpc::{client::RpcClient, jsonrpc::JsonRequest},
    Result,
};

#[derive(Subcommand)]
pub enum CliDaoSubCommands {
    /// Say hello to the RPC
    Hello {},
}

/// DAO cli
#[derive(Parser)]
#[clap(name = "dao")]
#[clap(arg_required_else_help(true))]
pub struct CliDao {
    /// Increase verbosity
    #[clap(short, parse(from_occurrences))]
    pub verbose: u8,
    #[clap(subcommand)]
    pub command: Option<CliDaoSubCommands>,
}
pub struct Rpc {
    client: RpcClient,
}

impl Rpc {
    // --> {"jsonrpc": "2.0", "method": "say_hello", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": "hello world", "id": 42}
    async fn say_hello(&self) -> Result<Value> {
        let req = JsonRequest::new("say_hello", json!([]));
        self.client.request(req).await
    }
}

async fn start(options: CliDao) -> Result<()> {
    let rpc_addr = "tcp://127.0.0.1:7777";
    let client = Rpc { client: RpcClient::new(Url::parse(rpc_addr)?).await? };
    match options.command {
        Some(CliDaoSubCommands::Hello {}) => {
            let reply = client.say_hello().await?;
            println!("Server replied: {}", &reply.to_string());
            return Ok(())
        }
        None => {}
    }

    Ok(())
}

#[async_std::main]
async fn main() -> Result<()> {
    let args = CliDao::parse();
    let _matches = CliDao::command().get_matches();

    //let config_path = if args.config.is_some() {
    //    expand_path(&args.config.clone().unwrap())?
    //} else {
    //    join_config_path(&PathBuf::from("drk.toml"))?
    //};

    // Spawn config file if it's not in place already.
    //spawn_config(&config_path, CONFIG_FILE_CONTENTS)?;

    //let (lvl, conf) = log_config(matches)?;
    //TermLogger::init(lvl, conf, TerminalMode::Mixed, ColorChoice::Auto)?;

    //let config = Config::<DrkConfig>::load(config_path)?;

    start(args).await
}
