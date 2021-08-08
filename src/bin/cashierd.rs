use std::net::SocketAddr;
use std::sync::Arc;

use std::{path::Path, path::PathBuf};
//use toml;

use drk::cli::{CashierdCli, CashierdConfig, Config};
use drk::service::CashierService;
use drk::wallet::CashierDb;

use drk::util::join_config_path;
use drk::Result;

use async_executor::Executor;
use easy_parallel::Parallel;

async fn start(executor: Arc<Executor<'_>>, config: Arc<&CashierdConfig>) -> Result<()> {
    let accept_addr: SocketAddr = config.accept_url.parse()?;

    let wallet = Arc::new(CashierDb::new("cashier.db", config.password.clone())?);

    let cashier = CashierService::new(accept_addr, wallet)?;

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
                Ok::<(), drk::Error>(())
            })
        });

    result
}
