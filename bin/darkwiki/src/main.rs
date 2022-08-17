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
    #[structopt(short, parse(from_occurrences))]
    /// Increase verbosity (-vvv supported)
    verbose: u8,

    #[structopt(short, long, default_value = "tcp://127.0.0.1:13055")]
    /// darkfid JSON-RPC endpoint
    endpoint: Url,
}

#[async_std::main]
async fn main() -> Result<()> {
    let args = Args::from_args();

    let log_level = get_log_level(args.verbose.into());
    let log_config = get_log_config();
    TermLogger::init(log_level, log_config, TerminalMode::Mixed, ColorChoice::Auto)?;

    let rpc_client = RpcClient::new(args.endpoint).await?;

    let req = JsonRequest::new("update", json!([]));
    rpc_client.request(req).await?;

    rpc_client.close().await
}
