use serde_json::json;
use simplelog::{ColorChoice, TermLogger, TerminalMode};
use structopt::StructOpt;
use url::Url;

use darkfi::{
    rpc::{client::RpcClient, jsonrpc::JsonRequest},
    util::cli::{get_log_config, get_log_level},
    Result,
};

#[derive(Clone, Debug, StructOpt)]
#[structopt(name = "darkwikiupdate")]
struct Args {
    #[structopt(long)]
    /// Merge/Update without applying the changes
    dry_run: bool,
    #[structopt(long)]
    /// Show all patches info
    log: bool,
    #[structopt(short, parse(from_occurrences))]
    /// Increase verbosity (-vvv supported)
    verbose: u8,
    #[structopt(short, long, default_value = "tcp://127.0.0.1:13055")]
    /// darkfid JSON-RPC endpoint
    endpoint: Url,
}

fn print_patches(value: &Vec<serde_json::Value>) {
    for res in value {
        let res = res.as_array().unwrap();
        let (title, changes) = (res[0].as_str().unwrap(), res[1].as_str().unwrap());
        println!("FILE: {}", title);
        println!("{}", changes);
        println!("----------------------------------");
    }
}

#[async_std::main]
async fn main() -> Result<()> {
    let args = Args::from_args();

    let log_level = get_log_level(args.verbose.into());
    let log_config = get_log_config();
    TermLogger::init(log_level, log_config, TerminalMode::Mixed, ColorChoice::Auto)?;

    let rpc_client = RpcClient::new(args.endpoint).await?;

    let req = if args.dry_run {
        JsonRequest::new("dry_run", json!([]))
    } else if args.log {
        JsonRequest::new("log", json!([]))
    } else {
        JsonRequest::new("update", json!([]))
    };

    let result = rpc_client.request(req).await?;

    if !args.log {
        let result = result.as_array().unwrap();
        let local_patches = result[0].as_array().unwrap();
        let sync_patches = result[1].as_array().unwrap();
        let merge_patches = result[2].as_array().unwrap();

        if !local_patches.is_empty() {
            println!("");
            println!("PUBLISH LOCAL PATCHES:");
            println!("");
            print_patches(local_patches);
        }

        if !sync_patches.is_empty() {
            println!("");
            println!("RECEIVED PATCHES:");
            println!("");
            print_patches(sync_patches);
        }

        if !merge_patches.is_empty() {
            println!("");
            println!("MERGE:");
            println!("");
            print_patches(merge_patches);
        }
    }

    if args.log {
        todo!("TODO");
    }

    rpc_client.close().await
}
