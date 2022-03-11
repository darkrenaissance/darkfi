use clap::{AppSettings, IntoApp, Parser, Subcommand};
use log::{debug, error};

use std::{
    env::{temp_dir, var},
    fs::{self, File},
    io::{Read, Write},
};

use darkfi::{
    rpc::jsonrpc::{self, JsonResult},
    util::cli::log_config,
    Error, Result,
};
use serde_json::{json, Value};
use simplelog::{ColorChoice, TermLogger, TerminalMode};
use std::{io, process::Command};
use url::Url;

#[derive(Subcommand)]
pub enum CliTauSubCommands {
    /// Add a new task
    Add {
        #[clap(short, long)]
        title: Option<String>,
        #[clap(long)]
        desc: Option<String>,
        #[clap(short, long)]
        assign: Option<Vec<String>>,
        #[clap(short, long)]
        project: Option<Vec<String>>,
        #[clap(short, long)]
        due: Option<String>,
        #[clap(short, long)]
        rank: Option<u32>,
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
    title: Option<String>,
    desc: Option<String>,
    assign: Option<Vec<String>>,
    project: Option<Vec<String>>,
    due: Option<String>,
    rank: Option<u32>,
) -> Result<Value> {
    let req = jsonrpc::request(json!("add"), json!([title, desc, assign, project, due, rank]));
    Ok(request(req, url).await?)
}

async fn start(options: CliTau) -> Result<()> {
    let rpc_addr = "tcp://127.0.0.1:8875";
    if let Some(CliTauSubCommands::Add { title, desc, assign, project, due, rank }) =
        options.command
    {
        let t = if title.is_none() {
            print!("Title: ");
            io::stdout().flush().unwrap();
            let mut t = String::new();
            io::stdin().read_line(&mut t)?;
            if &t[(t.len() - 1)..] == "\n" {
                t.pop();
            }
            Some(t)
        } else {
            title
        };

        let des = if desc.is_none() {
            let editor = var("EDITOR").unwrap();
            let mut file_path = temp_dir();
            file_path.push("temp_file");
            File::create(&file_path)?;
            fs::write(
                &file_path,
                "\n# Write task description above this line\n# These lines will be removed\n",
            )?;

            Command::new(editor).arg(&file_path).status()?;

            let mut lines = String::new();
            File::open(file_path)?.read_to_string(&mut lines)?;

            let mut description = String::new();
            for line in lines.split('\n') {
                if !line.starts_with('#') {
                    description.push_str(line)
                }
            }

            Some(description)
        } else {
            desc
        };

        let r = if rank.is_none() { Some(0) } else { rank };

        add(rpc_addr.to_string(), t, des, assign, project, due, r).await?;
        //println!("Added task: {:#?}", t);
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
