use std::{path::PathBuf, str::FromStr};

use async_executor::Executor;
use async_std::sync::{Arc, Mutex};
use async_trait::async_trait;
use clap::clap_app;
use easy_parallel::Parallel;
use log::debug;
use pasta_curves::pallas;
use serde_json::{json, Value};

use drk::{
    blockchain::{rocks::columns, Rocks, RocksColumn},
    cli::{CashierdConfig, Config},
    client::{Client, State},
    crypto::{mint_proof::MintProofKeys, schnorr, spend_proof::SpendProofKeys},
    rpc::{
        jsonrpc::{error as jsonerr, response as jsonresp, ErrorCode::*, JsonRequest, JsonResult},
        rpcserver::{listen_and_serve, RequestHandler, RpcServerConfig},
    },
    serial::{deserialize, serialize},
    service::{bridge, bridge::Bridge},
    types::DrkTokenId,
    util::{expand_path, generate_id, join_config_path, parse::truncate, NetworkName},
    wallet::{cashierdb::TokenKey, CashierDb, WalletDb},
    Error, Result,
};

fn handle_bridge_error(error_code: u32) -> Result<()> {
    match error_code {
        1 => Err(Error::BridgeError("Not Supported Client".into())),
        2 => Err(Error::BridgeError("Unable to watch the deposit address".into())),
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
    bridge: Arc<Bridge>,
    cashier_wallet: Arc<CashierDb>,
    networks: Vec<Network>,
    public_key: String,
    config: CashierdConfig,
}

#[async_trait]
impl RequestHandler for Cashierd {
    async fn handle_request(&self, req: JsonRequest, executor: Arc<Executor<'_>>) -> JsonResult {
        if req.params.as_array().is_none() {
            return JsonResult::Err(jsonerr(InvalidParams, None, req.id))
        }

        debug!(target: "RPC", "--> {}", serde_json::to_string(&req).unwrap());

        match req.method.as_str() {
            Some("deposit") => return self.deposit(req.id, req.params, executor).await,
            Some("withdraw") => return self.withdraw(req.id, req.params).await,
            Some("features") => return self.features(req.id, req.params).await,
            Some(_) => {}
            None => {}
        };

        return JsonResult::Err(jsonerr(MethodNotFound, None, req.id))
    }
}

impl Cashierd {
    async fn new(config: CashierdConfig) -> Result<Self> {
        debug!(target: "CASHIER DAEMON", "Initialize");

        let cashier_wallet = CashierDb::new(
            expand_path(&config.cashier_wallet_path)?.as_path(),
            config.cashier_wallet_password.clone(),
        )?;

        let mut networks = Vec::new();

        let cfg = config.clone();
        for network in cfg.networks {
            networks.push(Network {
                name: NetworkName::from_str(&network.name)?,
                blockchain: network.blockchain,
                keypair: network.keypair,
            });
        }

        let bridge = bridge::Bridge::new();

        Ok(Self { bridge, cashier_wallet, networks, public_key: String::from(""), config })
    }

    async fn listen_for_receiving_coins(
        bridge: Arc<Bridge>,
        cashier_wallet: Arc<CashierDb>,
        recv_coin: async_channel::Receiver<(pallas::Point, u64)>,
        executor: Arc<Executor<'_>>,
    ) -> Result<()> {
        // received drk coin
        let (drk_pub_key, amount) = recv_coin.recv().await?;

        debug!(target: "CASHIER DAEMON", "Receive coin with amount: {}", amount);

        // get public key, and token_id of the token
        let token =
            cashier_wallet.get_withdraw_token_public_key_by_dkey_public(&drk_pub_key).await?;

        // send a request to bridge to send equivalent amount of
        // received drk coin to token publickey
        if let Some(withdraw_token) = token {
            let bridge_subscribtion = bridge
                .subscribe(drk_pub_key, Some(withdraw_token.mint_address), executor.clone())
                .await;

            // send a request to the bridge to send amount of token
            // equivalent to the received drk
            bridge_subscribtion
                .sender
                .send(bridge::BridgeRequests {
                    network: withdraw_token.network.clone(),
                    payload: bridge::BridgeRequestsPayload::Send(
                        withdraw_token.token_public_key.clone(),
                        amount,
                    ),
                })
                .await?;

            // receive a response
            let res = bridge_subscribtion.receiver.recv().await?;

            // check the response's error
            let error_code = res.error as u32;

            if error_code != 0 {
                return handle_bridge_error(error_code)
            }

            match res.payload {
                bridge::BridgeResponsePayload::Send => {
                    cashier_wallet
                        .confirm_withdraw_key_record(
                            &withdraw_token.token_public_key,
                            &withdraw_token.network,
                        )
                        .await?;
                }
                _ => {
                    return Err(Error::BridgeError("Receive unknown value from Subscription".into()))
                }
            }
        }

        Ok(())
    }

    async fn deposit(&self, id: Value, params: Value, executor: Arc<Executor<'_>>) -> JsonResult {
        debug!(target: "CASHIER DAEMON", "RECEIVED DEPOSIT REQUEST");

        let args: &Vec<serde_json::Value> = params.as_array().unwrap();

        if args.len() != 3 {
            return JsonResult::Err(jsonerr(InvalidParams, None, id))
        }

        let network: NetworkName;
        let mut mint_address: &str;
        let drk_pub_key: &str;

        match (args[0].as_str(), args[1].as_str(), args[2].as_str()) {
            (Some(n), Some(m), Some(d)) => {
                if NetworkName::from_str(n).is_err() {
                    return JsonResult::Err(jsonerr(InvalidNetworkParam, None, id))
                }
                network = NetworkName::from_str(n).unwrap();
                mint_address = m;
                drk_pub_key = d;
            }
            (None, _, _) => return JsonResult::Err(jsonerr(InvalidNetworkParam, None, id)),
            (_, None, _) => return JsonResult::Err(jsonerr(InvalidTokenIdParam, None, id)),
            (_, _, None) => return JsonResult::Err(jsonerr(InvalidAddressParam, None, id)),
        }

        // Check if the features list contains this network
        if !self.networks.iter().any(|net| net.name == network) {
            return JsonResult::Err(jsonerr(
                InvalidParams,
                Some(format!("Cashier doesn't support this network: {}", network)),
                id,
            ))
        }

        let result: Result<String> = async {
            let token_id = generate_id(mint_address, &network)?;

            let mint_address_opt = Self::check_token_id(&network, mint_address)?;

            if mint_address_opt.is_none() {
                mint_address = "";
            }
            let drk_pub_key = bs58::decode(&drk_pub_key).into_vec()?;
            let drk_pub_key: pallas::Point = deserialize(&drk_pub_key)?;

            // check if the drk public key already exist
            let check = self
                .cashier_wallet
                .get_deposit_token_keys_by_dkey_public(&drk_pub_key, &network)
                .await?;

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
            let bridge_subscribtion =
                bridge.subscribe(drk_pub_key, mint_address_opt, executor).await;

            if check.is_empty() {
                bridge_subscribtion
                    .sender
                    .send(bridge::BridgeRequests {
                        network: network.clone(),
                        payload: bridge::BridgeRequestsPayload::Watch(None),
                    })
                    .await?;
            } else {
                let keypair = check[0].clone();
                bridge_subscribtion
                    .sender
                    .send(bridge::BridgeRequests {
                        network: network.clone(),
                        payload: bridge::BridgeRequestsPayload::Watch(Some(keypair)),
                    })
                    .await?;
            }

            let bridge_res = bridge_subscribtion.receiver.recv().await?;

            let error_code = bridge_res.error as u32;

            if error_code != 0 {
                return handle_bridge_error(error_code).map(|_| String::new())
            }

            match bridge_res.payload {
                bridge::BridgeResponsePayload::Watch(token_key) => {
                    // add pairings to db
                    self.cashier_wallet
                        .put_deposit_keys(
                            &drk_pub_key,
                            &token_key.private_key,
                            &serialize(&token_key.public_key),
                            &network,
                            &token_id,
                            mint_address.into(),
                        )
                        .await?;

                    Ok(token_key.public_key)
                }
                bridge::BridgeResponsePayload::Address(token_pub) => Ok(token_pub),
                _ => Err(Error::BridgeError("Receive unknown value from Subscription".into())),
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
            return JsonResult::Err(jsonerr(InvalidParams, None, id))
        }

        let network: NetworkName;
        let mut mint_address: &str;
        let address: &str;

        match (args[0].as_str(), args[1].as_str(), args[2].as_str()) {
            (Some(n), Some(m), Some(a)) => {
                if NetworkName::from_str(n).is_err() {
                    return JsonResult::Err(jsonerr(InvalidNetworkParam, None, id))
                }
                network = NetworkName::from_str(n).unwrap();
                mint_address = m;
                address = a;
            }
            (None, _, _) => return JsonResult::Err(jsonerr(InvalidNetworkParam, None, id)),
            (_, None, _) => return JsonResult::Err(jsonerr(InvalidTokenIdParam, None, id)),
            (_, _, None) => return JsonResult::Err(jsonerr(InvalidAddressParam, None, id)),
        }

        // Check if the features list contains this network
        if !self.networks.iter().any(|net| net.name == network) {
            return JsonResult::Err(jsonerr(
                InvalidParams,
                Some(format!("Cashier doesn't support this network: {}", network)),
                id,
            ))
        }

        let result: Result<String> = async {
            let token_id: DrkTokenId = generate_id(mint_address, &network)?;

            let mint_address_opt = Self::check_token_id(&network, mint_address)?;

            if mint_address_opt.is_none() {
                // empty string
                mint_address = "";
            }

            let address = serialize(&address.to_string());

            let cashier_public: pallas::Point;

            if let Some(addr) = self
                .cashier_wallet
                .get_withdraw_keys_by_token_public_key(&address, &network)
                .await?
            {
                cashier_public = addr.public;
            } else {
                let cashier_secret = schnorr::SecretKey::random();
                cashier_public = cashier_secret.public_key();

                self.cashier_wallet
                    .put_withdraw_keys(
                        &address,
                        &cashier_public,
                        &cashier_secret,
                        &network,
                        &token_id,
                        mint_address.into(),
                    )
                    .await?;
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
        let tcp_port: Option<u16>;
        let tls_port: Option<u16>;
        let onionaddr: Option<String>;
        let dnsaddr: Option<String>;

        if self.config.serve_tls {
            tls_port = Some(self.config.rpc_listen_address.port());
            tcp_port = None;
        } else {
            tcp_port = Some(self.config.rpc_listen_address.port());
            tls_port = None;
        }

        if self.config.dns_addr.ends_with(".onion") {
            onionaddr = Some(self.config.dns_addr.clone());
            dnsaddr = None;
        } else {
            dnsaddr = Some(self.config.dns_addr.clone());
            onionaddr = None;
        }

        let mut resp: serde_json::Value = json!(
        {
            "server_version": env!("CARGO_PKG_VERSION"),
            "protocol_version": "1.0",
            "public_key": self.public_key,
            "networks": [],
            "hosts": {
                "tcp_port": tcp_port,
                "tls_port": tls_port,
                "onion_addr": onionaddr,
                "dns_addr": dnsaddr,
            }
        }
        );

        for network in self.networks.iter() {
            resp.as_object_mut().unwrap()["networks"].as_array_mut().unwrap().push(json!(
                    {
                        network.name.to_string().to_lowercase():
                        {"chain": network.blockchain.to_lowercase()}
                    }
            ));
        }

        JsonResult::Resp(jsonresp(resp, id))
    }

    fn check_token_id(network: &NetworkName, _token_id: &str) -> Result<Option<String>> {
        match network {
            #[cfg(feature = "sol")]
            NetworkName::Solana => {
                use drk::service::sol::SOL_NATIVE_TOKEN_ID;
                if _token_id != SOL_NATIVE_TOKEN_ID {
                    return Ok(Some(_token_id.to_string()))
                }
                Ok(None)
            }
            #[cfg(feature = "eth")]
            NetworkName::Ethereum => {
                use drk::service::eth::ETH_NATIVE_TOKEN_ID;
                if _token_id != ETH_NATIVE_TOKEN_ID {
                    return Ok(Some(_token_id.to_string()))
                }
                Ok(None)
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
        executor: Arc<Executor<'_>>,
    ) -> Result<(smol::Task<Result<()>>, smol::Task<Result<()>>)> {
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

                    let main_keypairs =
                        self.cashier_wallet.get_main_keys(&NetworkName::Solana).await?;

                    if network.keypair.is_empty() {
                        if main_keypairs.is_empty() {
                            main_keypair = Keypair::new();
                            self.cashier_wallet
                                .put_main_keys(
                                    &TokenKey {
                                        private_key: serialize(&main_keypair),
                                        public_key: serialize(&main_keypair.pubkey()),
                                    },
                                    &NetworkName::Solana,
                                )
                                .await?;
                        } else {
                            main_keypair =
                                deserialize(&main_keypairs[main_keypairs.len() - 1].private_key)?;
                        }
                    } else {
                        let keypair_str = drk::cli::cli_config::load_keypair_to_str(expand_path(
                            &network.keypair.clone(),
                        )?)?;
                        let keypair_bytes: Vec<u8> = serde_json::from_str(&keypair_str)?;
                        main_keypair = Keypair::from_bytes(&keypair_bytes)
                            .map_err(|e| SolFailed::Signature(e.to_string()))?;
                    }

                    let sol_client = SolClient::new(main_keypair, &network.blockchain).await?;

                    bridge2.add_clients(NetworkName::Solana, sol_client).await?;
                }

                #[cfg(feature = "eth")]
                NetworkName::Ethereum => {
                    debug!(target: "CASHIER DAEMON", "Add eth network");
                    use drk::service::{
                        eth::{generate_privkey, Keypair},
                        EthClient,
                    };

                    let bridge2 = self.bridge.clone();

                    let main_keypair: Keypair;

                    let main_keypairs =
                        self.cashier_wallet.get_main_keys(&NetworkName::Ethereum).await?;

                    let passphrase = self.config.geth_passphrase.clone();

                    let mut eth_client = EthClient::new(
                        expand_path(&self.config.geth_socket)?.to_str().unwrap().into(),
                        passphrase.clone(),
                    );

                    if main_keypairs.is_empty() {
                        let main_private_key = generate_privkey();
                        let main_public_key = eth_client
                            .import_privkey(&main_private_key, &passphrase)
                            .await?
                            .as_str()
                            .unwrap()
                            .to_string();

                        self.cashier_wallet
                            .put_main_keys(
                                &TokenKey {
                                    private_key: serialize(&main_private_key),
                                    public_key: serialize(&main_public_key),
                                },
                                &NetworkName::Ethereum,
                            )
                            .await?;

                        main_keypair =
                            Keypair { private_key: main_private_key, public_key: main_public_key };
                    } else {
                        let last_keypair = &main_keypairs[main_keypairs.len() - 1];

                        main_keypair = Keypair {
                            private_key: deserialize(&last_keypair.private_key)?,
                            public_key: deserialize(&last_keypair.public_key)?,
                        }
                    }

                    eth_client.set_main_keypair(&main_keypair);

                    bridge2.add_clients(NetworkName::Ethereum, Arc::new(eth_client)).await?;
                }

                #[cfg(feature = "btc")]
                NetworkName::Bitcoin => {
                    debug!(target: "CASHIER DAEMON", "Add btc network");
                    use drk::service::btc::{BtcClient, BtcFailed, Keypair};

                    let bridge2 = self.bridge.clone();

                    let main_keypair: Keypair;

                    let main_keypairs =
                        self.cashier_wallet.get_main_keys(&NetworkName::Bitcoin).await?;

                    if network.keypair.is_empty() {
                        if main_keypairs.is_empty() {
                            main_keypair = Keypair::new();
                            self.cashier_wallet
                                .put_main_keys(
                                    &TokenKey {
                                        private_key: serialize(&main_keypair),
                                        public_key: serialize(&main_keypair.pubkey()),
                                    },
                                    &NetworkName::Bitcoin,
                                )
                                .await?;
                        } else {
                            main_keypair =
                                deserialize(&main_keypairs[main_keypairs.len() - 1].private_key)?;
                        }
                    } else {
                        let keypair_str = drk::cli::cli_config::load_keypair_to_str(expand_path(
                            &network.keypair.clone(),
                        )?)?;
                        let keypair_bytes: Vec<u8> = serde_json::from_str(&keypair_str)?;
                        main_keypair = Keypair::from_bytes(&keypair_bytes)
                            .map_err(|e| BtcFailed::DecodeAndEncodeError(e.to_string()))?;
                    }

                    let btc_client = BtcClient::new(main_keypair, &network.blockchain).await?;

                    bridge2.add_clients(NetworkName::Bitcoin, btc_client).await?;
                }
                _ => {}
            }
        }

        client.start().await?;

        let (notify, recv_coin) = async_channel::unbounded::<(pallas::Point, u64)>();

        client
            .connect_to_subscriber_from_cashier(
                state.clone(),
                self.cashier_wallet.clone(),
                notify.clone(),
                executor.clone(),
            )
            .await?;

        let cashier_wallet = self.cashier_wallet.clone();
        let bridge = self.bridge.clone();
        let ex = executor.clone();
        let listen_for_receiving_coins_task: smol::Task<Result<()>> = executor.spawn(async move {
            let ex2 = ex.clone();
            loop {
                Self::listen_for_receiving_coins(
                    bridge.clone(),
                    cashier_wallet.clone(),
                    recv_coin.clone(),
                    ex2.clone(),
                )
                .await?;
            }
        });

        let bridge2 = self.bridge.clone();
        let listen_for_notification_from_bridge_task: smol::Task<Result<()>> =
            executor.spawn(async move {
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
                            state.clone(),
                        )
                        .await?;
                }
                Ok(())
            });

        Ok((listen_for_receiving_coins_task, listen_for_notification_from_bridge_task))
    }
}

async fn start(
    executor: Arc<Executor<'_>>,
    config: &CashierdConfig,
    get_address_flag: bool,
) -> Result<()> {
    let mut cashierd = Cashierd::new(config.clone()).await?;

    let client_wallet = WalletDb::new(
        expand_path(&config.client_wallet_path.clone())?.as_path(),
        config.client_wallet_password.clone(),
    )?;

    let rocks = Rocks::new(expand_path(&config.database_path.clone())?.as_path())?;

    let mint_proof_keys = MintProofKeys::initialize();
    let spend_proof_keys = SpendProofKeys::initialize();

    let client = Client::new(
        rocks.clone(),
        (config.gateway_protocol_url.parse()?, config.gateway_publisher_url.parse()?),
        client_wallet.clone(),
        mint_proof_keys,
        spend_proof_keys,
    )
    .await?;

    let merkle_roots = RocksColumn::<columns::MerkleRoots>::new(rocks.clone());
    let nullifiers = RocksColumn::<columns::Nullifiers>::new(rocks);

    let cashier_public = client.main_keypair.public;
    let cashier_public_str = bs58::encode(&serialize(&cashier_public)).into_string();
    cashierd.public_key = cashier_public_str.clone();

    let cashier_public_keys = vec![cashier_public];

    let state = Arc::new(Mutex::new(State {
        tree: BridgeTree::<MerkleNode, 32>::new(100),
        merkle_roots,
        nullifiers,
        mint_vk: mint_proof_keys.vk,
        spend_vk: spend_proof_keys.vk,
        public_keys: cashier_public_keys,
    }));

    if get_address_flag {
        println!("Public Key: {}", cashier_public_str);
        return Ok(())
    };

    let cfg = RpcServerConfig {
        socket_addr: config.rpc_listen_address,
        use_tls: config.serve_tls,
        identity_path: expand_path(&config.clone().tls_identity_path)?,
        identity_pass: config.tls_identity_password.clone(),
    };

    let (t1, t2) = cashierd.start(client, state, executor.clone()).await?;
    listen_and_serve(cfg, Arc::new(cashierd), executor).await?;

    t1.cancel().await;
    t2.cancel().await;

    Ok(())
}

#[async_std::main]
async fn main() -> Result<()> {
    let args = clap_app!(cashierd =>
        (@arg CONFIG: -c --config +takes_value "Sets a custom config file")
        (@arg ADDRESS: -a --address "Get Cashier Public key")
        (@arg verbose: -v --verbose "Increase verbosity")
        (@arg refresh: -r --refresh "Refresh the wallet and slabstore")
    )
    .get_matches();

    let config_path = if args.is_present("CONFIG") {
        expand_path(args.value_of("CONFIG").unwrap())?
    } else {
        join_config_path(&PathBuf::from("cashierd.toml"))?
    };

    let loglevel = if args.is_present("verbose") { log::Level::Debug } else { log::Level::Info };

    simple_logger::init_with_level(loglevel)?;

    let config: CashierdConfig = Config::<CashierdConfig>::load(config_path)?;

    if args.is_present("refresh") {
        debug!(target: "CASHIER DAEMON", "Refresh the wallet and the database");

        let client_wallet = WalletDb::new(
            expand_path(&config.client_wallet_path)?.as_path(),
            config.client_wallet_password.clone(),
        )?;

        client_wallet.remove_own_coins()?;

        let wallet = CashierDb::new(
            expand_path(&config.cashier_wallet_path)?.as_path(),
            config.cashier_wallet_password.clone(),
        )?;

        wallet.remove_withdraw_and_deposit_keys().await?;

        if let Some(path) = expand_path(&config.database_path)?.to_str() {
            debug!(target: "CASHIER DAEMON", "Remove database: {}", path);
            std::fs::remove_dir_all(path)?;
        }

        println!("Wallet got updated successfully.");

        return Ok(())
    }

    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = async_channel::unbounded::<()>();

    let ex2 = ex.clone();

    let get_address_flag = args.is_present("ADDRESS");

    let nthreads = num_cpus::get();
    debug!(target: "CASHIER DAEMON", "Run {} executor threads", nthreads);

    let (_, result) = Parallel::new()
        .each(0..nthreads, |_| smol::future::block_on(ex.run(shutdown.recv())))
        // Run the main future on the current thread.
        .finish(|| {
            smol::future::block_on(async move {
                start(ex2, &config, get_address_flag).await?;
                drop(signal);
                Ok::<(), drk::Error>(())
            })
        });

    result
}

// async fn resume_watch_deposit_keys(
//     bridge: Arc<Bridge>,
//     cashier_wallet: Arc<CashierDb>,
//     networks: Vec<Network>,
//     executor: Arc<Executor<'_>>,
// ) -> Result<()> {
//     debug!(target: "CASHIER DAEMON", "Resume watch deposit keys");

//     for network in networks.iter() {
//         let keypairs_to_watch =
//             cashier_wallet.get_deposit_token_keys_by_network(&network.name)?;

//         for deposit_token in keypairs_to_watch {
//             let bridge = bridge.clone();

//             let bridge_subscribtion = bridge
//                 .subscribe(
//                     deposit_token.drk_public_key,
//                     Some(deposit_token.mint_address),
//                     executor.clone(),
//                 )
//                 .await;

//             bridge_subscribtion
//                 .sender
//                 .send(bridge::BridgeRequests {
//                     network: network.name.clone(),
//                     payload: bridge::BridgeRequestsPayload::Watch(Some(
//                         deposit_token.token_key,
//                     )),
//                 })
//                 .await?;
//         }
//     }

//     Ok(())
// }
