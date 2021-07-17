use std::net::SocketAddr;
use std::sync::Arc;

use drk::blockchain::{rocks::columns, Rocks, RocksColumn};
use drk::cli::{GatewaydCli, GatewaydConfig};
use drk::service::GatewayService;
use drk::util::join_config_path;
use drk::Result;
use std::path::{Path, PathBuf};

extern crate clap;
use async_executor::Executor;
use easy_parallel::Parallel;

async fn start(executor: Arc<Executor<'_>>, config: Arc<&GatewaydConfig>) -> Result<()> {
    let accept_addr: SocketAddr = config.accept_url.parse()?;
    let pub_addr: SocketAddr = config.publisher_url.parse()?;
    let database_path = config.database_path.clone();
    let database_path = join_config_path(&PathBuf::from(database_path))?;

    let rocks = Rocks::new(&database_path)?;
    let rocks_slabstore_column = RocksColumn::<columns::Slabs>::new(rocks);

    let gateway = GatewayService::new(accept_addr, pub_addr, rocks_slabstore_column)?;

    gateway.start(executor.clone()).await?;
    Ok(())
}

fn main() -> Result<()> {
    use simplelog::*;

    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = async_channel::unbounded::<()>();

    let path = join_config_path(&PathBuf::from("gatewayd.toml")).unwrap();

    let config: GatewaydConfig = if Path::new(&path).exists() {
        GatewaydConfig::load(path)?
    } else {
        GatewaydConfig::load_default(path)?
    };

    let config_ptr = Arc::new(&config);

    let options = GatewaydCli::load()?;

    let logger_config = ConfigBuilder::new().set_time_format_str("%T%.6f").build();

    let debug_level = if options.verbose {
        LevelFilter::Debug
    } else {
        LevelFilter::Off
    };

    let log_path = config.log_path.clone();
    CombinedLogger::init(vec![
        TermLogger::new(debug_level, logger_config, TerminalMode::Mixed).unwrap(),
        WriteLogger::new(
            LevelFilter::Debug,
            Config::default(),
            std::fs::File::create(log_path).unwrap(),
        ),
    ])
    .unwrap();

    let ex2 = ex.clone();

    let (_, result) = Parallel::new()
        // Run four executor threads.
        .each(0..3, |_| smol::future::block_on(ex.run(shutdown.recv())))
        // Run the main future on the current thread.
        .finish(|| {
            smol::future::block_on(async move {
                start(ex2, config_ptr).await?;
                drop(signal);
                Ok::<(), drk::Error>(())
            })
        });

    result
}
