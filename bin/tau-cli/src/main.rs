use clap::{IntoApp, Parser};
use log::{debug, error};

use darkfi::{
    rpc::jsonrpc::{self, JsonResult},
    util::cli::log_config,
    Error, Result,
};
use serde_json::{json, Value};
use simplelog::{ColorChoice, TermLogger, TerminalMode};
use url::Url;

/// Tau cli
#[derive(Parser)]
#[clap(name = "tau")]
pub struct CliTau {
    /// Add a new task
    #[clap(long)]
    pub add: Option<String>,
    /// list open tasks
    #[clap(long)]
    pub list: Option<String>,
    /// Show task by ID
    #[clap(long)]
    pub show: Option<u32>,
    /// Start task by ID
    #[clap(long)]
    pub start: Option<u32>,
    /// Pause task by ID
    #[clap(long)]
    pub pause: Option<u32>,
    /// Stop task by ID
    #[clap(long)]
    pub stop: Option<u32>,
    /// Comment on task by ID
    #[clap(long)]
    pub comment: Option<u32>,
    /// Log drawdown
    #[clap(long)]
    pub log: Option<String>,
    /// Increase verbosity
    #[clap(short, parse(from_occurrences))]
    pub verbose: u8,
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

    // --> {"jsonrpc": "2.0", "method": "cmd_add", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": "params", "id": 42}
    async fn cmd_add(&self) -> Result<Value> {
        let req = jsonrpc::request(json!("cmd_add"), json!([]));
        Ok(self.request(req).await?)
    }
}

async fn start(options: CliTau) -> Result<()> {
    let rpc_addr = "tcp://127.0.0.1:7777";
    let client = Client::new(rpc_addr.to_string());
    if options.add.is_some() {
        let reply = client.cmd_add().await?;
        println!("Server replied: {}", &reply.to_string());
        return Ok(())
    }
    error!("Please run 'tau help' to see usage.");

    Err(Error::MissingParams)
}

#[async_std::main]
async fn main() -> Result<()> {
    let args = CliTau::parse();
    let matches = CliTau::into_app().get_matches();
    let verbosity_level = matches.occurrences_of("verbose");

    //let config_path = if args.config.is_some() {
    //    expand_path(&args.config.clone().unwrap())?
    //} else {
    //    join_config_path(&PathBuf::from("tau.toml"))?
    //};

    // Spawn config file if it's not in place already.
    //spawn_config(&config_path, CONFIG_FILE_CONTENTS)?;

    let (lvl, conf) = log_config(verbosity_level)?;
    TermLogger::init(lvl, conf, TerminalMode::Mixed, ColorChoice::Auto)?;

    start(args).await
}
