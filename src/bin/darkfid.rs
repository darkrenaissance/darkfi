use drk::blockchain::Rocks;
use drk::cli::{Config, DarkfidCli, DarkfidConfig};
use drk::util::join_config_path;
use drk::wallet::WalletDb;
use drk::Result;

use drk::client::Client;

use async_executor::Executor;
use easy_parallel::Parallel;

use async_std::sync::Arc;
use std::net::SocketAddr;
use std::path::PathBuf;

async fn start(executor: Arc<Executor<'_>>, config: Arc<DarkfidConfig>) -> Result<()> {
    let connect_addr: SocketAddr = config.connect_url.parse()?;
    let sub_addr: SocketAddr = config.subscriber_url.parse()?;
    let cashier_addr: SocketAddr = config.cashier_url.parse()?;
    let database_path = config.database_path.clone();
    let walletdb_path = config.walletdb_path.clone();
    let rpc_url: std::net::SocketAddr = config.rpc_url.parse()?;

    let database_path = join_config_path(&PathBuf::from(database_path))?;
    let walletdb_path = join_config_path(&PathBuf::from(walletdb_path))?;

    let rocks = Rocks::new(&database_path)?;

    let wallet = WalletDb::new(&walletdb_path, config.password.clone())?;

    let mint_params_path = join_config_path(&PathBuf::from("mint.params"))?;
    let spend_params_path = join_config_path(&PathBuf::from("spend.params"))?;

    if let Err(_) = wallet.get_private_keys() {
        wallet.init_db()?;
        wallet.key_gen()?;
    }

    let mut client = Client::new(
        rocks,
        (connect_addr, sub_addr),
        (mint_params_path, spend_params_path),
        wallet.clone(),
    )?;

    client.start().await?;

    Client::connect_to_cashier(
        client,
        executor.clone(),
        cashier_addr.clone(),
        rpc_url.clone(),
    )
    .await?;

    Ok(())
}

fn main() -> Result<()> {
    let options = Arc::new(DarkfidCli::load()?);

    let config_path: PathBuf;

    match options.config.as_ref() {
        Some(path) => {
            config_path = path.to_owned();
        }
        None => {
            config_path = join_config_path(&PathBuf::from("darkfid.toml"))?;
        }
    }

    let config: DarkfidConfig = Config::<DarkfidConfig>::load(config_path)?;

    let config = Arc::new(config);

    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = async_channel::unbounded::<()>();

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
                Ok::<(), drk::Error>(())
            })
        });

    result
}
