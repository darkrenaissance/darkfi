use drk::{
    blockchain::Rocks,
    cli::{CashierdConfig, Config},
    client::Client,
    rpc::{
        jsonrpc::{error as jsonerr, response as jsonresp},
        jsonrpc::{ErrorCode::*, JsonRequest, JsonResult},
        rpcserver::{listen_and_serve, RequestHandler, RpcServerConfig},
    },
    serial::{deserialize, serialize},
    service::{bridge, bridge::Bridge},
    util::join_config_path,
    wallet::{CashierDb, WalletDb},
    Error, Result,
};

use clap::clap_app;
use log::*;
use serde_json::{json, Value};

use async_executor::Executor;
use ff::Field;
use rand::rngs::OsRng;

use async_std::sync::{Arc, Mutex};
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Clone)]
struct Cashierd {
    verbose: bool,
    config: CashierdConfig,
    bridge: Arc<Bridge>,
    cashier_wallet: Arc<CashierDb>,
    features: HashMap<String, String>,
    client: Arc<Mutex<Client>>,
    executor: Arc<Executor<'static>>,
}

#[async_trait]
impl RequestHandler for Cashierd {
    // TODO: ServerError codes should be part of the lib.
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        if req.params.as_array().is_none() {
            return JsonResult::Err(jsonerr(InvalidParams, None, req.id));
        }

        debug!(target: "RPC", "--> {:#?}", serde_json::to_string(&req).unwrap());

        match req.method.as_str() {
            Some("deposit") => return self.deposit(req.id, req.params).await,
            Some("withdraw") => return self.withdraw(req.id, req.params).await,
            Some("features") => return self.features(req.id, req.params).await,
            Some(_) => {}
            None => {}
        };

        return JsonResult::Err(jsonerr(MethodNotFound, None, req.id));
    }
}

impl Cashierd {
    fn new(verbose: bool, executor: Arc<Executor<'static>>, config_path: PathBuf) -> Result<Self> {
        let mint_params_path = join_config_path(&PathBuf::from("cashier_mint.params"))?;
        let spend_params_path = join_config_path(&PathBuf::from("cashier_spend.params"))?;

        let config: CashierdConfig = Config::<CashierdConfig>::load(config_path)?;

        let cashier_wallet_path = join_config_path(&PathBuf::from("cashier_wallet.db"))?;

        let client_wallet_path = join_config_path(&PathBuf::from("cashier_client_wallet.db"))?;

        let cashier_wallet = CashierDb::new(&cashier_wallet_path, config.password.clone())?;
        let client_wallet = WalletDb::new(&client_wallet_path.clone(), config.password.clone())?;

        let database_path = join_config_path(&PathBuf::from("cashier_database.db"))?;

        let rocks = Rocks::new(&database_path)?;

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

        let mut features = HashMap::new();

        for network in config.clone().networks {
            features.insert(network.name, network.blockchain);
        }

        let bridge = bridge::Bridge::new();

        Ok(Self {
            verbose,
            config: config.clone(),
            bridge,
            cashier_wallet,
            features,
            client: client.clone(),
            executor: executor.clone(),
        })
    }

    async fn start(&self) -> Result<()> {
        self.cashier_wallet.init_db()?;

        let bridge = Bridge::new();

        for (feature_name, _) in self.features.iter() {
            match feature_name.as_str() {
                #[cfg(feature = "sol")]
                "sol" | "solana" => {
                    debug!(target: "CASHIER DAEMON", "Add sol network");
                    use drk::service::SolClient;
                    use solana_sdk::signer::keypair::Keypair;
                    let main_keypari = Keypair::new();
                    let _sol_client = SolClient::new(serialize(&main_keypari));
                    //bridge.add_clients(sol_client).await?;
                }
                #[cfg(feature = "btc")]
                "btc" | "bitcoin" => {
                    debug!(target: "CASHIER DAEMON", "Add btc network");
                    let btc_endpoint: (bitcoin::network::constants::Network, String) = (
                        bitcoin::network::constants::Network::Bitcoin,
                        String::from("ssl://blockstream.info:993"),
                    );
                    use drk::service::btc::BtcClient;
                    let _btc_client = BtcClient::new(btc_endpoint);
                    //bridge.add_clients(sol_client).await?;
                }
                _ => {
                    warn!("No feature enabled for {} network", feature_name);
                }
            }
        }

        self.client.lock().await.start().await?;

        let (notify, recv_coin) = async_channel::unbounded::<(jubjub::SubgroupPoint, u64)>();
        let cashier_client_subscriber_task =
            self.executor
                .spawn(Client::connect_to_subscriber_from_cashier(
                    self.client.clone(),
                    self.executor.clone(),
                    self.cashier_wallet.clone(),
                    notify.clone(),
                ));

        let cashier_wallet = self.cashier_wallet.clone();
        let ex = self.executor.clone();
        let listen_for_receiving_coins_task = self.executor.spawn(async move {
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
        });

        let cfg = RpcServerConfig {
            socket_addr: self.config.clone().rpc_url,
            use_tls: self.config.use_tls,
            identity_path: self.config.clone().tls_identity_path,
            identity_pass: self.config.clone().tls_identity_password,
        };

        listen_and_serve(cfg, self.clone()).await?;

        listen_for_receiving_coins_task.cancel().await;
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
                        cashier_wallet.confirm_withdraw_key_record(&addr, &asset_id)?;
                    }
                    _ => {}
                }
            }
        }

        Ok(())
    }

    async fn deposit(&self, id: Value, params: Value) -> JsonResult {
        debug!(target: "CASHIER DAEMON", "RECEIVED DEPOSIT REQUEST");

        let args: &Vec<serde_json::Value> = params.as_array().unwrap();

        if args.len() != 3 {
            return JsonResult::Err(jsonerr(InvalidParams, None, id));
        }

        let network = &args[0].as_str().unwrap();
        let token_id = &args[1];
        let drk_pub_key = &args[2].as_str().unwrap();

        if !self.features.contains_key(network.clone()) {
            return JsonResult::Err(jsonerr(
                InvalidParams,
                Some(format!("Cashier doesn't support this network: {}", network)),
                id,
            ));
        }

        let result: Result<String> = async {
            let asset_id = drk::util::parse_id(token_id)?;

            let drk_pub_key = bs58::decode(&drk_pub_key).into_vec()?;
            let drk_pub_key: jubjub::SubgroupPoint = deserialize(&drk_pub_key)?;

            // TODO check if the drk public key is already exist
            let _check = self
                .cashier_wallet
                .get_deposit_token_keys_by_dkey_public(&drk_pub_key, &asset_id)?;

            let bridge = self.bridge.clone();
            let bridge_subscribtion = bridge.subscribe(self.executor.clone()).await;

            bridge_subscribtion
                .sender
                .send(bridge::BridgeRequests {
                    asset_id,
                    payload: bridge::BridgeRequestsPayload::WatchRequest,
                })
                .await?;

            let bridge_res = bridge_subscribtion.receiver.recv().await?;

            match bridge_res.payload {
                bridge::BridgeResponsePayload::WatchResponse(token_priv, token_pub) => {
                    // add pairings to db
                    self.cashier_wallet.put_exchange_keys(
                        &drk_pub_key,
                        &token_priv,
                        &token_pub,
                        &asset_id,
                    )?;

                    return Ok(String::new());
                }
                _ => Err(Error::BridgeError(
                    "Receive unknown value from Subscription".into(),
                )),
            }
        }
        .await;

        match result {
            Ok(res) => JsonResult::Resp(jsonresp(json!(res), json!(id))),
            Err(err) => JsonResult::Err(jsonerr(InternalError, Some(err.to_string()), json!(id))),
        }
    }

    async fn withdraw(&self, id: Value, params: Value) -> JsonResult {
        debug!(target: "CASHIER DAEMON", "RECEIVED DEPOSIT REQUEST");

        let args: &Vec<serde_json::Value> = params.as_array().unwrap();

        if args.len() != 4 {
            return JsonResult::Err(jsonerr(InvalidParams, None, id));
        }

        let network = &args[0].as_str().unwrap();
        let token = &args[1];
        let address = &args[2].as_str().unwrap();
        let _amount = &args[3];

        if !self.features.contains_key(network.clone()) {
            return JsonResult::Err(jsonerr(
                InvalidParams,
                Some(format!("Cashier doesn't support this network: {}", network)),
                id,
            ));
        }

        let result: Result<String> = async {
            let asset_id = drk::util::parse_id(&token)?;
            let address = serialize(&address.to_string());

            let cashier_public: jubjub::SubgroupPoint;

            if let Some(addr) = self
                .cashier_wallet
                .get_withdraw_keys_by_token_public_key(&address, &asset_id)?
            {
                cashier_public = addr.public;
            } else {
                let cashier_secret = jubjub::Fr::random(&mut OsRng);
                cashier_public =
                    zcash_primitives::constants::SPENDING_KEY_GENERATOR * cashier_secret;

                self.cashier_wallet.put_withdraw_keys(
                    &address,
                    &cashier_public,
                    &cashier_secret,
                    &asset_id,
                )?;
            }

            let cashier_public_str = bs58::encode(serialize(&cashier_public)).into_string();
            Ok(cashier_public_str)
        }
        .await;

        match result {
            Ok(res) => JsonResult::Resp(jsonresp(json!(res), json!(id))),
            Err(err) => JsonResult::Err(jsonerr(InternalError, Some(err.to_string()), json!(id))),
        }
    }

    async fn features(&self, id: Value, _params: Value) -> JsonResult {
        JsonResult::Resp(jsonresp(json!(self.features), id))
    }
}

#[async_std::main]
async fn main() -> Result<()> {
    let args = clap_app!(cashierd =>
        (@arg CONFIG: -c --config +takes_value "Sets a custom config file")
        (@arg verbose: -v --verbose "Increase verbosity")
    )
    .get_matches();

    let config_path = if args.is_present("CONFIG") {
        PathBuf::from(args.value_of("CONFIG").unwrap())
    } else {
        join_config_path(&PathBuf::from("cashierd.toml"))?
    };

    let loglevel = if args.is_present("verbose") {
        log::Level::Debug
    } else {
        log::Level::Info
    };

    simple_logger::init_with_level(loglevel)?;
    let ex = Arc::new(Executor::new());
    let cashierd = Cashierd::new(args.clone().is_present("verbose"), ex.clone(), config_path)?;
    cashierd.start().await
}
