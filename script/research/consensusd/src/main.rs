use std::net::SocketAddr;

use easy_parallel::Parallel;
use async_executor::Executor;
use async_std::sync::Arc;
use clap::{IntoApp, Parser};
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

use consensusd::service::ConsensusService;

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

/// Consensus service initialization.
async fn start(executor: Arc<Executor<'_>>, config: &ConsensusdConfig) -> Result<()> {
    let server_config = RpcServerConfig {
        socket_addr: config.rpc_listen_address,
        use_tls: config.serve_tls,
        identity_path: expand_path(&config.clone().tls_identity_path)?,
        identity_pass: config.tls_identity_password.clone(),
    };

    let state_path = expand_path(&config.state_path)?;
    let id = config.id;

    let chain_service = ConsensusService::new(id, state_path)?;

    listen_and_serve(server_config, chain_service, executor).await
}

async fn start2(executor: Arc<Executor<'_>>, config: &ConsensusdConfig) -> Result<()> {
    
    while true {
        println!("sss");
    };
    Ok(())
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

    let ex = Arc::new(Executor::new());
    let ex2 = ex.clone();
    let ex3 = ex.clone();
    let (signal, shutdown) = async_channel::unbounded::<()>();
    let signal1 = signal.clone();
    let signal2 = signal.clone();    
    let (result, _) = Parallel::new()
        .add(|| {
            smol::future::block_on(async {
                start(ex2, &config).await?;
                drop(signal1);
                Ok::<(), darkfi::Error>(())
            })
        })
        .add(|| {
            smol::future::block_on(async {
                start2(ex3, &config).await?;
                drop(signal2);
                Ok::<(), darkfi::Error>(())
            })
        })
        // Run the main future on the current thread.
        .finish(|| smol::future::block_on(ex.run(shutdown.recv())));

    Ok(())
}
