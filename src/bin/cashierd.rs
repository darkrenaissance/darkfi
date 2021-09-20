use drk::{
    blockchain::Rocks,
    cli::{CashierdConfig, Config},
    client::Client,
    rpc::{
        jsonrpc::{error as jsonerr, response as jsonresp},
        jsonrpc::{ErrorCode::*, JsonRequest, JsonResult},
    },
    serial::{deserialize, serialize},
    service::{bridge, bridge::Bridge},
    util::join_config_path,
    wallet::{CashierDb, WalletDb},
    Error, Result,
};

use clap::clap_app;
use log::*;
use serde::Serialize;
use serde_json::{json, Value};
use simplelog::{
    CombinedLogger, Config as SimLogConfig, ConfigBuilder, LevelFilter, TermLogger, TerminalMode,
    WriteLogger,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use async_executor::Executor;
use easy_parallel::Parallel;

use async_std::sync::{Arc, Mutex};
use ff::PrimeField;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize)]
struct Features {
    networks: Vec<String>,
}

impl Features {
    fn new() -> Self {
        let mut networks = Vec::new();
        networks.push("solana".to_string());
        networks.push("bitcoin".to_string());
        Self { networks }
    }
}

#[derive(Clone)]
struct Cashierd {
    verbose: bool,
    config: CashierdConfig,
    client_wallet: Arc<WalletDb>,
    cashier_wallet: Arc<CashierDb>,
    features: Features,
    client: Arc<Mutex<Client>>,
}

impl Cashierd {
    fn new(verbose: bool, config_path: PathBuf) -> Result<Self> {
        let mint_params_path = join_config_path(&PathBuf::from("cashier_mint.params"))?;
        let spend_params_path = join_config_path(&PathBuf::from("cashier_spend.params"))?;

        let config: CashierdConfig = Config::<CashierdConfig>::load(config_path)?;

        let cashier_wallet = CashierDb::new(
            &PathBuf::from(config.cashierdb_path.clone()),
            config.password.clone(),
        )?;
        let client_wallet = WalletDb::new(
            &PathBuf::from(config.cashierdb_path.clone()),
            config.password.clone(),
        )?;

        let rocks = Rocks::new(&PathBuf::from(&config.cashierdb_path))?;

        let client = Client::new(
            rocks,
            (
                config.gateway_url.parse()?,
                config.gateway_subscriber_url.parse()?,
            ),
            (mint_params_path, spend_params_path),
            client_wallet.clone(),
        )?;

        let client = Arc::new(Mutex::new(client));

        let features = Features::new();

        Ok(Self {
            verbose,
            config: config.clone(),
            cashier_wallet,
            client_wallet,
            features,
            client: client.clone(),
        })
    }

    async fn start(&self, executor: Arc<Executor<'_>>) -> Result<()> {
        //// TODO: pass vector of assets

        self.cashier_wallet.init_db()?;

        let bridge = Bridge::new();

        self.client.lock().await.start().await?;

        let (notify, recv_coin) = async_channel::unbounded::<(jubjub::SubgroupPoint, u64)>();

        let cashier_client_subscriber_task =
            executor.spawn(Client::connect_to_subscriber_from_cashier(
                self.client.clone(),
                executor.clone(),
                self.cashier_wallet.clone(),
                notify.clone(),
            ));

        let cashier_wallet = self.cashier_wallet.clone();

        let ex = executor.clone();
        executor
            .spawn(async move {
                loop {
                    Self::listen_for_receiving_coins(
                        ex.clone(),
                        bridge.clone(),
                        cashier_wallet.clone(),
                        recv_coin.clone(),
                    )
                    .await
                    .expect(" listen for receiving coins");
                }
            })
            .await;

        cashier_client_subscriber_task.cancel().await;

        Ok(())
    }

    async fn listen_for_receiving_coins(
        ex: Arc<Executor<'_>>,
        bridge: Arc<Bridge>,
        cashier_wallet: Arc<CashierDb>,
        recv_coin: async_channel::Receiver<(jubjub::SubgroupPoint, u64)>,
    ) -> Result<()> {
        let bridge_subscribtion = bridge.subscribe(ex.clone()).await;

        // received drk coin
        let (drk_pub_key, amount) = recv_coin.recv().await?;

        debug!(target: "CASHIER DAEMON", "Receive coin with following address and amount: {}, {}"
            , drk_pub_key, amount);

        // get public key, and asset_id of the token
        let token = cashier_wallet.get_withdraw_token_public_key_by_dkey_public(&drk_pub_key)?;

        // send a request to bridge to send equivalent amount of
        // received drk coin to token publickey
        if let Some((addr, asset_id)) = token {
            bridge_subscribtion
                .sender
                .send(bridge::BridgeRequests {
                    asset_id,
                    payload: bridge::BridgeRequestsPayload::SendRequest(addr.clone(), amount),
                })
                .await?;

            // receive a response
            let res = bridge_subscribtion.receiver.recv().await?;

            if res.error == 0 {
                match res.payload {
                    bridge::BridgeResponsePayload::SendResponse => {
                        // TODO Send the received coins to the main address
                        cashier_wallet.confirm_withdraw_key_record(&addr, &serialize(&1))?;
                    }
                    _ => {}
                }
            }
        }

        Ok(())
    }

    async fn handle_request(self, req: JsonRequest) -> JsonResult {
        if req.params.as_array().is_none() {
            return JsonResult::Err(jsonerr(InvalidParams, None, req.id));
        }

        debug!(target: "RPC", "--> {:#?}", serde_json::to_string(&req).unwrap());

        // TODO: "features"
        match req.method.as_str() {
            Some("deposit") => return self.deposit(req.id, req.params).await,
            Some("withdraw") => return self.withdraw(req.id, req.params).await,
            Some("features") => return self.features(req.id, req.params).await,
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

        let _ntwk = &args[0];
        let tkn = &args[1];
        let pk = &args[2];

        debug!(target: "CASHIER", "PROCESSING INPUT");

        if tkn.as_str().is_none() {
            return JsonResult::Err(jsonerr(InvalidParams, None, id));
        }
        let tkn_str = tkn.as_str().unwrap();

        let _tkn_fr = jubjub::Fr::from_str(tkn_str);
        // TODO: debug this
        //if tkn_fr.is_none() {
        //    return JsonResult::Err(jsonerr(InvalidParams, None, id));
        //};
        //let token = tkn_fr.unwrap();

        if pk.as_str().is_none() {
            return JsonResult::Err(jsonerr(InvalidParams, None, id));
        }
        let pk_str = pk.as_str().unwrap();

        let pk_58 = bs58::decode(pk_str).into_vec().unwrap();

        let pubkey: jubjub::SubgroupPoint = deserialize(&pk_58).unwrap();

        //// TODO: Sanity check.
        let _check = self
            .cashier_wallet
            .get_deposit_token_keys_by_dkey_public(&pubkey, &serialize(&1));

        // TODO: implement bridge communication
        // this just returns the user public key
        let pubkey = bs58::encode(serialize(&pubkey)).into_string();
        debug!(target: "CASHIER", "ATTEMPING REPLY");
        JsonResult::Resp(jsonresp(json!(pubkey), json!(id)))
    }

    async fn withdraw(self, id: Value, params: Value) -> JsonResult {
        debug!(target: "CASHIER", "RECEIVED DEPOSIT REQUEST");

        let args = params.as_array().unwrap();

        let _network = &args[0];
        let _token = &args[1];
        let _address = &args[2];
        let _amount = &args[3];

        // 2. Cashier checks if they support the network, and if so,
        //    return adeposit address.

        JsonResult::Err(jsonerr(InvalidParams, None, id))
    }

    // TODO: implement this
    async fn features(self, id: Value, _params: Value) -> JsonResult {
        JsonResult::Resp(jsonresp(json!(self.features), id))
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
    let (_, _result) = Parallel::new()
        // Run four executor threads.
        .each(0..3, |_| smol::future::block_on(ex.run(shutdown.recv())))
        // Run the main future on the current thread.
        .finish(|| {
            smol::future::block_on(async move {
                cashierd2.start(ex2).await?;
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
