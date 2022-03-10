use clap::{AppSettings, IntoApp, Parser, Subcommand};
use log::{debug, error};

use darkfi::{
    rpc::jsonrpc::{self, JsonResult},
    util::cli::log_config,
    Error, Result,
};
use serde_json::{json, Value};
use simplelog::{ColorChoice, TermLogger, TerminalMode};
use url::Url;

#[derive(Subcommand)]
pub enum CliTauSubCommands {
    /// Add a new task
    Add {
        #[clap(short, long)]
        title: String,
        #[clap(long)]
        desc: String,
        #[clap(short, long)]
        assign: Option<String>,
        #[clap(short, long)]
        project: Option<String>,
        #[clap(short, long)]
        due: Option<String>,
        #[clap(short, long)]
        rank: Option<u64>,
    },
}

/// Tau cli
#[derive(Parser)]
#[clap(name = "tau")]
#[clap(author, version, about)]
#[clap(global_setting(AppSettings::PropagateVersion))]
#[clap(global_setting(AppSettings::UseLongFormatForHelpSubcommand))]
#[clap(setting(AppSettings::SubcommandRequiredElseHelp))]
pub struct CliTau {
    /// Increase verbosity
    #[clap(short, parse(from_occurrences))]
    pub verbose: u8,
    #[clap(subcommand)]
    pub command: Option<CliTauSubCommands>,
}

async fn request(r: jsonrpc::JsonRequest, url: String) -> Result<Value> {
    let reply: JsonResult = match jsonrpc::send_request(&Url::parse(&url)?, json!(r), None).await {
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

// Add new task and returns `true` upon success.
// --> {"jsonrpc": "2.0", "method": "add", "params": ["title", "desc", ["assign"], ["project"], "due", "rank"], "id": 1}
// <-- {"jsonrpc": "2.0", "result": true, "id": 1}
async fn add(
    url: String,
    title: String,
    desc: String,
    assign: Option<String>,
    project: Option<String>,
    due: Option<String>,
    rank: Option<u64>,
) -> Result<Value> {
    let req = jsonrpc::request(json!("add"), json!([title, desc, assign, project, due, rank]));
    Ok(request(req, url).await?)
}

async fn start(options: CliTau) -> Result<()> {
    let rpc_addr = "tcp://127.0.0.1:8875";
    if let Some(CliTauSubCommands::Add { title, desc, assign, project, due, rank }) =
        options.command
    {
        add(rpc_addr.to_string(), title.clone(), desc, assign, project, due, rank).await?;
        println!("Added task: {}", title);
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
