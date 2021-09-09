use async_std::sync::Arc;
use std::net::SocketAddr;

use std::path::PathBuf;

use drk::cli::{CashierdCli, CashierdConfig, Config};
use drk::service::CashierService;
use drk::util::join_config_path;
use drk::wallet::{CashierDb, WalletDb};
use drk::{Error, Result};

use async_executor::Executor;
use easy_parallel::Parallel;

async fn start(executor: Arc<Executor<'_>>, config: Arc<CashierdConfig>) -> Result<()> {
    let ex = executor.clone();
    let accept_addr: SocketAddr = config.accept_url.parse()?;

    let gateway_addr: SocketAddr = config.gateway_url.parse()?;

    let btc_endpoint: String = config.btc_endpoint.clone();

    let database_path = config.client_database_path.clone();
    let database_path = join_config_path(&PathBuf::from(database_path))?;

    let wallet = CashierDb::new(
        &PathBuf::from(&config.cashierdb_path),
        config.password.clone(),
    )?;

    let client_wallet = WalletDb::new(
        &PathBuf::from(&config.client_walletdb_path),
        config.client_password.clone(),
    )?;

    let mint_params_path = join_config_path(&PathBuf::from("cashier_mint.params"))?;
    let spend_params_path = join_config_path(&PathBuf::from("cashier_spend.params"))?;

    let mut cashier = CashierService::new(
        accept_addr,
        btc_endpoint,
        wallet.clone(),
        client_wallet.clone(),
        database_path,
        (gateway_addr, "127.0.0.1:4444".parse()?),
        (mint_params_path, spend_params_path),
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

    let config: CashierdConfig = Config::<CashierdConfig>::load(path)?;

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
