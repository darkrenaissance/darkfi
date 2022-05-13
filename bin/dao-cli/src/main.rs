use async_std::sync::Arc;

use async_executor::Executor;
use clap::{IntoApp, Parser, Subcommand};
use serde_json::{json, Value};
use smol::future;
use url::Url;

use darkfi::{
    rpc::{jsonrpc, rpcclient::RpcClient},
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
        let req = jsonrpc::request(json!("say_hello"), json!([]));
        Ok(self.client.request(req).await?)
    }
}

async fn start(options: CliDao, executor: Arc<Executor<'_>>) -> Result<()> {
    let rpc_addr = "tcp://127.0.0.1:7777";
    let client = Rpc { client: RpcClient::new(Url::parse(rpc_addr)?, executor).await? };
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

    let executor = Arc::new(Executor::new());

    let task = executor.spawn(start(args, executor.clone()));

    // Run the executor until the task completes.
    future::block_on(executor.run(task))?;
    Ok(())
}
