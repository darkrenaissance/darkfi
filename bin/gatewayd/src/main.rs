use std::{path::PathBuf, sync::Arc};

use async_executor::Executor;
use clap::{IntoApp, Parser};
use easy_parallel::Parallel;
use log::debug;
use simplelog::{ColorChoice, LevelFilter, TermLogger, TerminalMode};

use darkfi::{
    blockchain::{rocks::columns, Rocks, RocksColumn},
    cli::{
        cli_config::{log_config, spawn_config},
        CliGatewayd, Config, GatewaydConfig,
    },
    node::service::gateway::GatewayService,
    util::{expand_path, join_config_path},
    Result,
};

const CONFIG_FILE_CONTENTS: &[u8] = include_bytes!("../gatewayd_config.toml");

async fn start(executor: Arc<Executor<'_>>, config: &GatewaydConfig) -> Result<()> {
    let rocks = Rocks::new(&expand_path(&config.database_path)?)?;
    let rocks_slabstore_column = RocksColumn::<columns::Slabs>::new(rocks);

    let gateway = GatewayService::new(
        config.protocol_listen_address,
        config.publisher_listen_address,
        rocks_slabstore_column,
    )?;

    Ok(gateway.start(executor.clone()).await?)
}

#[async_std::main]
async fn main() -> Result<()> {
    let args = CliGatewayd::parse();
    let matches = CliGatewayd::into_app().get_matches();

    let config_path = if args.config.is_some() {
        expand_path(&args.config.unwrap())?
    } else {
        join_config_path(&PathBuf::from("gatewayd.toml"))?
    };

    // Spawn config file if it's not in place already.
    spawn_config(&config_path, CONFIG_FILE_CONTENTS)?;

    let conf: simplelog::Config;
    let lvl: simplelog::LevelFilter;

    (lvl, conf) = log_config(matches)?;

    TermLogger::init(lvl, conf, TerminalMode::Mixed, ColorChoice::Auto)?;

    let config: GatewaydConfig = Config::<GatewaydConfig>::load(config_path)?;

    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = async_channel::unbounded::<()>();

    let ex2 = ex.clone();

    let nthreads = num_cpus::get();
    debug!(target: "GATEWAY DAEMON", "Run {} executor threads", nthreads);

    let (_, result) = Parallel::new()
        .each(0..nthreads, |_| smol::future::block_on(ex.run(shutdown.recv())))
        // Run the main future on the current thread.
        .finish(|| {
            smol::future::block_on(async move {
                start(ex2, &config).await?;
                drop(signal);
                Ok::<(), darkfi::Error>(())
            })
        });

    result
}
