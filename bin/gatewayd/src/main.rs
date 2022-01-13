use std::{path::PathBuf, sync::Arc};

use async_executor::Executor;
use clap::{IntoApp, Parser};
use easy_parallel::Parallel;
use log::debug;
use simplelog::{ColorChoice, LevelFilter, TermLogger, TerminalMode};

use darkfi::{
    chain::{rocks::columns, Rocks, RocksColumn},
    cli::{CliGatewayd, Config, GatewaydConfig},
    node::service::gateway::GatewayService,
    util::{expand_path, join_config_path},
    Result,
};

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

    let mut verbosity_level = 0;
    verbosity_level += matches.occurrences_of("verbose");
    let loglevel = match verbosity_level {
        0 => LevelFilter::Info,
        1 => LevelFilter::Debug,
        _ => LevelFilter::Trace,
    };

    TermLogger::init(
        loglevel,
        simplelog::Config::default(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    )?;

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
