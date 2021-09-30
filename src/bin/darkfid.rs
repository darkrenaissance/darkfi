use async_trait::async_trait;
use clap::clap_app;
use log::debug;
use serde_json::{json, Value};

use async_std::sync::{Arc, Mutex};
use std::path::PathBuf;
//use std::sync::Arc;

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
    util::{assign_id, decimals, decode_base10, expand_path, join_config_path, TokenList},
    wallet::WalletDb,
    Result,
};

#[derive(Clone)]
struct Darkfid {
    config: DarkfidConfig,
    wallet: Arc<WalletDb>,
    client: Arc<Mutex<Client>>,
    tokenlist: TokenList,
}

#[async_trait]
impl RequestHandler for Darkfid {
    // TODO: ServerError codes should be part of the lib.
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
    fn new(config_path: PathBuf) -> Result<Self> {
        let config: DarkfidConfig = Config::<DarkfidConfig>::load(config_path)?;
        let wallet = WalletDb::new(
            expand_path(&config.wallet_path)?.as_path(),
            config.wallet_password.clone(),
        )?;
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
        )?;
        let client = Arc::new(Mutex::new(client));

        let tokenlist = TokenList::new()?;

        Ok(Self {
            config,
            wallet,
            client,
            tokenlist,
        })
    }

    // --> {"method": "say_hello", "params": []}
    // <-- {"result": "hello world"}
    async fn say_hello(&self, id: Value, _params: Value) -> JsonResult {
        JsonResult::Resp(jsonresp(json!("hello world"), id))
    }

    // --> {"method": "create_wallet", "params": []}
    // <-- {"result": true}
    async fn create_wallet(&self, id: Value, _params: Value) -> JsonResult {
        match self.wallet.init_db().await {
            Ok(()) => return JsonResult::Resp(jsonresp(json!(true), id)),
            Err(e) => {
                return JsonResult::Err(jsonerr(ServerError(-32001), Some(e.to_string()), id))
            }
        }
    }

    // --> {"method": "key_gen", "params": []}
    // <-- {"result": true}
    async fn key_gen(&self, id: Value, _params: Value) -> JsonResult {
        match self.wallet.key_gen() {
            Ok(()) => return JsonResult::Resp(jsonresp(json!(true), id)),
            Err(e) => {
                return JsonResult::Err(jsonerr(ServerError(-32002), Some(e.to_string()), id))
            }
        }
    }

    // --> {"method": "get_key", "params": []}
    // <-- {"result": "vdNS7oBj7KvsMWWmo9r96SV4SqATLrGsH2a3PGpCfJC"}
    async fn get_key(&self, id: Value, _params: Value) -> JsonResult {
        match self.wallet.get_keypairs() {
            Ok(v) => {
                let pk = v[0].public;
                let b58 = bs58::encode(serialize(&pk)).into_string();
                return JsonResult::Resp(jsonresp(json!(b58), id));
            }
            Err(e) => {
                return JsonResult::Err(jsonerr(ServerError(-32003), Some(e.to_string()), id))
            }
        }
    }

    // --> {"method": "get_token_id", "params": [token]}
    // <-- {"result": "Ht5G1RhkcKnpLVLMhqJc5aqZ4wYUEbxbtZwGCVbgU7DL"}
    async fn get_token_id(&self, id: Value, params: Value) -> JsonResult {
        let args = params.as_array();

        if args.is_none() {
            return JsonResult::Err(jsonerr(InvalidParams, None, id));
        }

        let args = args.unwrap();
        let symbol = args[0].as_str();

        if symbol.is_none() {
            return JsonResult::Err(jsonerr(InvalidParams, None, id));
        }
        let symbol = symbol.unwrap();

        let result: Result<Value> = async {
            let token_id = self.tokenlist.clone().search_id(symbol)?;
            Ok(json!(token_id))
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
        // TODO: return a dictionary of features
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
            return JsonResult::Err(jsonerr(InvalidParams, None, id));
        }

        let token = token.as_str().unwrap();

        if network.as_str().is_none() {
            return JsonResult::Err(jsonerr(InvalidParams, None, id));
        }

        let network = network.as_str().unwrap();

        let token_id = match assign_id(&network, &token, self.tokenlist.clone()) {
            Ok(t) => t,
            Err(_e) => {
                debug!(target: "DARKFID", "TOKEN ID IS ERR");
                // TODO: this should return the relevant drk error
                // right now it just flattens it into ParseError
                return JsonResult::Err(jsonerr(ParseError, None, id));
            }
        };

        // TODO: Optional sanity checking here, but cashier *must* do so too.

        let pubkey: String;
        match self.wallet.get_keypairs() {
            Ok(v) => {
                let pk = v[0].public;
                let pk = serialize(&pk);
                pubkey = bs58::encode(pk).into_string();
            }
            Err(e) => {
                return JsonResult::Err(jsonerr(ServerError(-32003), Some(e.to_string()), id))
            }
        }

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
            return JsonResult::Err(jsonerr(InvalidParams, None, id));
        }

        let token = token.as_str().unwrap();

        if network.as_str().is_none() {
            return JsonResult::Err(jsonerr(InvalidParams, None, id));
        }

        let network = network.as_str().unwrap();

        if amount.as_f64().is_none() {
            return JsonResult::Err(jsonerr(InvalidParams, None, id));
        }

        let amount = amount.as_str().unwrap();

        // TODO: get rid of these unwraps
        let decimals = decimals(network, token, self.tokenlist.clone()).unwrap();
        let amount_in_apo = decode_base10(amount, decimals).unwrap();

        let token_id = match assign_id(&network, &token, self.tokenlist.clone()) {
            Ok(t) => t,
            Err(_e) => {
                debug!(target: "DARKFID", "TOKEN ID IS ERR");
                // TODO: this should return the relevant drk error
                // right now it just flattens it into ParseError
                return JsonResult::Err(jsonerr(ParseError, None, id));
            }
        };

        let req = jsonreq(
            json!("withdraw"),
            json!([network, token_id, address, amount_in_apo]),
        );
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

        let token = &args[0];
        let address = &args[1];
        let amount = &args[2];

        if token.as_str().is_none() {
            return JsonResult::Err(jsonerr(InvalidParams, None, id));
        }

        let token = address.as_str().unwrap();

        if address.as_str().is_none() {
            return JsonResult::Err(jsonerr(InvalidParams, None, id));
        }

        let address = address.as_str().unwrap();

        if amount.as_f64().is_none() {
            return JsonResult::Err(jsonerr(InvalidParams, None, id));
        }

        let amount = amount.as_f64().unwrap();

        // TODO: get tokenID from walletdb
        //let result: Result<()> = async {
        //    let token_id = parse_wrapped_token(token, self.tokenlist.clone())?;
        //    let address = bs58::decode(&address).into_vec()?;
        //    let address: jubjub::SubgroupPoint = deserialize(&address)?;
        //    self.client
        //        .lock()
        //        .await
        //        .transfer(token_id, address, amount)
        //        .await?;
        //    Ok(())
        //}
        //.await;

        //match result {
        //    Ok(res) => JsonResult::Resp(jsonresp(json!(res), json!(id))),
        //    Err(err) => JsonResult::Err(jsonerr(InternalError, Some(err.to_string()), json!(id))),
        //}
        return JsonResult::Err(jsonerr(
            ServerError(-32005),
            Some("failed to withdraw".to_string()),
            id,
        ));
    }
}

#[async_std::main]
async fn main() -> Result<()> {
    let args = clap_app!(darkfid =>
        (@arg CONFIG: -c --config +takes_value "Sets a custom config file")
        (@arg verbose: -v --verbose "Increase verbosity")
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

    let darkfid = Darkfid::new(config_path)?;

    let server_config = RpcServerConfig {
        socket_addr: darkfid.config.rpc_listen_address.clone(),
        use_tls: darkfid.config.serve_tls,
        identity_path: expand_path(&darkfid.config.tls_identity_path.clone())?,
        identity_pass: darkfid.config.tls_identity_password.clone(),
    };

    listen_and_serve(server_config, darkfid).await
}

mod tests {

    //#[test]
    //fn test_token_parsing() {
    //    let token = "usdc";

    //    let vec: Vec<char> = token.chars().collect();
    //    let mut counter = 0;
    //    for c in vec {
    //        if c.is_alphabetic() {
    //            counter += 1;
    //            println!("Found letter: {}", c)
    //        }
    //    }
    //    if counter == token.len() {
    //        println!("Every character is a letter");
    //    }
    //}
}
