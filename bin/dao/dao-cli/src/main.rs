use clap::{IntoApp, Parser, Subcommand};
use url::Url;

use darkfi::{rpc::client::RpcClient, Result};

mod rpc;

#[derive(Subcommand)]
pub enum CliDaoSubCommands {
    /// Create DAO
    Create {
        /// Minium number of governance tokens a user must have to propose a vote.
        dao_proposer_limit: u64,

        /// Minimum number of governance tokens staked on a proposal for it to pass.
        dao_quorum: u64,

        /// Quotient value of minimum vote ratio of yes:no votes required for a proposal to pass.
        dao_approval_ratio_quot: u64,

        /// Base value of minimum vote ratio of yes:no votes required for a proposal to pass.
        dao_approval_ratio_base: u64,
    },
    /// Mint tokens
    Addr {},
    Mint {
        /// Number of treasury tokens to mint.
        token_supply: u64,

        /// Public key of the DAO treasury.
        dao_addr: String,

        /// DAO public identifier.
        dao_bulla: String,
    },
    Keygen {
        nym: String,
    },
    /// Airdrop tokens
    Airdrop {
        nym: String,

        value: u64,
    },
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
        Some(CliDaoSubCommands::Create {
            dao_proposer_limit,
            dao_quorum,
            dao_approval_ratio_base,
            dao_approval_ratio_quot,
        }) => {
            let reply = client
                .create(
                    dao_proposer_limit,
                    dao_quorum,
                    dao_approval_ratio_quot,
                    dao_approval_ratio_base,
                )
                .await?;
            println!("Created DAO bulla: {}", &reply.to_string());
            return Ok(())
        }
        Some(CliDaoSubCommands::Addr {}) => {
            let reply = client.addr().await?;
            println!("DAO public address: {}", &reply.to_string());
            return Ok(())
        }
        Some(CliDaoSubCommands::Mint { token_supply, dao_addr, dao_bulla }) => {
            let reply = client.mint(token_supply, dao_addr, dao_bulla).await?;
            println!("New DAO balance: {}", &reply.to_string());
            return Ok(())
        }
        Some(CliDaoSubCommands::Keygen { nym }) => {
            let reply = client.keygen(nym).await?;
            println!("User public key: {}", &reply.to_string());
            return Ok(())
        }
        Some(CliDaoSubCommands::Airdrop { nym, value }) => {
            let reply = client.airdrop(nym, value).await?;
            println!("New user balance: {}", &reply.to_string());
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
