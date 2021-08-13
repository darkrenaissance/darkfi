use std::net::SocketAddr;
use async_std::sync::Arc;

use std::{path::Path, path::PathBuf};
//use toml;

use drk::cli::{CashierdCli, CashierdConfig, Config};
use drk::service::CashierService;
use drk::service::GatewayClient;
use drk::wallet::CashierDb;
use drk::blockchain::{rocks::columns, Rocks, RocksColumn};
use drk::util::join_config_path;
use drk::{Error, Result};
use log::*;

use async_executor::Executor;
use easy_parallel::Parallel;

async fn start(executor: Arc<Executor<'_>>, config: Arc<&CashierdConfig>) -> Result<()> {
    let accept_addr: SocketAddr = config.accept_url.parse()?;

    let gateway_addr: SocketAddr = config.gateway_url.parse()?;

    let database_path = config.database_path.clone();
    let database_path = join_config_path(&PathBuf::from(database_path))?;
    let rocks = Rocks::new(&database_path)?;
    let slabstore = RocksColumn::<columns::Slabs>::new(rocks.clone());

    let wallet = Arc::new(CashierDb::new("cashier.db", config.password.clone())?);

    debug!(target: "Client", "Creating gateway client");
    let mut gateway = GatewayClient::new(gateway_addr, slabstore)?;

    debug!(target: "fn::start gateway client", "start() Gateway Client started");
    gateway.start().await?;

    // Probably not ideal to create an Arc here
    let cashier = CashierService::new(accept_addr, wallet, gateway)?;

    cashier.start(executor.clone()).await?;
    Ok(())
}

fn main() -> Result<()> {
    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = async_channel::unbounded::<()>();

    let path = join_config_path(&PathBuf::from("cashierd.toml")).unwrap();

    let config: CashierdConfig = if Path::new(&path).exists() {
        Config::<CashierdConfig>::load(path)?
    } else {
        Config::<CashierdConfig>::load_default(path)?
    };

    let config_ptr = Arc::new(&config);

    let options = CashierdCli::load()?;

    {
        use simplelog::*;
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
    }

    let ex2 = ex.clone();

    let (_, result) = Parallel::new()
        // Run four executor threads.
        .each(0..3, |_| smol::future::block_on(ex.run(shutdown.recv())))
        // Run the main future on the current thread.
        .finish(|| {
            smol::future::block_on(async move {
                start(ex2, config_ptr).await?;
                drop(signal);
                Ok::<(), Error>(())
            })
        });

    result
}
