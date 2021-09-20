use async_std::sync::Arc;
use std::net::SocketAddr;

use std::path::PathBuf;

use blake2b_simd::Params;
use drk::cli::{CashierdCli, CashierdConfig, Config};
use drk::serial::{deserialize, serialize};
use drk::service::CashierService;
use drk::util::join_config_path;
use drk::wallet::{CashierDb, WalletDb};
use drk::{Error, Result};
use serde::{Deserialize, Serialize};

use async_executor::Executor;
use easy_parallel::Parallel;

// TODO: this will be replaced by a vector of assets that can be updated at runtime
#[derive(Deserialize, Serialize, Debug)]
pub struct Asset {
    pub name: String,
    pub id: Vec<u8>,
}

impl Asset {
    pub fn new(name: String) -> Self {
        let id = Self::id_hash(&name);
        Self { name, id }
    }
    pub fn id_hash(name: &String) -> Vec<u8> {
        let mut hasher = Params::new().hash_length(64).to_state();
        hasher.update(name.as_bytes());
        let result = hasher.finalize();
        let hash = jubjub::Fr::from_bytes_wide(result.as_array());
        let id = serialize(&hash);
        id
    }
}

async fn start(executor: Arc<Executor<'_>>, config: Arc<CashierdConfig>) -> Result<()> {
    let ex = executor.clone();
    let accept_addr: SocketAddr = config.accept_url.parse()?;

    let gateway_addr: SocketAddr = config.gateway_url.parse()?;

    let database_path = join_config_path(&PathBuf::from("cashier_client_database.db"))?;

    let cashierdb = join_config_path(&PathBuf::from("cashier.db"))?;
    let client_wallet = join_config_path(&PathBuf::from("cashier_client_walletdb.db"))?;

    let wallet = CashierDb::new(
        &cashierdb.clone(),
        config.password.clone(),
    )?;

    let client_wallet = WalletDb::new(
        &client_wallet.clone(),
        config.client_password.clone(),
    )?;

    let mint_params_path = join_config_path(&PathBuf::from("cashier_mint.params"))?;
    let spend_params_path = join_config_path(&PathBuf::from("cashier_spend.params"))?;

    let mut cashier = CashierService::new(
        accept_addr,
        wallet.clone(),
        client_wallet.clone(),
        database_path,
        (gateway_addr, "127.0.0.1:4444".parse()?),
        (mint_params_path, spend_params_path),
    )
    .await?;

    // TODO: make this a vector of accepted assets
    let asset = Asset::new("btc".to_string());
    // TODO: this should be done by the user
    let asset_id = deserialize(&asset.id)?;

    // TODO: pass vector of assets into cashier.start()
    cashier.start(ex.clone(), asset_id).await?;

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
