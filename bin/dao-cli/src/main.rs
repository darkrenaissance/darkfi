use clap::{IntoApp, Parser, Subcommand};
use log::{debug, error};
use serde_json::{json, Value};
use url::Url;

use darkfi::{
    rpc::{jsonrpc, jsonrpc::JsonResult},
    Error, Result,
};

#[derive(Subcommand)]
pub enum CliDaoSubCommands {
    /// Say hello to the RPC
    Hello {},
}

/// DAO cli
#[derive(Parser)]
#[clap(name = "dao")]
pub struct CliDao {
    /// Increase verbosity
    #[clap(short, parse(from_occurrences))]
    pub verbose: u8,
    #[clap(subcommand)]
    pub command: Option<CliDaoSubCommands>,
}
pub struct Client {
    url: String,
}

impl Client {
    pub fn new(url: String) -> Self {
        Self { url }
    }

    async fn request(&self, r: jsonrpc::JsonRequest) -> Result<Value> {
        let reply: JsonResult =
            match jsonrpc::send_request(&Url::parse(&self.url)?, json!(r), None).await {
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
}

async fn start(options: CliDao) -> Result<()> {
    let rpc_addr = "tcp://127.0.0.1:7777";
    let client = Client::new(rpc_addr.to_string());
    match options.command {
        Some(CliDaoSubCommands::Hello {}) => {
            let reply = client.say_hello().await?;
            println!("Server replied: {}", &reply.to_string());
            return Ok(())
        }
        None => {}
    }
    error!("Please run 'dao help' to see usage.");

    Err(Error::MissingParams)
}

#[async_std::main]
async fn main() -> Result<()> {
    let args = CliDao::parse();
    let _matches = CliDao::into_app().get_matches();

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
