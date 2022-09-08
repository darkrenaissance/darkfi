use clap::{IntoApp, Parser, Subcommand};
use url::Url;

use darkfi::{rpc::client::RpcClient, Result};

mod rpc;

#[derive(Subcommand)]
pub enum CliDaoSubCommands {
    /// Create DAO
    Create {},
    /// Airdrop tokens
    Airdrop {},
    /// Propose
    Propose {},
    /// Vote
    Vote {},
    /// Execute
    Exec {},
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

async fn start(options: CliDao) -> Result<()> {
    let rpc_addr = "tcp://127.0.0.1:7777";
    let client = Rpc { client: RpcClient::new(Url::parse(rpc_addr)?).await? };
    match options.command {
        Some(CliDaoSubCommands::Create {}) => {
            let reply = client.create().await?;
            println!("Server replied: {}", &reply.to_string());
            return Ok(())
        }
        Some(CliDaoSubCommands::Airdrop {}) => {
            let reply = client.airdrop().await?;
            println!("Server replied: {}", &reply.to_string());
            return Ok(())
        }
        Some(CliDaoSubCommands::Propose {}) => {
            let reply = client.propose().await?;
            println!("Server replied: {}", &reply.to_string());
            return Ok(())
        }
        Some(CliDaoSubCommands::Vote {}) => {
            let reply = client.vote().await?;
            println!("Server replied: {}", &reply.to_string());
            return Ok(())
        }
        Some(CliDaoSubCommands::Exec {}) => {
            let reply = client.exec().await?;
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
