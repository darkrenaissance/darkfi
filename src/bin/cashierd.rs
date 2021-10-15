use async_std::sync::{Arc, Mutex};
use std::collections::HashMap;
use std::path::PathBuf;
use std::str::FromStr;

use async_trait::async_trait;
use clap::clap_app;
use ff::Field;
use log::debug;
use rand::rngs::OsRng;
use serde_json::{json, Value};

use drk::{
    blockchain::{rocks::columns, Rocks, RocksColumn},
    cli::{CashierdConfig, Config},
    client::{Client, State},
    crypto::{
        load_params, merkle::CommitmentTree, save_params, setup_mint_prover, setup_spend_prover,
    },
    rpc::{
        jsonrpc::{error as jsonerr, response as jsonresp},
        jsonrpc::{ErrorCode::*, JsonRequest, JsonResult},
        rpcserver::{listen_and_serve, RequestHandler, RpcServerConfig},
    },
    serial::{deserialize, serialize},
    service::{bridge, bridge::Bridge},
    util::{expand_path, generate_id, join_config_path, parse::truncate, NetworkName},
    wallet::{CashierDb, WalletDb},
    Error, Result,
};

fn handle_bridge_error(error_code: u32) -> Result<()> {
    match error_code {
        1 => Err(Error::BridgeError("Not Supported Client".into())),
        2 => Err(Error::BridgeError(
            "Unable to watch the deposit address".into(),
        )),
        3 => Err(Error::BridgeError("Unable to send the token".into())),
        _ => Err(Error::BridgeError("Unknown error_code".into())),
    }
}

#[derive(Clone, Debug)]
pub struct Network {
    pub name: NetworkName,
    pub blockchain: String,
    pub keypair: String,
}

struct Cashierd {
    config: CashierdConfig,
    bridge: Arc<Bridge>,
    cashier_wallet: Arc<CashierDb>,
    networks: Vec<Network>,
}

#[async_trait]
impl RequestHandler for Cashierd {
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
    async fn new(config_path: PathBuf) -> Result<Self> {
        debug!(target: "CASHIER DAEMON", "Initialize");

        let config: CashierdConfig = Config::<CashierdConfig>::load(config_path)?;

        let cashier_wallet = CashierDb::new(
            expand_path(&config.cashier_wallet_path.clone())?.as_path(),
            config.cashier_wallet_password.clone(),
        )?;

        let mut networks = Vec::new();

        for network in config.clone().networks {
            networks.push(Network {
                name: NetworkName::from_str(&network.name)?,
                blockchain: network.blockchain,
                keypair: network.keypair,
            });
        }

        let bridge = bridge::Bridge::new();

        Ok(Self {
            config: config.clone(),
            bridge,
            cashier_wallet,
            networks,
        })
    }

    async fn resume_watch_deposit_keys(
        bridge: Arc<Bridge>,
        cashier_wallet: Arc<CashierDb>,
        networks: Vec<Network>,
    ) -> Result<()> {
        debug!(target: "CASHIER DAEMON", "Resume watch deposit keys");

        for network in networks.iter() {
            let keypairs_to_watch =
                cashier_wallet.get_deposit_token_keys_by_network(&network.name)?;

            for (drk_pub_key, private_key, public_key, _token_id, mint_address) in keypairs_to_watch
            {
                let bridge = bridge.clone();

                let bridge_subscribtion = bridge.subscribe(drk_pub_key, Some(mint_address)).await;

                bridge_subscribtion
                    .sender
                    .send(bridge::BridgeRequests {
                        network: network.name.clone(),
                        payload: bridge::BridgeRequestsPayload::Watch(Some((
                            private_key,
                            public_key,
                        ))),
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
        // received drk coin
        let (drk_pub_key, amount) = recv_coin.recv().await?;

        debug!(target: "CASHIER DAEMON", "Receive coin with amount: {}", amount);

        // get public key, and token_id of the token
        let token = cashier_wallet.get_withdraw_token_public_key_by_dkey_public(&drk_pub_key)?;

        // send a request to bridge to send equivalent amount of
        // received drk coin to token publickey
        if let Some((addr, network, _token_id, mint_address)) = token {
            let bridge_subscribtion = bridge.subscribe(drk_pub_key, Some(mint_address)).await;

            // send a request to the bridge to send amount of token
            // equivalent to the received drk
            bridge_subscribtion
                .sender
                .send(bridge::BridgeRequests {
                    network: network.clone(),
                    payload: bridge::BridgeRequestsPayload::Send(addr.clone(), amount),
                })
                .await?;

            // receive a response
            let res = bridge_subscribtion.receiver.recv().await?;

            // check the response's error
            let error_code = res.error as u32;

            if error_code != 0 {
                return handle_bridge_error(error_code);
            }

            match res.payload {
                bridge::BridgeResponsePayload::Send => {
                    cashier_wallet.confirm_withdraw_key_record(&addr, &network)?;
                }
                _ => {
                    return Err(Error::BridgeError(
                        "Receive unknown value from Subscription".into(),
                    ));
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

        let network = &args[0];
        let mint_address = &args[1];
        let drk_pub_key = &args[2];

        if network.as_str().is_none() {
            return JsonResult::Err(jsonerr(InvalidNetworkParam, None, id));
        }

        let network = network.as_str().unwrap();

        let network = match NetworkName::from_str(network) {
            Ok(d) => d,
            Err(e) => {
                return JsonResult::Err(jsonerr(InvalidNetworkParam, Some(e.to_string()), id));
            }
        };

        if mint_address.as_str().is_none() {
            return JsonResult::Err(jsonerr(InvalidTokenIdParam, None, id));
        }

        let mut mint_address = mint_address.as_str().unwrap().to_string();

        if drk_pub_key.as_str().is_none() {
            return JsonResult::Err(jsonerr(InvalidAddressParam, None, id));
        }

        let drk_pub_key = drk_pub_key.as_str().unwrap();

        // Check if the features list contains this network
        if self
            .networks
            .iter()
            .find(|net| net.name == network)
            .is_none()
        {
            return JsonResult::Err(jsonerr(
                InvalidParams,
                Some(format!("Cashier doesn't support this network: {}", network)),
                id,
            ));
        }

        let result: Result<String> = async {
            let token_id = generate_id(&mint_address, &network)?;

            let mint_address_opt = Self::check_token_id(&network, &mint_address)?;

            if mint_address_opt.is_none() {
                mint_address = String::new();
            }
            let drk_pub_key = bs58::decode(&drk_pub_key).into_vec()?;
            let drk_pub_key: jubjub::SubgroupPoint = deserialize(&drk_pub_key)?;

            // check if the drk public key already exist
            let check = self
                .cashier_wallet
                .get_deposit_token_keys_by_dkey_public(&drk_pub_key, &network)?;

            // start new subscription from the bridge and then cashierd will
            // send a request to the bridge to generate keypair for the desired token
            // and start watch this token's keypair
            // once a bridge receive an update for this token's address
            // cashierd will get notification from bridge.listen() function
            //
            // The "if statement" check from the cashierdb if the node's drk_pub_key already exist
            // in this case it will not generate new keypair but it will
            // retrieve the old generated keypair
            //
            // Once receive a response from the bridge, the cashierd then save a deposit
            // record in cashierdb with the network name and token id

            let bridge = self.bridge.clone();
            let bridge_subscribtion = bridge.subscribe(drk_pub_key, mint_address_opt).await;

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
                        &token_id,
                        mint_address,
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
        debug!(target: "CASHIER DAEMON", "RECEIVED WITHDRAW REQUEST");

        let args: &Vec<serde_json::Value> = params.as_array().unwrap();

        if args.len() != 4 {
            return JsonResult::Err(jsonerr(InvalidParams, None, id));
        }

        let network = &args[0];
        let mint_address = &args[1];
        let address = &args[2];
        let amount = &args[3];

        if network.as_str().is_none() {
            return JsonResult::Err(jsonerr(InvalidNetworkParam, None, id));
        }

        let network = NetworkName::from_str(args[0].as_str().unwrap()).unwrap();

        if mint_address.as_str().is_none() {
            return JsonResult::Err(jsonerr(InvalidTokenIdParam, None, id));
        }

        let mut mint_address = mint_address.as_str().unwrap().to_string();

        if address.as_str().is_none() {
            return JsonResult::Err(jsonerr(InvalidAddressParam, None, id));
        }

        let address = address.as_str().unwrap();

        if amount.as_u64().is_none() {
            return JsonResult::Err(jsonerr(InvalidAmountParam, None, id));
        }

        let _amount = amount.as_u64().unwrap();

        // Check if the features list contains this network
        if self
            .networks
            .iter()
            .find(|net| net.name == network)
            .is_none()
        {
            return JsonResult::Err(jsonerr(
                InvalidParams,
                Some(format!("Cashier doesn't support this network: {}", network)),
                id,
            ));
        }

        let result: Result<String> = async {
            let token_id = generate_id(&mint_address, &network)?;

            let mint_address_opt = Self::check_token_id(&network, &mint_address)?;

            if mint_address_opt.is_none() {
                // empty string
                mint_address = String::new();
            }

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
                    &network,
                    &token_id,
                    mint_address,
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
        JsonResult::Resp(jsonresp(
            json!(self
                .networks
                .iter()
                .map(|net| (net.name.to_string(), net.blockchain.to_owned()))
                .collect::<HashMap<String, String>>()),
            id,
        ))
    }

    fn check_token_id(network: &NetworkName, _token_id: &str) -> Result<Option<String>> {
        match network {
            #[cfg(feature = "sol")]
            NetworkName::Solana => {
                use drk::service::sol::SOL_NATIVE_TOKEN_ID;
                if _token_id != SOL_NATIVE_TOKEN_ID {
                    return Ok(Some(_token_id.to_string()));
                }
                return Ok(None);
            }
            #[cfg(feature = "btc")]
            NetworkName::Bitcoin => Ok(None),
            _ => Err(Error::NotSupportedNetwork),
        }
    }

    async fn start(
        &mut self,
        mut client: Client,
        state: Arc<Mutex<State>>,
    ) -> Result<(
        smol::Task<Result<()>>,
        smol::Task<Result<()>>,
        smol::Task<Result<()>>,
    )> {
        self.cashier_wallet.init_db().await?;

        for network in self.networks.iter() {
            match network.name {
                #[cfg(feature = "sol")]
                NetworkName::Solana => {
                    debug!(target: "CASHIER DAEMON", "Add sol network");
                    use drk::service::{sol::SolFailed, SolClient};
                    use solana_sdk::{signature::Signer, signer::keypair::Keypair};

                    let bridge2 = self.bridge.clone();

                    let main_keypair: Keypair;

                    let main_keypairs = self.cashier_wallet.get_main_keys(&NetworkName::Solana)?;

                    if network.keypair.is_empty() {
                        if main_keypairs.is_empty() {
                            main_keypair = Keypair::new();
                            self.cashier_wallet.put_main_keys(
                                &serialize(&main_keypair),
                                &serialize(&main_keypair.pubkey()),
                                &NetworkName::Solana,
                            )?;
                        } else {
                            main_keypair = deserialize(&main_keypairs[main_keypairs.len() - 1].0)?;
                        }
                    } else {
                        let keypair_str = drk::cli::cli_config::load_keypair_to_str(
                            PathBuf::from(expand_path(&network.keypair.clone())?),
                        )?;
                        let keypair_bytes: Vec<u8> = serde_json::from_str(&keypair_str)?;
                        main_keypair = Keypair::from_bytes(&keypair_bytes)
                            .map_err(|e| SolFailed::ParseError(e.to_string()))?;
                    }

                    let sol_client = SolClient::new(main_keypair, &network.blockchain).await?;

                    bridge2.add_clients(NetworkName::Solana, sol_client).await?;
                }

                #[cfg(feature = "btc")]
                NetworkName::Bitcoin => {
                    debug!(target: "CASHIER DAEMON", "Add btc network");
                    use drk::service::btc::{BtcClient, BtcFailed, Keypair};

                    let bridge2 = self.bridge.clone();

                    let main_keypair: Keypair;

                    let main_keypairs = self.cashier_wallet.get_main_keys(&NetworkName::Bitcoin)?;

                    if network.keypair.is_empty() {
                        if main_keypairs.is_empty() {
                            main_keypair = Keypair::new();
                            self.cashier_wallet.put_main_keys(
                                &serialize(&main_keypair),
                                &serialize(&main_keypair.pubkey()),
                                &NetworkName::Bitcoin,
                            )?;
                        } else {
                            main_keypair = deserialize(&main_keypairs[main_keypairs.len() - 1].0)?;
                        }
                    } else {
                        let keypair_str = drk::cli::cli_config::load_keypair_to_str(
                            PathBuf::from(expand_path(&network.keypair.clone())?),
                        )?;
                        let keypair_bytes: Vec<u8> = serde_json::from_str(&keypair_str)?;
                        main_keypair = Keypair::from_bytes(&keypair_bytes)
                            .map_err(|e| BtcFailed::DecodeAndEncodeError(e.to_string()))?;
                    }

                    let btc_client = BtcClient::new(main_keypair, &network.blockchain).await?;

                    bridge2
                        .add_clients(NetworkName::Bitcoin, btc_client)
                        .await?;
                }
                _ => {}
            }
        }

        let resume_watch_deposit_keys_task = smol::spawn(Self::resume_watch_deposit_keys(
            self.bridge.clone(),
            self.cashier_wallet.clone(),
            self.networks.clone(),
        ));

        client.start().await?;

        let (notify, recv_coin) = async_channel::unbounded::<(jubjub::SubgroupPoint, u64)>();

        client
            .connect_to_subscriber_from_cashier(state, self.cashier_wallet.clone(), notify.clone())
            .await?;

        let cashier_wallet = self.cashier_wallet.clone();
        let bridge = self.bridge.clone();
        let listen_for_receiving_coins_task: smol::Task<Result<()>> = smol::spawn(async move {
            loop {
                Self::listen_for_receiving_coins(
                    bridge.clone(),
                    cashier_wallet.clone(),
                    recv_coin.clone(),
                )
                .await?;
            }
        });

        let bridge2 = self.bridge.clone();
        let listen_for_notification_from_bridge_task: smol::Task<Result<()>> =
            smol::spawn(async move {
                while let Some(token_notification) = bridge2.clone().listen().await {
                    debug!(target: "CASHIER DAEMON", "Notification from birdge");

                    let token_notification = token_notification?;

                    let received_balance = truncate(
                        token_notification.received_balance,
                        8,
                        token_notification.decimals,
                    )?;

                    client
                        .send(
                            token_notification.drk_pub_key,
                            received_balance,
                            token_notification.token_id,
                            true,
                        )
                        .await?;
                }
                Ok(())
            });

        Ok((
            resume_watch_deposit_keys_task,
            listen_for_receiving_coins_task,
            listen_for_notification_from_bridge_task,
        ))
    }
}

#[async_std::main]
async fn main() -> Result<()> {
    let args = clap_app!(cashierd =>
        (@arg CONFIG: -c --config +takes_value "Sets a custom config file")
        (@arg ADDRESS: -a --address "Get Cashier Public key")
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
    let mut cashierd = Cashierd::new(config_path).await?;

    let client_wallet = WalletDb::new(
        expand_path(&cashierd.config.client_wallet_path.clone())?.as_path(),
        cashierd.config.client_wallet_password.clone(),
    )?;

    let rocks = Rocks::new(expand_path(&cashierd.config.database_path.clone())?.as_path())?;

    let params_paths = (
        expand_path(&cashierd.config.mint_params_path.clone())?,
        expand_path(&cashierd.config.spend_params_path.clone())?,
    );

    let mint_params_path = params_paths.0.to_str().unwrap_or("mint.params");
    let spend_params_path = params_paths.1.to_str().unwrap_or("spend.params");
    // Auto create trusted ceremony parameters if they don't exist
    if !params_paths.0.exists() {
        let params = setup_mint_prover();
        save_params(mint_params_path, &params)?;
    }
    if !params_paths.1.exists() {
        let params = setup_spend_prover();
        save_params(spend_params_path, &params)?;
    }

    // Load trusted setup parameters
    let (mint_params, mint_pvk) = load_params(mint_params_path)?;
    let (spend_params, spend_pvk) = load_params(spend_params_path)?;

    let client = Client::new(
        rocks.clone(),
        (
            cashierd.config.gateway_protocol_url.parse()?,
            cashierd.config.gateway_publisher_url.parse()?,
        ),
        client_wallet.clone(),
        mint_params,
        spend_params,
    )
    .await?;

    let merkle_roots = RocksColumn::<columns::MerkleRoots>::new(rocks.clone());
    let nullifiers = RocksColumn::<columns::Nullifiers>::new(rocks);

    let cashier_public_keys = vec![client.main_keypair.public];

    let state = Arc::new(Mutex::new(State {
        tree: CommitmentTree::empty(),
        merkle_roots,
        nullifiers,
        mint_pvk,
        spend_pvk,
        public_keys: cashier_public_keys,
    }));

    if args.is_present("ADDRESS") {
        let cashier_public = client.main_keypair.public;
        let cashier_public = bs58::encode(&serialize(&cashier_public)).into_string();
        println!("Public Key: {}", cashier_public);
        return Ok(());
    };

    let cfg = RpcServerConfig {
        socket_addr: cashierd.config.rpc_listen_address.clone(),
        use_tls: cashierd.config.serve_tls,
        identity_path: expand_path(&cashierd.config.clone().tls_identity_path)?,
        identity_pass: cashierd.config.tls_identity_password.clone(),
    };

    let (t1, t2, t3) = cashierd.start(client, state).await?;
    listen_and_serve(cfg, Arc::new(cashierd)).await?;

    t1.cancel().await;
    t2.cancel().await;
    t3.cancel().await;

    Ok(())
}
