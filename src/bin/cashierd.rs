use async_executor::Executor;
use async_std::sync::{Arc, Mutex};
use async_trait::async_trait;
use clap::clap_app;
use ff::Field;
use log::{debug, warn};
use rand::rngs::OsRng;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;

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
    util::{expand_path, join_config_path},
    wallet::{CashierDb, WalletDb},
    Error, Result,
};

fn handle_bridge_error(error_code: u32) -> Result<()> {
    match error_code {
        1 => Err(Error::BridgeError("Not Supported Client".into())),
        _ => Err(Error::BridgeError("Unknown error_code".into())),
    }
}

#[derive(Clone)]
struct Cashierd {
    config: CashierdConfig,
    bridge: Arc<Bridge>,
    cashier_wallet: Arc<CashierDb>,
    features: HashMap<String, String>,
    client: Arc<Mutex<Client>>,
}

#[async_trait]
impl RequestHandler for Cashierd {
    // TODO: ServerError codes should be part of the lib.
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        if req.params.as_array().is_none() {
            return JsonResult::Err(jsonerr(InvalidParams, None, req.id));
        }

        debug!(target: "RPC", "--> {}", serde_json::to_string(&req).unwrap());

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
    fn new(config_path: PathBuf) -> Result<Self> {
        let config: CashierdConfig = Config::<CashierdConfig>::load(config_path)?;

        let cashier_wallet = CashierDb::new(
            expand_path(&config.cashier_wallet_path.clone())?.as_path(),
            config.cashier_wallet_password.clone(),
        )?;

        let client_wallet = WalletDb::new(
            expand_path(&config.client_wallet_path.clone())?.as_path(),
            config.client_wallet_password.clone(),
        )?;

        let rocks = Rocks::new(expand_path(&config.database_path.clone())?.as_path())?;

        let client = Client::new(
            rocks,
            (
                config.gateway_protocol_url.parse()?,
                config.gateway_publisher_url.parse()?,
            ),
            (
                expand_path(&config.mint_params_path.clone())?,
                expand_path(&config.spend_params_path.clone())?,
            ),
            client_wallet.clone(),
        )?;

        let client = Arc::new(Mutex::new(client));

        let mut features = HashMap::new();

        for network in config.clone().networks {
            features.insert(network.name, network.blockchain);
        }

        let bridge = bridge::Bridge::new();

        Ok(Self {
            config: config.clone(),
            bridge,
            cashier_wallet,
            features,
            client: client.clone(),
        })
    }

    async fn start(&self, executor: Arc<Executor<'static>>) -> Result<()> {
        self.cashier_wallet.init_db().await?;


        for (feature_name, _) in self.features.iter() {
            let bridge2 = self.bridge.clone();
            match feature_name.as_str() {
                #[cfg(feature = "sol")]
                "sol" | "solana" => {
                    debug!(target: "CASHIER DAEMON", "Add sol network");
                    use drk::service::SolClient;
                    use solana_sdk::signer::keypair::Keypair;
                    let main_keypari = Keypair::new();
                    let sol_client = SolClient::new(serialize(&main_keypari)).await?;
                    bridge2.add_clients("sol".into(), sol_client).await?;
                }
                #[cfg(feature = "btc")]
                "btc" | "bitcoin" => {
                    debug!(target: "CASHIER DAEMON", "Add btc network");
                    let btc_endpoint: (bitcoin::network::constants::Network, String) = (
                        bitcoin::network::constants::Network::Bitcoin,
                        String::from("ssl://blockstream.info:993"),
                    );
                    use drk::service::btc::BtcClient;
                    let _btc_client = BtcClient::new(btc_endpoint)?;
                    //bridge2.add_clients("btc".into(), btc_client).await?;
                }
                _ => {
                    warn!("No feature enabled for {} network", feature_name);
                }
            }
        }

        self.client.lock().await.start().await?;

        let (notify, recv_coin) = async_channel::unbounded::<(jubjub::SubgroupPoint, u64)>();
        let cashier_client_subscriber_task =
            smol::spawn(Client::connect_to_subscriber_from_cashier(
                self.client.clone(),
                executor.clone(),
                self.cashier_wallet.clone(),
                notify.clone(),
            ));

        let cashier_wallet = self.cashier_wallet.clone();
        let bridge = self.bridge.clone();
        let listen_for_receiving_coins_task = smol::spawn(async move {
            loop {
                Self::listen_for_receiving_coins(
                    bridge.clone(),
                    cashier_wallet.clone(),
                    recv_coin.clone(),
                )
                .await
                .expect(" listen for receiving coins");
            }
        });

        let cfg = RpcServerConfig {
            socket_addr: self.config.rpc_listen_address.clone(),
            use_tls: self.config.serve_tls,
            identity_path: expand_path(&self.config.clone().tls_identity_path)?,
            identity_pass: self.config.tls_identity_password.clone(),
        };

        listen_and_serve(cfg, self.clone()).await?;

        listen_for_receiving_coins_task.cancel().await;
        cashier_client_subscriber_task.cancel().await;
        Ok(())
    }

    async fn listen_for_receiving_coins(
        bridge: Arc<Bridge>,
        cashier_wallet: Arc<CashierDb>,
        recv_coin: async_channel::Receiver<(jubjub::SubgroupPoint, u64)>,
    ) -> Result<()> {
        let bridge_subscribtion = bridge.subscribe().await;

        // received drk coin
        let (drk_pub_key, amount) = recv_coin.recv().await?;

        debug!(target: "CASHIER DAEMON", "Receive coin with following address and amount: {}, {}"
            , drk_pub_key, amount);

        // get public key, and asset_id of the token
        let token = cashier_wallet.get_withdraw_token_public_key_by_dkey_public(&drk_pub_key)?;

        // send a request to bridge to send equivalent amount of
        // received drk coin to token publickey
        if let Some((addr, network, asset_id)) = token {
            bridge_subscribtion
                .sender
                .send(bridge::BridgeRequests {
                    network: network.to_string(),
                    asset_id,
                    payload: bridge::BridgeRequestsPayload::SendRequest(addr.clone(), amount),
                })
                .await?;

            // receive a response
            let res = bridge_subscribtion.receiver.recv().await?;

            let error_code = res.error as u32;
            if error_code == 0 {
                match res.payload {
                    bridge::BridgeResponsePayload::SendResponse => {
                        // TODO Send the received coins to the main address
                        cashier_wallet.confirm_withdraw_key_record(&addr, &asset_id)?;
                    }
                    _ => {}
                }
            } else {
                return handle_bridge_error(error_code);
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
        let network = network.to_string();
        let token_id = &args[1];
        let drk_pub_key = &args[2].as_str().unwrap();

        if !self.features.contains_key(&network.clone()) {
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
            let bridge_subscribtion = bridge.subscribe().await;

            bridge_subscribtion
                .sender
                .send(bridge::BridgeRequests {
                    network: network.clone(),
                    asset_id,
                    payload: bridge::BridgeRequestsPayload::WatchRequest,
                })
                .await?;

            let bridge_res = bridge_subscribtion.receiver.recv().await?;

            let error_code = bridge_res.error as u32;

            if error_code != 0 {
                return handle_bridge_error(error_code).map(|_| String::new());
            }

            match bridge_res.payload {
                bridge::BridgeResponsePayload::WatchResponse(token_priv, token_pub) => {
                    // add pairings to db
                    self.cashier_wallet.put_exchange_keys(
                        &drk_pub_key,
                        &token_priv,
                        &serialize(&token_pub),
                        &network,
                        &asset_id,
                    )?;

                    return Ok(token_pub);
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
                    &network.to_string(),
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
    let cashierd = Cashierd::new(config_path)?;
    cashierd.start(ex.clone()).await
}
