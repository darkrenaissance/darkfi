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
use ff::Field;
use rand::rngs::OsRng;

use async_std::sync::{Arc, Mutex};
use sha2::{Digest, Sha256};
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
}

impl Cashierd {
    fn new(verbose: bool, config_path: PathBuf) -> Result<Self> {
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
        })
    }

    async fn start(&self, executor: Arc<Executor<'_>>) -> Result<()> {
        self.cashier_wallet.init_db()?;

        let bridge = Bridge::new();

        let sol_feature = String::from("sol");
        let btc_feature = String::from("btc");
        for (feature_name, _) in self.features.iter() {
            match feature_name {
                #[cfg(feature = "sol")]
                sol_feature => {
                    debug!(target: "CASHIER DAEMON", "Add sol network");
                    use drk::service::SolClient;
                    use solana_sdk::signer::keypair::Keypair;
                    let main_keypari = Keypair::new();
                    let _sol_client = SolClient::new(serialize(&main_keypari));
                    //bridge.add_clients(sol_client).await?;
                }
                #[cfg(feature = "btc")]
                btc_feature => {
                    debug!(target: "CASHIER DAEMON", "Add btc network");
                    let btc_endpoint: (bitcoin::network::constants::Network, String) = (
                        bitcoin::network::constants::Network::Bitcoin,
                        String::from("ssl://blockstream.info:993"),
                    );
                    use drk::service::btc::BtcClient;
                    let _btc_client = BtcClient::new(btc_endpoint);
                    //bridge.add_clients(sol_client).await?;
                }
                _ => return Err(Error::NotSupportedNetwork),
            }
        }

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
        let listen_for_receiving_coins_task = executor.spawn(async move {
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

        let rpc_url = self.config.rpc_url.clone();
        run_rpc_server(executor.clone(), self.clone(), rpc_url).await?;

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

    async fn handle_request(self, executor: Arc<Executor<'_>>, req: JsonRequest) -> JsonResult {
        if req.params.as_array().is_none() {
            return JsonResult::Err(jsonerr(InvalidParams, None, req.id));
        }

        debug!(target: "RPC", "--> {:#?}", serde_json::to_string(&req).unwrap());

        match req.method.as_str() {
            Some("deposit") => return self.deposit(executor.clone(), req.id, req.params).await,
            Some("withdraw") => return self.withdraw(req.id, req.params).await,
            Some("features") => return self.features(req.id, req.params).await,
            Some(_) => {}
            None => {}
        };

        return JsonResult::Err(jsonerr(MethodNotFound, None, req.id));
    }

    async fn deposit(self, executor: Arc<Executor<'_>>, id: Value, params: Value) -> JsonResult {
        debug!(target: "CASHIER DAEMON", "RECEIVED DEPOSIT REQUEST");

        let args: &Vec<serde_json::Value>;

        if let Some(ar) = params.as_array() {
            args = ar;
        } else {
            return JsonResult::Err(jsonerr(InvalidParams, None, id));
        }

        let network = &args[0].to_string();
        let token_id = &args[1];
        let drk_pub_key = &args[2];

        if !self.features.contains_key(network) {
            return JsonResult::Err(jsonerr(
                InvalidParams,
                Some(format!("Cashier doesn't support this network: {}", network)),
                id,
            ));
        }

        let result: Result<String> = async {
            let asset_id = Self::parse_id(token_id)?;

            let drk_pub_key = bs58::decode(&drk_pub_key.to_string()).into_vec()?;
            let drk_pub_key: jubjub::SubgroupPoint = deserialize(&drk_pub_key)?;

            // TODO check if the drk public key is already exist
            let _check = self
                .cashier_wallet
                .get_deposit_token_keys_by_dkey_public(&drk_pub_key, &asset_id)?;

            let bridge_subscribtion = self.bridge.subscribe(executor.clone()).await;

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

    // here we hash the alphanumeric token ID. if it fails, we change the last 4 bytes and hash it
    // again, and keep repeating until it works.
    fn parse_id(token: &Value) -> Result<jubjub::Fr> {
        let tkn_str = token.as_str().unwrap();
        if bs58::decode(tkn_str).into_vec().is_err() {
            // TODO: make this an error
            debug!(target: "CASHIER", "COULD NOT DECODE STR");
        }
        let mut data = bs58::decode(tkn_str).into_vec().unwrap();
        let token_id = deserialize::<jubjub::Fr>(&data);
        if token_id.is_err() {
            let mut counter = 0;
            loop {
                data.truncate(28);
                let serialized_counter = serialize(&counter);
                data.extend(serialized_counter.iter());
                let mut hasher = Sha256::new();
                hasher.update(&data);
                let hash = hasher.finalize();
                let token_id = deserialize::<jubjub::Fr>(&hash);
                if token_id.is_err() {
                    counter += 1;
                    continue;
                }
                debug!(target: "CASHIER", "DESERIALIZATION SUCCESSFUL");
                let tkn = token_id.unwrap();
                return Ok(tkn);
            }
        }
        unreachable!();
    }

    async fn withdraw(self, id: Value, params: Value) -> JsonResult {
        debug!(target: "CASHIER DAEMON", "RECEIVED DEPOSIT REQUEST");

        // TODO Cashier checks if they support the network, and if so,
        //    return adeposit address.

        let args: &Vec<serde_json::Value>;

        if let Some(ar) = params.as_array() {
            args = ar;
        } else {
            return JsonResult::Err(jsonerr(InvalidParams, None, id));
        }

        let network = &args[0].to_string();
        let token = &args[1];
        let address = &args[2];
        let _amount = &args[3];

        if !self.features.contains_key(network) {
            return JsonResult::Err(jsonerr(
                InvalidParams,
                Some(format!("Cashier doesn't support this network: {}", network)),
                id,
            ));
        }

        let result: Result<String> = async {
            let asset_id = Self::parse_id(&token)?;
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

    async fn features(self, id: Value, _params: Value) -> JsonResult {
        JsonResult::Resp(jsonresp(json!(self.features), id))
    }
}

async fn run_rpc_server(
    executor: Arc<Executor<'_>>,
    cashierd: Cashierd,
    rpc_url: String,
) -> Result<()> {
    let listener = TcpListener::bind(rpc_url.clone()).await?;
    debug!(target: "RPC SERVER", "Listening on {}", rpc_url);
    loop {
        debug!(target: "RPC SERVER", "waiting for client");

        let (mut socket, _) = listener.accept().await?;

        debug!(target: "RPC SERVER", "accepted client");

        let cashierd = cashierd.clone();
        let ex = executor.clone();
        executor.spawn(async move {
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

                let reply = cashierd.clone().handle_request(ex.clone(), r).await;
                let j = serde_json::to_string(&reply).unwrap();

                debug!(target: "RPC", "<-- {:#?}", j);

                // Write the data back
                if let Err(e) = socket.write_all(j.as_bytes()).await {
                    debug!(target: "RPC SERVER", "failed to write to socket; err = {:?}", e);
                    return;
                }
            }
        }).await;
    }
}

fn main() -> Result<()> {
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

    Ok(())
}

#[cfg(test)]
mod tests {
    use drk::serial::{deserialize, serialize};
    use sha2::{Digest, Sha256};

    #[test]
    fn test_jubjub_parsing() {
        // 1. counter = 0
        // 2. serialized_counter = serialize(counter)
        // 3. asset_id_data = hash(data + serialized_counter)
        // 4. asset_id = deserialize(asset_id_data)
        // 5. test parse
        // 6. loop
        let tkn_str = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";
        println!("{}", tkn_str);
        if bs58::decode(tkn_str).into_vec().is_err() {
            println!("Could not decode str into vec");
        }
        let mut data = bs58::decode(tkn_str).into_vec().unwrap();
        println!("{:?}", data);
        let mut hasher = Sha256::new();
        hasher.update(&data);
        let hash = hasher.finalize();
        let token_id = deserialize::<jubjub::Fr>(&hash);
        println!("{:?}", token_id);
        let mut counter = 0;
        if token_id.is_err() {
            println!("could not deserialize tkn 58");
            loop {
                println!("TOKEN IS NONE. COMMENCING LOOP");
                counter += 1;
                println!("LOOP NUMBER {}", counter);
                println!("{:?}", data.len());
                data.truncate(28);
                let serialized_counter = serialize(&counter);
                println!("{:?}", serialized_counter);
                data.extend(serialized_counter.iter());
                println!("{:?}", data.len());
                let mut hasher = Sha256::new();
                hasher.update(&data);
                let hash = hasher.finalize();
                let token_id = deserialize::<jubjub::Fr>(&hash);
                println!("{:?}", token_id);
                if token_id.is_err() {
                    continue;
                }
                if counter > 10 {
                    break;
                }
                println!("deserialization successful");
                token_id.unwrap();
                break;
            }
        };
    }
}
