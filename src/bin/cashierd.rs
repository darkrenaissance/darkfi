use async_std::sync::Arc;
use std::net::SocketAddr;

use std::{path::Path, path::PathBuf};

use drk::cli::{CashierdCli, CashierdConfig, Config};
use drk::service::CashierService;
use drk::util::join_config_path;
use drk::wallet::CashierDb;
use drk::{Error, Result};
use log::*;

use async_executor::Executor;
use easy_parallel::Parallel;

async fn start(executor: Arc<Executor<'_>>, config: Arc<CashierdConfig>) -> Result<()> {
    let ex = executor.clone();
    let accept_addr: SocketAddr = config.accept_url.parse()?;

    let gateway_addr: SocketAddr = config.gateway_url.parse()?;

    let btc_endpoint: String = config.btc_endpoint.clone();

    let database_path = config.database_path.clone();
    let database_path = join_config_path(&PathBuf::from(database_path))?;

    let wallet = Arc::new(CashierDb::new("cashier.db", config.password.clone())?);

    // TODO add to config
    let client_wallet_path = "cashier_client_wallet.db";

    debug!(target: "cashierd", "starting cashier service");
    let cashier = CashierService::new(
        accept_addr,
        btc_endpoint,
        wallet.clone(),
        database_path,
        (gateway_addr, "127.0.0.1:4444".parse()?),
        (
            PathBuf::from("cashier_mint.params"),
            PathBuf::from("cashier_spend.params"),
        ),
        PathBuf::from(client_wallet_path),
    )
    .await?;

    cashier.start(ex.clone()).await?;

    //let rpc_url: std::net::SocketAddr = config.rpc_url.parse()?;
    //let adapter = Arc::new(CashierAdapter::new(wallet.clone())?);
    //let io = Arc::new(adapter.handle_input()?);
    //jsonserver::start(ex, rpc_url, io).await?;

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

    let config = Arc::new(config);

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
                start(ex2, config).await?;
                drop(signal);
                Ok::<(), Error>(())
            })
        });

    result
}
