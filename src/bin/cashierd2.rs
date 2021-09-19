use async_std::sync::Arc;
use log::*;
use std::path::PathBuf;

use clap::clap_app;
use serde_json::{json, Value};
use simplelog::{
    CombinedLogger, Config as SimLogConfig, ConfigBuilder, LevelFilter, TermLogger, TerminalMode,
    WriteLogger,
};
use std::net::SocketAddr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use async_executor::Executor;
use easy_parallel::Parallel;

use drk::{
    cli::{CashierdConfig, Config},
    rpc::{
        jsonrpc::{error as jsonerr, response as jsonresp},
        jsonrpc::{ErrorCode::*, JsonRequest, JsonResult},
    },
    serial::{deserialize, serialize},
    service::{bridge, CashierService},
    util::join_config_path,
    wallet::{CashierDb, WalletDb},
    Error, Result,
};

use ff::PrimeField;

#[derive(Clone)]
struct Cashierd {
    verbose: bool,
    config: CashierdConfig,
    client_wallet: Arc<WalletDb>,
    cashier_wallet: Arc<CashierDb>,
    // clientdb:
    // mint_params:
    // spend_params:
}

impl Cashierd {
    fn new(verbose: bool, config_path: PathBuf) -> Result<Self> {
        let config: CashierdConfig = Config::<CashierdConfig>::load(config_path)?;
        let cashier_wallet = CashierDb::new(
            &PathBuf::from(config.cashierdb_path.clone()),
            config.password.clone(),
        )?;
        let client_wallet = WalletDb::new(
            &PathBuf::from(config.cashierdb_path.clone()),
            config.password.clone(),
        )?;

        Ok(Self {
            verbose,
            config,
            cashier_wallet,
            client_wallet,
        })
    }

    async fn start(self, executor: Arc<Executor<'_>>, config: CashierdConfig) -> Result<()> {
        let ex = executor.clone();
        let accept_addr: SocketAddr = config.accept_url.parse()?;

        let gateway_addr: SocketAddr = config.gateway_url.parse()?;

        let database_path = PathBuf::from(config.cashierdb_path);

        let mint_params_path = join_config_path(&PathBuf::from("cashier_mint.params"))?;
        let spend_params_path = join_config_path(&PathBuf::from("cashier_spend.params"))?;

        let mut cashier = CashierService::new(
            accept_addr,
            self.cashier_wallet.clone(),
            self.client_wallet.clone(),
            database_path,
            (gateway_addr, "127.0.0.1:4444".parse()?),
            (mint_params_path, spend_params_path),
        )
        .await?;

        //// TODO: make this a vector of accepted assets
        //let asset = Asset::new("btc".to_string());
        //// TODO: this should be done by the user
        //let asset_id = deserialize(&asset.id)?;

        //// TODO: pass vector of assets into cashier.start()
        //cashier.start(ex.clone(), asset_id).await?;

        Ok(())
    }

    async fn handle_request(self, req: JsonRequest) -> JsonResult {
        if req.params.as_array().is_none() {
            return JsonResult::Err(jsonerr(InvalidParams, None, req.id));
        }

        debug!(target: "RPC", "--> {:#?}", serde_json::to_string(&req).unwrap());

        // TODO: "features"
        match req.method.as_str() {
            //Some("say_hello") => return self.say_hello(req.id, req.params).await,
            //Some("create_wallet") => return self.create_wallet(req.id, req.params).await,
            //Some("key_gen") => return self.key_gen(req.id, req.params).await,
            //Some("get_key") => return self.get_key(req.id, req.params).await,
            Some("deposit") => return self.deposit(req.id, req.params).await,
            //Some("withdraw") => return self.withdraw(req.id, req.params).await,
            //Some("transfer") => return self.transfer(req.id, req.params).await,
            Some(_) => {}
            None => {}
        };

        return JsonResult::Err(jsonerr(MethodNotFound, None, req.id));
    }

    async fn deposit(self, id: Value, params: Value) -> JsonResult {
        debug!(target: "CASHIER", "RECEIVED DEPOSIT REQUEST");

        if params.as_array().is_none() {
            return JsonResult::Err(jsonerr(InvalidParams, None, id));
        }

        let args = params.as_array().unwrap();

        let _network = &args[0];
        let token = &args[1];
        let pubkey = &args[2];

        debug!(target: "CASHIER", "PROCESSING INPUT");

        // parse token
        if token.as_str().is_none() {
            return JsonResult::Err(jsonerr(InvalidParams, None, id));
        }
        let token_str = token.as_str().unwrap();
        let token_fr = jubjub::Fr::from_str(token_str);
        if token_fr.is_none() {
            return JsonResult::Err(jsonerr(InvalidParams, None, id));
        };
        let token = token_fr.unwrap();

        // parse pubkey
        if pubkey.as_str().is_none() {
            return JsonResult::Err(jsonerr(InvalidParams, None, id));
        }
        let pubkey = pubkey.as_str().unwrap();
        let hex = hex::decode(pubkey);
        if hex.is_none() {
            return JsonResult::Err(jsonerr(InvalidParams, None, id));
        };
        let pubkey: jubjub::SubgroupPoint = deserialize(&hex);
        if pubkey.is_none() {
            return JsonResult::Err(jsonerr(InvalidParams, None, id));
        }

        //// TODO: Sanity check.
        debug!(target: "CASHIER", "GET DEPOSIT COIN KEYS");
        let _check = self
            .cashier_wallet
            .get_deposit_coin_keys_by_dkey_public(&pubkey, &serialize(&1));

        // TODO: implement bridge communication
        // NOTE: this just returns the user public key
        debug!(target: "CASHIER", "ATTEMPING REPLY");
        JsonResult::Resp(jsonresp(json!(pubkey), json!(id)))
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = clap_app!(cashierd =>
        (@arg CONFIG: -c --config +takes_value "Sets a custom config file")
        (@arg verbose: -v --verbose "Increase verbosity")
    )
    .get_matches();

    let config_path: PathBuf;

    if args.is_present("CONFIG") {
        config_path = PathBuf::from(args.value_of("CONFIG").unwrap());
    } else {
        config_path = join_config_path(&PathBuf::from("cashierd.toml"))?;
    }

    let cashierd = Cashierd::new(args.clone().is_present("verbose"), config_path)?;

    let listener = TcpListener::bind(cashierd.clone().config.rpc_url).await?;
    debug!(target: "RPC SERVER", "Listening on {}", cashierd.clone().config.rpc_url);

    let logger_config = ConfigBuilder::new().set_time_format_str("%T%.6f").build();
    let debug_level = if args.is_present("verbose") {
        LevelFilter::Debug
    } else {
        LevelFilter::Off
    };

    let log_path = cashierd.clone().config.log_path;
    CombinedLogger::init(vec![
        TermLogger::new(debug_level, logger_config, TerminalMode::Mixed).unwrap(),
        WriteLogger::new(
            LevelFilter::Debug,
            SimLogConfig::default(),
            std::fs::File::create(log_path).unwrap(),
        ),
    ])
    .unwrap();

    let ex = Arc::new(Executor::new());
    let ex2 = ex.clone();
    let (signal, shutdown) = async_channel::unbounded::<()>();

    let cashierd2 = cashierd.clone();
    let cashierd3 = cashierd.clone();
    let (_, _result) = Parallel::new()
        // Run four executor threads.
        .each(0..3, |_| smol::future::block_on(ex.run(shutdown.recv())))
        // Run the main future on the current thread.
        .finish(|| {
            smol::future::block_on(async move {
                cashierd2.start(ex2, cashierd3.clone().config).await?;
                drop(signal);
                Ok::<(), Error>(())
            })
        });

    loop {
        debug!(target: "RPC SERVER", "waiting for client");

        let (mut socket, _) = listener.accept().await?;

        debug!(target: "RPC SERVER", "accepted client");

        let cashierd = cashierd.clone();
        tokio::spawn(async move {
            let mut buf = [0; 2048];

            loop {
                let n = match socket.read(&mut buf).await {
                    Ok(n) if n == 0 => {
                        debug!(target: "RPC SERVER", "closed connection");
                        return;
                    }
                    Ok(n) => n,
                    Err(e) => {
                        debug!(target: "RPC SERVER", "failed to read from socket; err = {:?}", e);
                        return;
                    }
                };

                let r: JsonRequest = match serde_json::from_slice(&buf[0..n]) {
                    Ok(r) => r,
                    Err(e) => {
                        debug!(target: "RPC SERVER", "received invalid json; err = {:?}", e);
                        return;
                    }
                };

                let reply = cashierd.clone().handle_request(r).await;
                let j = serde_json::to_string(&reply).unwrap();

                debug!(target: "RPC", "<-- {:#?}", j);

                // Write the data back
                if let Err(e) = socket.write_all(j.as_bytes()).await {
                    debug!(target: "RPC SERVER", "failed to write to socket; err = {:?}", e);
                    return;
                }
            }
        });
    }
}
