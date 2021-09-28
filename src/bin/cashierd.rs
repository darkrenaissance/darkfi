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
    util::{expand_path, generate_id, join_config_path},
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

    async fn resume_watch_deposit_keys(
        bridge: Arc<Bridge>,
        cashier_wallet: Arc<CashierDb>,
        features: HashMap<String, String>,
    ) -> Result<()> {
        for (network, _) in features.iter() {
            let keypairs_to_watch = cashier_wallet.get_deposit_token_keys_by_network(&network)?;

            for keypair in keypairs_to_watch {
                let bridge = bridge.clone();
                let bridge_subscribtion = bridge.subscribe().await;
                bridge_subscribtion
                    .sender
                    .send(bridge::BridgeRequests {
                        network: network.to_owned(),
                        payload: bridge::BridgeRequestsPayload::Watch(Some((keypair.0, keypair.1))),
                    })
                    .await?;
            }
        }
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
        if let Some((addr, network, _asset_id)) = token {
            bridge_subscribtion
                .sender
                .send(bridge::BridgeRequests {
                    network: network.to_string(),
                    payload: bridge::BridgeRequestsPayload::Send(addr.clone(), amount),
                })
                .await?;

            // receive a response
            let res = bridge_subscribtion.receiver.recv().await?;

            let error_code = res.error as u32;
            if error_code == 0 {
                match res.payload {
                    bridge::BridgeResponsePayload::Send => {
                        // TODO Send the received coins to the main address
                        cashier_wallet.confirm_withdraw_key_record(&addr, &network)?;
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
        let token_id = &args[1].as_str().unwrap();
        let drk_pub_key = &args[2].as_str().unwrap();

        if !self.features.contains_key(&network.clone()) {
            return JsonResult::Err(jsonerr(
                InvalidParams,
                Some(format!("Cashier doesn't support this network: {}", network)),
                id,
            ));
        }

        let result: Result<String> = async {
            Self::check_token_id(&network, token_id)?;

            let asset_id = generate_id(token_id)?;

            let drk_pub_key = bs58::decode(&drk_pub_key).into_vec()?;
            let drk_pub_key: jubjub::SubgroupPoint = deserialize(&drk_pub_key)?;

            // check if the drk public key is already exist
            let check = self
                .cashier_wallet
                .get_deposit_token_keys_by_dkey_public(&drk_pub_key, &network)?;

            let bridge = self.bridge.clone();
            let bridge_subscribtion = bridge.subscribe().await;

            if check.is_empty() {
                bridge_subscribtion
                    .sender
                    .send(bridge::BridgeRequests {
                        network: network.clone(),
                        payload: bridge::BridgeRequestsPayload::Watch(None),
                    })
                    .await?;
            } else {
                let keypair = check[0].to_owned();
                bridge_subscribtion
                    .sender
                    .send(bridge::BridgeRequests {
                        network: network.clone(),
                        payload: bridge::BridgeRequestsPayload::Watch(Some((keypair.0, keypair.1))),
                    })
                    .await?;
            }

            let bridge_res = bridge_subscribtion.receiver.recv().await?;

            let error_code = bridge_res.error as u32;

            if error_code != 0 {
                return handle_bridge_error(error_code).map(|_| String::new());
            }

            match bridge_res.payload {
                bridge::BridgeResponsePayload::Watch(token_priv, token_pub) => {
                    // add pairings to db
                    self.cashier_wallet.put_deposit_keys(
                        &drk_pub_key,
                        &token_priv,
                        &serialize(&token_pub),
                        &network,
                        &asset_id,
                    )?;

                    return Ok(token_pub);
                }
                bridge::BridgeResponsePayload::Address(token_pub) => {
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
        let network = network.to_string();
        let token = &args[1].as_str().unwrap();
        let address = &args[2].as_str().unwrap();
        let _amount = &args[3];

        if !self.features.contains_key(&network.clone()) {
            return JsonResult::Err(jsonerr(
                InvalidParams,
                Some(format!("Cashier doesn't support this network: {}", network)),
                id,
            ));
        }

        let result: Result<String> = async {
            Self::check_token_id(&network, token)?;

            let asset_id = generate_id(&token)?;
            let address = serialize(&address.to_string());

            let cashier_public: jubjub::SubgroupPoint;

            if let Some(addr) = self
                .cashier_wallet
                .get_withdraw_keys_by_token_public_key(&address, &network)?
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

    fn check_token_id(network: &str, token_id: &str) -> Result<()> {
        match network {
            #[cfg(feature = "sol")]
            "sol" | "solana" => {
                if token_id != "So11111111111111111111111111111111111111112" {
                    // This is supposed to be a token mint account now
                    use drk::service::sol::account_is_initialized_mint;
                    use drk::service::sol::SolFailed::BadSolAddress;
                    use solana_sdk::pubkey::Pubkey;
                    use std::str::FromStr;

                    let pubkey = match Pubkey::from_str(token_id) {
                        Ok(v) => v,
                        Err(e) => return Err(Error::from(BadSolAddress(e.to_string()))),
                    };

                    // FIXME: Use network name from variable
                    if !account_is_initialized_mint("devnet".to_string(), &pubkey) {
                        return Err(Error::CashierInvalidTokenId(
                            "Given address is not a valid token mint".into(),
                        ));
                    }
                }
            }
            #[cfg(feature = "btc")]
            "btc" | "bitcoin" => {
                // Handle bitcoin address here if needed
            }
            _ => {}
        }
        Ok(())
    }

    async fn start(&self, executor: Arc<Executor<'static>>) -> Result<()> {
        self.cashier_wallet.init_db().await?;

        for (feature_name, chain) in self.features.iter() {
            let bridge2 = self.bridge.clone();

            match feature_name.as_str() {
                #[cfg(feature = "sol")]
                "sol" | "solana" => {
                    debug!(target: "CASHIER DAEMON", "Add sol network");
                    use drk::service::SolClient;
                    use solana_sdk::signer::keypair::Keypair;

                    let main_keypair: Keypair;

                    let native_sol_token_id = "So11111111111111111111111111111111111111112";
                    let native_sol_token_id = generate_id(native_sol_token_id)?;
                    let main_keypairs = self
                        .cashier_wallet
                        .get_main_keys(&"sol".into(), &native_sol_token_id)?;

                    if main_keypairs.is_empty() {
                        main_keypair = Keypair::new();
                    } else {
                        main_keypair = deserialize(&main_keypairs[0].0)?;
                    }

                    let sol_client = SolClient::new(serialize(&main_keypair), &chain).await?;

                    bridge2.add_clients("sol".into(), sol_client).await?;
                }

                #[cfg(feature = "btc")]
                "btc" | "bitcoin" => {
                    debug!(target: "CASHIER DAEMON", "Add btc network");
                    //use drk::service::btc::{BtcClient, BitcoinKeys};

                    //let _btc_client = BtcClient::new("testnet")?;
                    // NOTE bitcoin is not implemented yet
                    //let _main_keypair: BitcoinKeys;

                    let native_btc_token_id = generate_id("btc")?;
                    let _main_keypairs = self
                        .cashier_wallet
                        .get_main_keys(&"btc".into(), &native_btc_token_id)?;
                    // if main_keypairs.is_empty() {
                    //     //main_keypair = BitcoinKeys::new(bitcoin::network::constants::Network::Testnet)?;
                    // } else {
                    //     //main_keypair = deserialize(&main_keypairs[0].0)?;
                    // }
                    //
                    // TODO check if there is main_keypair inside
                    // cashierdb before generating new one
                    //
                    //bridge2.add_clients("btc".into(), btc_client).await?;
                }

                _ => {
                    warn!("No feature enabled for {} network", feature_name);
                }
            }
        }

        let resume_watch_deposit_keys_task = executor.spawn(Self::resume_watch_deposit_keys(
            self.bridge.clone(),
            self.cashier_wallet.clone(),
            self.features.clone(),
        ));

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

        resume_watch_deposit_keys_task.cancel().await;
        listen_for_receiving_coins_task.cancel().await;
        cashier_client_subscriber_task.cancel().await;
        Ok(())
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
