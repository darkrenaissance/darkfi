/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use std::{net::SocketAddr, path::PathBuf, str::FromStr};

use async_executor::Executor;
use async_std::sync::{Arc, Mutex};
use async_trait::async_trait;
use clap::{IntoApp, Parser};
use easy_parallel::Parallel;
use log::{debug, info};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use simplelog::{ColorChoice, TermLogger, TerminalMode};

use darkfi::{
    blockchain::{rocks::columns, Rocks, RocksColumn},
    crypto::{
        address::Address,
        keypair::{PublicKey, SecretKey},
        proof::VerifyingKey,
        token_id::generate_id2,
        types::DrkTokenId,
    },
    node::{client::Client, state::State},
    rpc::{
        jsonrpc::{error as jsonerr, response as jsonresp, ErrorCode::*, JsonRequest, JsonResult},
        rpcserver::{listen_and_serve, RequestHandler, RpcServerConfig},
    },
    util::{
        cli::{log_config, spawn_config, Config},
        expand_path, join_config_path,
        parse::truncate,
        serial::serialize,
        NetworkName,
    },
    wallet::{cashierdb::CashierDb, walletdb::WalletDb},
    zk::circuit::{MintContract, SpendContract},
    Error, Result,
};

use cashierd::service::{bridge, bridge::Bridge};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FeatureNetwork {
    /// Network name
    pub name: String,
    /// Blockchain (mainnet/testnet/etc.)
    pub blockchain: String,
    /// Keypair
    pub keypair: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct CashierdConfig {
    /// The DNS name of the cashier (can also be an IP, or a .onion address)
    pub dns_addr: String,
    /// The endpoint where cashierd will bind its RPC socket
    pub rpc_listen_address: SocketAddr,
    /// Whether to listen with TLS or plain TCP
    pub serve_tls: bool,
    /// Path to DER-formatted PKCS#12 archive. (Unused if serve_tls=false)
    pub tls_identity_path: String,
    /// Password for the TLS identity. (Unused if serve_tls=false)
    pub tls_identity_password: String,
    /// The endpoint to a gatewayd protocol API
    pub gateway_protocol_url: String,
    /// The endpoint to a gatewayd publisher API
    pub gateway_publisher_url: String,
    /// Path to cashierd wallet
    pub cashier_wallet_path: String,
    /// Password for cashierd wallet
    pub cashier_wallet_password: String,
    /// Path to client wallet
    pub client_wallet_path: String,
    /// Password for client wallet
    pub client_wallet_password: String,
    /// Path to database
    pub database_path: String,
    /// Geth IPC endpoint
    pub geth_socket: String,
    /// Geth passphrase
    pub geth_passphrase: String,
    /// The configured networks to use
    pub networks: Vec<FeatureNetwork>,
}

/// Cashierd cli
#[derive(Parser)]
#[clap(name = "cashierd")]
pub struct CliCashierd {
    /// Sets a custom config file
    #[clap(short, long)]
    pub config: Option<String>,
    /// Get Cashier Public key
    #[clap(short, long)]
    pub address: bool,
    /// Increase verbosity
    #[clap(short, parse(from_occurrences))]
    pub verbose: u8,
    /// Refresh the wallet and slabstore
    #[clap(short, long)]
    pub refresh: bool,
}

const CONFIG_FILE_CONTENTS: &[u8] = include_bytes!("../cashierd_config.toml");

fn handle_bridge_error(error_code: u32) -> Result<()> {
    match error_code {
        1 => Err(Error::CashierError("Not Supported Client".into())),
        2 => Err(Error::CashierError("Unable to watch the deposit address".into())),
        3 => Err(Error::CashierError("Unable to send the token".into())),
        _ => Err(Error::CashierError("Unknown error_code".into())),
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
    public_key: Address,
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
    async fn new(config: CashierdConfig, public_key: Address) -> Result<Self> {
        debug!(target: "CASHIER DAEMON", "Initialize");

        let wallet_path =
            format!("sqlite://{}", expand_path(&config.cashier_wallet_path)?.to_str().unwrap());

        let cashier_wallet = CashierDb::new(&wallet_path, &config.cashier_wallet_password).await?;

        let mut networks = Vec::new();

        for network in config.clone().networks {
            networks.push(Network {
                name: NetworkName::from_str(&network.name)?,
                blockchain: network.blockchain,
                keypair: network.keypair,
            });
        }

        let bridge = bridge::Bridge::new();

        Ok(Self { bridge, cashier_wallet, networks, public_key, config })
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
                    debug!(target: "CASHIER DAEMON", "Adding solana network");
                    use cashierd::service::SolClient;

                    let _bridge = self.bridge.clone();

                    let sol_client = SolClient::new(
                        self.cashier_wallet.clone(),
                        &network.blockchain,
                        &network.keypair,
                    )
                    .await?;

                    _bridge.add_clients(NetworkName::Solana, sol_client).await?;
                }

                #[cfg(feature = "eth")]
                NetworkName::Ethereum => {
                    debug!(target: "CASHIER DAEMON", "Adding ethereum network");

                    use cashierd::service::EthClient;

                    let _bridge = self.bridge.clone();

                    let passphrase = self.config.geth_passphrase.clone();

                    let mut eth_client = EthClient::new(
                        &network.blockchain,
                        expand_path(&self.config.geth_socket)?.to_str().unwrap(),
                        &passphrase,
                    );

                    eth_client.setup_keypair(self.cashier_wallet.clone(), &network.keypair).await?;

                    _bridge.add_clients(NetworkName::Ethereum, Arc::new(eth_client)).await?;
                }

                #[cfg(feature = "btc")]
                NetworkName::Bitcoin => {
                    debug!(target: "CASHIER DAEMON", "Adding bitcoin network");
                    use cashierd::service::btc::BtcClient;

                    let _bridge = self.bridge.clone();

                    let btc_client = BtcClient::new(
                        self.cashier_wallet.clone(),
                        &network.blockchain,
                        &network.keypair,
                    )
                    .await?;

                    _bridge.add_clients(NetworkName::Bitcoin, btc_client).await?;
                }
                _ => {}
            }
        }

        client.start().await?;

        let (notify, recv_coin) = async_channel::unbounded::<(PublicKey, u64)>();

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
                    debug!(target: "CASHIER DAEMON", "Received notification from bridge");

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

    async fn listen_for_receiving_coins(
        bridge: Arc<Bridge>,
        cashier_wallet: Arc<CashierDb>,
        recv_coin: async_channel::Receiver<(PublicKey, u64)>,
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
                    return Err(Error::CashierError(
                        "Receive unknown value from Subscription".into(),
                    ))
                }
            }
        }

        Ok(())
    }

    fn check_token_id(network: &NetworkName, _token_id: &str) -> Result<Option<String>> {
        match network {
            #[cfg(feature = "sol")]
            NetworkName::Solana => {
                use cashierd::service::sol::SOL_NATIVE_TOKEN_ID;
                if _token_id != SOL_NATIVE_TOKEN_ID {
                    return Ok(Some(_token_id.to_string()))
                }
                Ok(None)
            }
            #[cfg(feature = "eth")]
            NetworkName::Ethereum => {
                use cashierd::service::eth::ETH_NATIVE_TOKEN_ID;
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

    // RPCAPI:
    // Executes a deposit request given `network` and `token_id`.
    // Returns the address where the deposit shall be transferred to.
    // --> {"jsonrpc": "2.0", "method": "deposit", "params": ["network", "token", "publickey"], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "Ht5G1RhkcKnpLVLMhqJc5aqZ4wYUEbxbtZwGCVbgU7DL", "id": 1}
    async fn deposit(&self, id: Value, params: Value, executor: Arc<Executor<'_>>) -> JsonResult {
        info!(target: "CASHIER DAEMON", "Received deposit request");

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
            let token_id = generate_id2(mint_address, &network)?;

            let mint_address_opt = Self::check_token_id(&network, mint_address)?;

            if mint_address_opt.is_none() {
                mint_address = "";
            }
            let drk_pub_key = Address::from_str(drk_pub_key)?;
            let drk_pub_key: PublicKey = PublicKey::try_from(drk_pub_key)?;

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
                _ => Err(Error::CashierError("Receive unknown value from Subscription".into())),
            }
        }
        .await;

        match result {
            Ok(res) => JsonResult::Resp(jsonresp(json!(res), json!(id))),
            Err(err) => JsonResult::Err(jsonerr(InternalError, Some(err.to_string()), json!(id))),
        }
    }

    // RPCAPI:
    // Executes a withdraw request given `network`, `token_id`, `publickey`
    // and `amount`. `publickey` is supposed to correspond to `network`.
    // Returns the transaction ID of the processed withdraw.
    // --> {"jsonrpc": "2.0", "method": "withdraw", "params": ["network", "token", "publickey", "amount"], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "txID", "id": 1}
    async fn withdraw(&self, id: Value, params: Value) -> JsonResult {
        info!(target: "CASHIER DAEMON", "Received withdraw request");

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
            let token_id: DrkTokenId = generate_id2(mint_address, &network)?;

            let mint_address_opt = Self::check_token_id(&network, mint_address)?;

            if mint_address_opt.is_none() {
                // empty string
                mint_address = "";
            }

            let address = serialize(&address.to_string());

            let cashier_public: PublicKey;

            if let Some(addr) = self
                .cashier_wallet
                .get_withdraw_keys_by_token_public_key(&address, &network)
                .await?
            {
                cashier_public = addr.public;
            } else {
                let cashier_secret = SecretKey::random(&mut OsRng);
                cashier_public = PublicKey::from_secret(cashier_secret);

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

            let cashier_public_str = Address::from(cashier_public).to_string();
            Ok(cashier_public_str)
        }
        .await;

        match result {
            Ok(res) => JsonResult::Resp(jsonresp(json!(res), json!(id))),
            Err(err) => JsonResult::Err(jsonerr(InternalError, Some(err.to_string()), json!(id))),
        }
    }

    // RPCAPI:
    // Returns supported cashier features, like network, listening ports, etc.
    // --> {"jsonrpc": "2.0", "method": "features", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": {"network": ["btc", "sol"]}, "id": 1}
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
            "public_key": self.public_key.to_string(),
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
}

async fn start(
    executor: Arc<Executor<'_>>,
    config: &CashierdConfig,
    get_address_flag: bool,
) -> Result<()> {
    let client_wallet_path =
        format!("sqlite://{}", expand_path(&config.client_wallet_path)?.to_str().unwrap());

    let client_wallet = WalletDb::new(&client_wallet_path, &config.client_wallet_password).await?;

    let rocks = Rocks::new(expand_path(&config.database_path.clone())?.as_path())?;

    info!("Building verifying key for the mint contract...");
    let mint_vk = VerifyingKey::build(11, &MintContract::default());
    info!("Building verifying key for the spend contract...");
    let spend_vk = VerifyingKey::build(11, &SpendContract::default());

    // new Client
    let gateway_urls =
        (config.gateway_protocol_url.parse()?, config.gateway_publisher_url.parse()?);
    let client = Client::new(rocks.clone(), gateway_urls, client_wallet.clone()).await?;

    let tree = client.get_tree().await?;
    let merkle_roots = RocksColumn::<columns::MerkleRoots>::new(rocks.clone());
    let nullifiers = RocksColumn::<columns::Nullifiers>::new(rocks);

    // get cashier public key
    let cashier_public = client.main_keypair.public;

    // new Cashier daemon
    let mut cashierd = Cashierd::new(config.clone(), Address::from(cashier_public)).await?;

    // this will print the cashier public key and exit
    if get_address_flag {
        info!("Public Key: {}", cashierd.public_key);
        return Ok(())
    };

    // new State
    let public_keys = vec![cashier_public];
    let state = Arc::new(Mutex::new(State {
        tree,
        merkle_roots,
        nullifiers,
        public_keys,
        mint_vk,
        spend_vk,
    }));

    // start cashier
    let (t1, t2) = cashierd.start(client, state, executor.clone()).await?;

    // config for rpc
    let cfg = RpcServerConfig {
        socket_addr: config.rpc_listen_address,
        use_tls: config.serve_tls,
        identity_path: expand_path(&config.clone().tls_identity_path)?,
        identity_pass: config.tls_identity_password.clone(),
    };

    // listen and serve RPC
    listen_and_serve(cfg, Arc::new(cashierd), executor).await?;

    t1.cancel().await;
    t2.cancel().await;

    Ok(())
}

#[async_std::main]
async fn main() -> Result<()> {
    let args = CliCashierd::parse();
    let matches = CliCashierd::command().get_matches();

    let config_path = if args.config.is_some() {
        expand_path(&args.config.unwrap())?
    } else {
        join_config_path(&PathBuf::from("cashierd.toml"))?
    };

    // Spawn config file if it's not in place already.
    spawn_config(&config_path, CONFIG_FILE_CONTENTS)?;

    let verbosity_level = matches.occurrences_of("verbose");

    let (lvl, conf) = log_config(verbosity_level)?;

    TermLogger::init(lvl, conf, TerminalMode::Mixed, ColorChoice::Auto)?;

    let config: CashierdConfig = Config::<CashierdConfig>::load(config_path)?;

    if args.refresh {
        info!(target: "CASHIER DAEMON", "Refresh the wallet and the database");

        // refresh cashier's client wallet
        let client_wallet_path =
            format!("sqlite://{}", expand_path(&config.client_wallet_path)?.to_str().unwrap());
        let client_wallet =
            WalletDb::new(&client_wallet_path, &config.client_wallet_password).await?;
        client_wallet.remove_own_coins().await?;

        // refresh cashier wallet
        let wallet_path =
            format!("sqlite://{}", expand_path(&config.cashier_wallet_path)?.to_str().unwrap());
        let wallet = CashierDb::new(&wallet_path, &config.cashier_wallet_password).await?;
        wallet.remove_withdraw_and_deposit_keys().await?;

        // refresh rocks database
        if let Some(path) = expand_path(&config.database_path)?.to_str() {
            info!(target: "CASHIER DAEMON", "Remove database: {}", path);
            std::fs::remove_dir_all(path)?;
        }

        info!("Wallet updated successfully.");
        return Ok(())
    }

    let get_address_flag = args.address;

    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = async_channel::unbounded::<()>();

    let ex2 = ex.clone();

    let nthreads = num_cpus::get();
    debug!(target: "CASHIER DAEMON", "Run {} executor threads", nthreads);

    let (_, result) = Parallel::new()
        .each(0..nthreads, |_| smol::future::block_on(ex.run(shutdown.recv())))
        // Run the main future on the current thread.
        .finish(|| {
            smol::future::block_on(async move {
                start(ex2, &config, get_address_flag).await?;
                drop(signal);
                Ok::<(), darkfi::Error>(())
            })
        });

    result
}
