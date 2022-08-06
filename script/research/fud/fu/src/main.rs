use clap::{Parser, Subcommand};
use log::info;
use serde_json::json;
use simplelog::{ColorChoice, TermLogger, TerminalMode};
use url::Url;

use darkfi::{
    cli_desc,
    rpc::{client::RpcClient, jsonrpc::JsonRequest},
    util::cli::{get_log_config, get_log_level},
    Result,
};

#[derive(Parser)]
#[clap(name = "fu", about = cli_desc!(), version)]
#[clap(arg_required_else_help(true))]
struct Args {
    #[clap(short, parse(from_occurrences))]
    /// Increase verbosity (-vvv supported)
    verbose: u8,

    #[clap(short, long, default_value = "tcp://127.0.0.1:9540")]
    /// fud JSON-RPC endpoint
    endpoint: Url,

    #[clap(subcommand)]
    command: Subcmd,
}

#[derive(Subcommand)]
enum Subcmd {
    /// List fud folder contents
    List,

    /// Sync fud folder contents and signal network for record changes
    Sync,

    /// Retrieve provided file name from the fud network
    Get {
        #[clap(short, long)]
        /// File name
        file: String,
    },
}

struct Fu {
    pub rpc_client: RpcClient,
}

impl Fu {
    async fn close_connection(&self) -> Result<()> {
        self.rpc_client.close().await
    }

    async fn list(&self) -> Result<()> {
        let req = JsonRequest::new("list", json!([]));
        let rep = self.rpc_client.request(req).await?;

        // Extract response
        let content = rep[0].as_array().unwrap();
        let new = rep[1].as_array().unwrap();
        let deleted = rep[2].as_array().unwrap();

        // Print info
        info!("----------Content-------------");
        if content.is_empty() {
            info!("No file records exists in DHT.");
        } else {
            for name in content {
                info!("\t{}", name.as_str().unwrap());
            }
        }
        info!("------------------------------");

        info!("----------New files-----------");
        if new.is_empty() {
            info!("No new files to import.");
        } else {
            for name in new {
                info!("\t{}", name.as_str().unwrap());
            }
        }
        info!("------------------------------");

        info!("----------Removed keys--------");
        if deleted.is_empty() {
            info!("No keys were removed.");
        } else {
            for key in deleted {
                info!("\t{}", key.as_str().unwrap());
            }
        }
        info!("------------------------------");

        Ok(())
    }

    async fn sync(&self) -> Result<()> {
        let req = JsonRequest::new("sync", json!([]));
        self.rpc_client.request(req).await?;
        info!("Daemon synced successfully!");
        Ok(())
    }

    async fn get(&self, file: String) -> Result<()> {
        let req = JsonRequest::new("get", json!([file]));
        let rep = self.rpc_client.request(req).await?;
        let path = rep.as_str().unwrap();
        info!("File waits you at: {}", path);
        Ok(())
    }
}

#[async_std::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let log_level = get_log_level(args.verbose.into());
    let log_config = get_log_config();
    TermLogger::init(log_level, log_config, TerminalMode::Mixed, ColorChoice::Auto)?;

    let rpc_client = RpcClient::new(args.endpoint).await?;
    let fu = Fu { rpc_client };

    match args.command {
        Subcmd::List => fu.list().await,
        Subcmd::Sync => fu.sync().await,
        Subcmd::Get { file } => fu.get(file).await,
    }?;

    fu.close_connection().await
}
