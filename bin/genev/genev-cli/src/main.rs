use clap::{Parser, Subcommand};

use darkfi_serial::{SerialDecodable, SerialEncodable};
use serde::Serialize;
use simplelog::{ColorChoice, TermLogger, TerminalMode};
use url::Url;

use darkfi::{
    rpc::client::RpcClient,
    util::cli::{get_log_config, get_log_level},
    Result,
};

use crate::rpc::Gen;

mod rpc;

#[derive(SerialEncodable, SerialDecodable, Debug, Serialize)]
pub struct BaseEvent {
    pub nick: String,
    pub title: String,
    pub text: String,
}

#[derive(Parser)]
#[clap(name = "genev", version)]
struct Args {
    #[arg(short, action = clap::ArgAction::Count)]
    /// Increase verbosity (-vvv supported)
    verbose: u8,

    #[clap(short, long, default_value = "tcp://127.0.0.1:28880")]
    /// JSON-RPC endpoint
    endpoint: Url,

    #[clap(subcommand)]
    command: Option<SubCmd>,
}

#[derive(Subcommand)]
enum SubCmd {
    Add { values: Vec<String> },

    List,
}

#[async_std::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let log_level = get_log_level(args.verbose.into());
    let log_config = get_log_config();
    TermLogger::init(log_level, log_config, TerminalMode::Mixed, ColorChoice::Auto)?;

    let rpc_client = RpcClient::new(args.endpoint).await?;
    let gen = Gen { rpc_client };

    match args.command {
        Some(subcmd) => match subcmd {
            SubCmd::Add { values } => {
                let event = BaseEvent {
                    nick: values[0].clone(),
                    title: values[1].clone(),
                    text: values[2..].join(" "),
                };
                return gen.add(event).await
            }
            SubCmd::List => {
                let events = gen.list().await?;
                for event in events {
                    println!("=============================");
                    println!(
                        "- nickname: {}, title: {}, text: {}",
                        event.action.nick, event.action.title, event.action.text
                    );
                }
            }
        },
        None => println!("none"),
    }

    gen.close_connection().await?;

    Ok(())
}
