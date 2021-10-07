use drk::{
    blockchain::Rocks,
    cli::{Config, DarkfidConfig},
    client::Client,
    rpc::{
        jsonrpc::{error as jsonerr, request as jsonreq, response as jsonresp, send_request},
        jsonrpc::{ErrorCode::*, JsonRequest, JsonResult},
        rpcserver::{listen_and_serve, RequestHandler, RpcServerConfig},
    },
    serial::{deserialize, serialize},
    util::{
        assign_id, decimals, decode_base10, expand_path, join_config_path, DrkTokenList,
        NetworkName, SolTokenList,
    },
    wallet::WalletDb,
    Error, Result,
};

use async_trait::async_trait;
use clap::clap_app;
use log::debug;
use serde_json::{json, Value};

use async_std::sync::{Arc, Mutex};
use std::path::PathBuf;
use std::str::FromStr;

struct Darkfid {
    config: DarkfidConfig,
    client: Arc<Mutex<Client>>,
    sol_tokenlist: SolTokenList,
    drk_tokenlist: DrkTokenList,
}

#[async_trait]
impl RequestHandler for Darkfid {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        if req.params.as_array().is_none() {
            return JsonResult::Err(jsonerr(InvalidParams, None, req.id));
        }

        debug!(target: "RPC", "--> {}", serde_json::to_string(&req).unwrap());

        match req.method.as_str() {
            Some("say_hello") => return self.say_hello(req.id, req.params).await,
            Some("create_wallet") => return self.create_wallet(req.id, req.params).await,
            Some("key_gen") => return self.key_gen(req.id, req.params).await,
            Some("get_key") => return self.get_key(req.id, req.params).await,
            Some("get_token_id") => return self.get_token_id(req.id, req.params).await,
            Some("features") => return self.features(req.id, req.params).await,
            Some("deposit") => return self.deposit(req.id, req.params).await,
            Some("withdraw") => return self.withdraw(req.id, req.params).await,
            Some("transfer") => return self.transfer(req.id, req.params).await,
            Some(_) | None => return JsonResult::Err(jsonerr(MethodNotFound, None, req.id)),
        };
    }
}

impl Darkfid {
    async fn new(config: DarkfidConfig, wallet: Arc<WalletDb>) -> Result<Self> {
        debug!(target: "DARKFID", "INIT WALLET WITH PATH {}", config.wallet_path);

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
            wallet.clone(),
        )
        .await?;

        let client = Arc::new(Mutex::new(client));

        let sol_tokenlist = SolTokenList::new()?;
        let drk_tokenlist = DrkTokenList::new(sol_tokenlist.clone())?;

        Ok(Self {
            config,
            client,
            sol_tokenlist,
            drk_tokenlist,
        })
    }

    async fn start(&mut self) -> Result<()> {
        self.client.lock().await.start().await?;
        self.client.lock().await.connect_to_subscriber().await?;

        Ok(())
    }

    // --> {"method": "say_hello", "params": []}
    // <-- {"result": "hello world"}
    async fn say_hello(&self, id: Value, _params: Value) -> JsonResult {
        JsonResult::Resp(jsonresp(json!("hello world"), id))
    }

    // --> {"method": "create_wallet", "params": []}
    // <-- {"result": true}
    async fn create_wallet(&self, id: Value, _params: Value) -> JsonResult {
        match self.client.lock().await.init_db().await {
            Ok(()) => return JsonResult::Resp(jsonresp(json!(true), id)),
            Err(e) => {
                return JsonResult::Err(jsonerr(ServerError(-32001), Some(e.to_string()), id))
            }
        }
    }

    // --> {"method": "key_gen", "params": []}
    // <-- {"result": true}
    async fn key_gen(&self, id: Value, _params: Value) -> JsonResult {
        match self.client.lock().await.key_gen().await {
            Ok(()) => return JsonResult::Resp(jsonresp(json!(true), id)),
            Err(e) => {
                return JsonResult::Err(jsonerr(ServerError(-32002), Some(e.to_string()), id))
            }
        }
    }

    // --> {"method": "get_key", "params": []}
    // <-- {"result": "vdNS7oBj7KvsMWWmo9r96SV4SqATLrGsH2a3PGpCfJC"}
    async fn get_key(&self, id: Value, _params: Value) -> JsonResult {
        let pk = self.client.lock().await.main_keypair.public;
        let b58 = bs58::encode(serialize(&pk)).into_string();
        return JsonResult::Resp(jsonresp(json!(b58), id));
    }

    // --> {"method": "get_token_id", "params": [network, token]}
    // <-- {"result": "Ht5G1RhkcKnpLVLMhqJc5aqZ4wYUEbxbtZwGCVbgU7DL"}
    async fn get_token_id(&self, id: Value, params: Value) -> JsonResult {
        let args = params.as_array();

        if args.is_none() {
            return JsonResult::Err(jsonerr(InvalidParams, None, id));
        }

        let args = args.unwrap();

        let network = args[0].as_str();
        let symbol = args[1].as_str();

        if network.is_none() {
            return JsonResult::Err(jsonerr(InvalidNetworkParam, None, id));
        }

        if symbol.is_none() {
            return JsonResult::Err(jsonerr(InvalidSymbolParam, None, id));
        }
        let symbol = symbol.unwrap();

        let result: Result<Value> = async {
            let network = NetworkName::from_str(&network.unwrap())?;
            match network {
                #[cfg(feature = "sol")]
                NetworkName::Solana => {
                    let token_id = self.sol_tokenlist.search_id(symbol)?;
                    Ok(json!(token_id))
                }
                #[cfg(feature = "btc")]
                NetworkName::Bitcoin => {
                    return Err(Error::NotSupportedToken);
                }
                _ => Err(Error::NotSupportedNetwork),
            }
        }
        .await;

        match result {
            Ok(res) => JsonResult::Resp(jsonresp(json!(res), json!(res))),
            Err(err) => JsonResult::Err(jsonerr(InternalError, Some(err.to_string()), json!(id))),
        }
    }

    // --> {""method": "features", "params": []}
    // <-- {"result": { "network": ["btc", "sol"] } }
    async fn features(&self, id: Value, _params: Value) -> JsonResult {
        let req = jsonreq(json!("features"), json!([]));
        let rep: JsonResult;
        match send_request(&self.config.cashier_rpc_url, json!(req)).await {
            Ok(v) => rep = v,
            Err(e) => {
                return JsonResult::Err(jsonerr(ServerError(-32004), Some(e.to_string()), id))
            }
        }

        match rep {
            JsonResult::Resp(r) => return JsonResult::Resp(r),
            JsonResult::Err(e) => return JsonResult::Err(e),
            JsonResult::Notif(_) => return JsonResult::Err(jsonerr(InternalError, None, id)),
        }
    }

    // --> {"method": "deposit", "params": [network, token, publickey]}
    // The publickey sent here is used so the cashier can know where to send
    // assets once the deposit is received.
    // <-- {"result": "Ht5G1RhkcKnpLVLMhqJc5aqZ4wYUEbxbtZwGCVbgU7DL"}
    async fn deposit(&self, id: Value, params: Value) -> JsonResult {
        let args = params.as_array();

        if args.is_none() {
            return JsonResult::Err(jsonerr(InvalidParams, None, id));
        }

        let args = args.unwrap();
        if args.len() != 2 {
            return JsonResult::Err(jsonerr(InvalidParams, None, id));
        }

        let network = &args[0];
        let token = &args[1];

        if token.as_str().is_none() {
            return JsonResult::Err(jsonerr(InvalidTokenIdParam, None, id));
        }

        let token = token.as_str().unwrap();

        if network.as_str().is_none() {
            return JsonResult::Err(jsonerr(InvalidNetworkParam, None, id));
        }

        let network = network.as_str().unwrap();

        let token_id = match assign_id(&network, &token, &self.sol_tokenlist) {
            Ok(t) => t,
            Err(e) => {
                return JsonResult::Err(jsonerr(InternalError, Some(e.to_string()), id));
            }
        };

        // TODO: Optional sanity checking here, but cashier *must* do so too.

        let pk = self.client.lock().await.main_keypair.public;
        let pubkey = bs58::encode(serialize(&pk)).into_string();

        // Send request to cashier. If the cashier supports the requested network
        // (and token), it shall return a valid address where assets can be deposited.
        // If not, an error is returned, and forwarded to the method caller.
        let req = jsonreq(json!("deposit"), json!([network, token_id, pubkey]));
        let rep: JsonResult;
        match send_request(&self.config.cashier_rpc_url, json!(req)).await {
            Ok(v) => rep = v,
            Err(e) => {
                debug!(target: "DARKFID", "REQUEST IS ERR");
                return JsonResult::Err(jsonerr(ServerError(-32004), Some(e.to_string()), id));
            }
        }

        match rep {
            JsonResult::Resp(r) => return JsonResult::Resp(r),
            JsonResult::Err(e) => return JsonResult::Err(e),
            JsonResult::Notif(_n) => return JsonResult::Err(jsonerr(InternalError, None, id)),
        }
    }

    // --> {"method": "withdraw", "params": [network, token, publickey, amount]}
    // The publickey sent here is the address where the caller wants to receive
    // the tokens they plan to withdraw.
    // On request, send request to cashier to get deposit address, and then transfer
    // dark assets to the cashier's wallet. Following that, the cashier should return
    // a transaction ID of them sending the funds that are requested for withdrawal.
    // <-- {"result": "txID"}
    async fn withdraw(&self, id: Value, params: Value) -> JsonResult {
        let args = params.as_array();

        if args.is_none() {
            return JsonResult::Err(jsonerr(InvalidParams, None, id));
        }

        let args = args.unwrap();

        if args.len() != 4 {
            return JsonResult::Err(jsonerr(InvalidParams, None, id));
        }

        let network = &args[0];
        let token = &args[1];
        let address = &args[2];
        let amount = &args[3];

        if token.as_str().is_none() {
            return JsonResult::Err(jsonerr(InvalidTokenIdParam, None, id));
        }

        let token = token.as_str().unwrap();

        if network.as_str().is_none() {
            return JsonResult::Err(jsonerr(InvalidNetworkParam, None, id));
        }

        let network = network.as_str().unwrap();

        if amount.as_f64().is_none() {
            return JsonResult::Err(jsonerr(InvalidAmountParam, None, id));
        }

        let amount = amount.as_f64().unwrap();

        let decimals = match decimals(network, token, &self.sol_tokenlist) {
            Ok(d) => d,
            Err(e) => {
                return JsonResult::Err(jsonerr(InternalError, Some(e.to_string()), id));
            }
        };

        let amount_in_apo = match decode_base10(&amount.to_string(), decimals, true) {
            Ok(a) => a,
            Err(e) => {
                return JsonResult::Err(jsonerr(InternalError, Some(e.to_string()), id));
            }
        };

        let token_id = match assign_id(&network, &token, &self.sol_tokenlist) {
            Ok(t) => t,
            Err(e) => {
                return JsonResult::Err(jsonerr(InternalError, Some(e.to_string()), id));
            }
        };

        let req = jsonreq(
            json!("withdraw"),
            json!([network, token_id, address, amount_in_apo]),
        );
        let mut rep: JsonResult;
        match send_request(&self.config.cashier_rpc_url, json!(req)).await {
            Ok(v) => rep = v,
            Err(e) => {
                return JsonResult::Err(jsonerr(ServerError(-32004), Some(e.to_string()), id));
            }
        }

        let token_id: &jubjub::Fr;

        // get the id for the token
        if let Some(tk_id) = self.drk_tokenlist.tokens.get(&token.to_uppercase()) {
            token_id = tk_id;
        } else {
            return JsonResult::Err(jsonerr(InvalidParams, None, id));
        }

        // send drk to cashier_public
        if let JsonResult::Resp(cashier_public) = &rep {
            let result: Result<()> = async {
                let cashier_public = cashier_public.result.as_str().unwrap();

                let cashier_public: jubjub::SubgroupPoint =
                    deserialize(&bs58::decode(cashier_public).into_vec()?)?;

                let decimals: usize = 8;
                let amount = decode_base10(&amount.to_string(), decimals, true)?;

                self.client
                    .lock()
                    .await
                    .transfer(token_id.clone(), cashier_public, amount)
                    .await?;

                Ok(())
            }
            .await;

            match result {
                Err(e) => {
                    rep = JsonResult::Err(jsonerr(InternalError, Some(e.to_string()), id.clone()))
                }
                Ok(_) => {
                    rep = JsonResult::Resp(jsonresp(
                        json!(format!(
                            "Sent request to withdraw {} amount of {}",
                            amount, token_id
                        )),
                        json!(id.clone()),
                    ))
                }
            }
        };

        match rep {
            JsonResult::Resp(r) => return JsonResult::Resp(r),
            JsonResult::Err(e) => return JsonResult::Err(e),
            JsonResult::Notif(_n) => return JsonResult::Err(jsonerr(InternalError, None, id)),
        }
    }

    // --> {"method": "transfer", [dToken, address, amount]}
    // <-- {"result": "txID"}
    async fn transfer(&self, id: Value, params: Value) -> JsonResult {
        let args = params.as_array();
        if args.is_none() {
            return JsonResult::Err(jsonerr(InvalidParams, None, id));
        }
        let args = args.unwrap();
        if args.len() != 3 {
            return JsonResult::Err(jsonerr(InvalidParams, None, id));
        }

        let token = &args[0].as_str();
        let address = &args[1].as_str();
        let amount = &args[2].as_f64();

        if token.is_none() {
            return JsonResult::Err(jsonerr(InvalidTokenIdParam, None, id));
        }

        let token = token.unwrap();

        if address.is_none() {
            return JsonResult::Err(jsonerr(InvalidAddressParam, None, id));
        }

        let address = address.unwrap();

        if amount.is_none() {
            return JsonResult::Err(jsonerr(InvalidAmountParam, None, id));
        }
        let amount = amount.unwrap();

        let token_id: &jubjub::Fr;

        // get the id for the token
        if let Some(tk_id) = self.drk_tokenlist.tokens.get(&token.to_uppercase()) {
            token_id = tk_id;
        } else {
            return JsonResult::Err(jsonerr(InvalidParams, None, id));
        }

        let result: Result<()> = async {
            let drk_address = bs58::decode(&address).into_vec()?;
            let drk_address: jubjub::SubgroupPoint = deserialize(&drk_address)?;

            let decimals: usize = 8;
            let amount = decode_base10(&amount.to_string(), decimals, true)?;

            self.client.lock().await.transfer(token_id.clone(), drk_address, amount).await?;

            Ok(())
        }
        .await;

        match result {
            Ok(msg) => JsonResult::Resp(jsonresp(json!(msg), json!(id))),
            Err(err) => JsonResult::Err(jsonerr(InternalError, Some(err.to_string()), json!(id))),
        }
    }
}

#[async_std::main]
async fn main() -> Result<()> {
    let args = clap_app!(darkfid =>
        (@arg CONFIG: -c --config +takes_value "Sets a custom config file")
        (@arg verbose: -v --verbose "Increase verbosity")
        (@subcommand cashier =>
            (about: "Manage cashier public key")
            (@arg GETCASHIERKEY: --get "Get cashier public key")
            (@arg SETCASHIERKEY: --set +takes_value "Sets cashier public key")
        )
    )
    .get_matches();

    let config_path = if args.is_present("CONFIG") {
        PathBuf::from(args.value_of("CONFIG").unwrap())
    } else {
        join_config_path(&PathBuf::from("darkfid.toml"))?
    };

    let loglevel = if args.is_present("verbose") {
        log::Level::Debug
    } else {
        log::Level::Info
    };

    simple_logger::init_with_level(loglevel)?;

    let config: DarkfidConfig = Config::<DarkfidConfig>::load(config_path)?;

    let wallet = WalletDb::new(
        expand_path(&config.wallet_path)?.as_path(),
        config.wallet_password.clone(),
    )?;

    if let Some(matches) = args.subcommand_matches("cashier") {
        if matches.is_present("GETCASHIERKEY") {
            let cashier_public = wallet.get_cashier_public_keys()?[0];
            let cashier_public = bs58::encode(&serialize(&cashier_public)).into_string();
            println!("Cashier Public Key: {}", cashier_public);
            return Ok(());
        }

        if matches.is_present("SETCASHIERKEY") {
            let cashier_public = matches.value_of("SETCASHIERKEY").unwrap();

            let cashier_public: jubjub::SubgroupPoint =
                deserialize(&bs58::decode(cashier_public).into_vec()?)?;
            wallet.put_cashier_pub(&cashier_public)?;
            println!("Cashier public key set successfully");
            return Ok(());
        }
    }

    let mut darkfid = Darkfid::new(config.clone(), wallet.clone()).await?;

    let server_config = RpcServerConfig {
        socket_addr: config.rpc_listen_address.clone(),
        use_tls: config.serve_tls,
        identity_path: expand_path(&config.tls_identity_path.clone())?,
        identity_pass: config.tls_identity_password.clone(),
    };

    darkfid.start().await?;
    listen_and_serve(server_config, Arc::new(darkfid)).await
}
