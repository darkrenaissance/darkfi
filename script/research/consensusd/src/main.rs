use std::{net::SocketAddr, thread, time};

use async_executor::Executor;
use async_std::sync::Arc;
use clap::{IntoApp, Parser};
use easy_parallel::Parallel;
use serde::{Deserialize, Serialize};
use simplelog::{ColorChoice, TermLogger, TerminalMode};

use darkfi::{
    rpc::rpcserver::{listen_and_serve, RpcServerConfig},
    util::{
        cli::{log_config, spawn_config, Config},
        expand_path,
        path::get_config_path,
    },
    Result,
};

use consensusd::service::{APIService, State};

/// This struct represent the configuration parameters used by the Consensus daemon.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ConsensusdConfig {
    /// The endpoint where chaind will bind its RPC socket
    pub rpc_listen_address: SocketAddr,
    /// Whether to listen with TLS or plain TCP
    pub serve_tls: bool,
    /// Path to DER-formatted PKCS#12 archive. (Unused if serve_tls=false)
    pub tls_identity_path: String,
    /// Password for the TLS identity. (Unused if serve_tls=false)
    pub tls_identity_password: String,
    /// Path to the state file
    pub state_path: String,
    /// Node ID, used only for testing
    pub id: u64,
}

/// Chaind cli configuration.
#[derive(Parser)]
#[clap(name = "consensusd")]
pub struct CliConsensusd {
    /// Sets a custom config file
    #[clap(short, long)]
    pub config: Option<String>,
    /// Increase verbosity
    #[clap(short, parse(from_occurrences))]
    pub verbose: u8,
}

/// RPCAPI service initialization.
async fn api_service_init(executor: Arc<Executor<'_>>, config: &ConsensusdConfig) -> Result<()> {
    let server_config = RpcServerConfig {
        socket_addr: config.rpc_listen_address,
        use_tls: config.serve_tls,
        identity_path: expand_path(&config.clone().tls_identity_path)?,
        identity_pass: config.tls_identity_password.clone(),
    };

    let state_path = expand_path(&config.state_path)?;
    let id = config.id;

    let api_service = APIService::new(id, state_path)?;

    listen_and_serve(server_config, api_service, executor).await
}

/// RPCAPI:
/// Node checks if its the current slot leader and generates the slot Block (represented as a Vote structure).
/// TODO: 1, This should be a scheduled task.
///       2. Nodes count not hard coded.
///       3. Proposed block broadcast.
fn consensus_task(config: &ConsensusdConfig) {
    let state_path = expand_path(&config.state_path).unwrap();
    let id = config.id;
    let nodes_count = 1;

    println!("Waiting for state initialization...");
    thread::sleep(time::Duration::from_secs(10));

    // After initialization node should wait for next epoch
    let state = State::load_current_state(id, &state_path).unwrap();
    let seconds_until_next_epoch = state.get_seconds_until_next_epoch_start();
    println!("Waiting for next epoch({:?} sec)...", seconds_until_next_epoch);
    thread::sleep(seconds_until_next_epoch);

    loop {
        let state = State::load_current_state(id, &state_path).unwrap();
        let proposed_block =
            if state.check_if_epoch_leader(nodes_count) { state.propose_block() } else { None };
        if proposed_block.is_none() {
            println!("Node is not the epoch leader. Sleeping till next epoch...");
        } else {
            // TODO: Proposed block broadcast.
            println!("Node is the epoch leader. Proposed block: {:?}", proposed_block);
        }

        let seconds_until_next_epoch = state.get_seconds_until_next_epoch_start();
        println!("Waiting for next epoch({:?} sec)...", seconds_until_next_epoch);
        thread::sleep(seconds_until_next_epoch);
    }
}

const CONFIG_FILE_CONTENTS: &[u8] = include_bytes!("../consensusd_config.toml");

/// Consensus daemon initialization.
#[async_std::main]
async fn main() -> Result<()> {
    let args = CliConsensusd::parse();
    let matches = CliConsensusd::command().get_matches();

    let verbosity_level = matches.occurrences_of("verbose");
    let (lvl, conf) = log_config(verbosity_level)?;
    TermLogger::init(lvl, conf, TerminalMode::Mixed, ColorChoice::Auto)?;

    let config_path = get_config_path(args.config, "consensusd_config.toml")?;
    spawn_config(&config_path, CONFIG_FILE_CONTENTS)?;

    let config: ConsensusdConfig = Config::<ConsensusdConfig>::load(config_path)?;

    let main_ex = Arc::new(Executor::new());
    let api_ex = main_ex.clone();
    let (signal, shutdown) = async_channel::unbounded::<()>();
    let signal1 = signal.clone();
    let signal2 = signal.clone();
    let (result, _) = Parallel::new()
        // Run the RCP API service future in background.
        .add(|| {
            smol::future::block_on(async {
                api_service_init(api_ex, &config).await?;
                drop(signal1);
                Ok::<(), darkfi::Error>(())
            })
        })
        // Run the consensus task in background.
        .add(|| {
            consensus_task(&config);
            drop(signal2);
            Ok::<(), darkfi::Error>(())
        })
        // Run the shutdown signal receive future on the current thread.
        .finish(|| smol::future::block_on(main_ex.run(shutdown.recv())));

    result.first().unwrap().clone()
}
