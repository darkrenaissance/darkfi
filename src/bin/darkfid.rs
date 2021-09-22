use log::*;
use std::fs;
use std::path::PathBuf;

use clap::clap_app;
use serde_json::{json, Value};
use simplelog::{
    CombinedLogger, Config as SimLogConfig, ConfigBuilder, LevelFilter, TermLogger, TerminalMode,
    WriteLogger,
};

use async_std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use drk::{
    cli::{Config, DarkfidConfig},
    rpc::{
        jsonrpc::{error as jsonerr, request as jsonreq, response as jsonresp, send_request},
        jsonrpc::{ErrorCode::*, JsonRequest, JsonResult},
    },
    serial::serialize,
    util::join_config_path,
    wallet::WalletDb,
    Error, Result,
};

#[derive(Clone)]
struct Darkfid {
    verbose: bool,
    config: DarkfidConfig,
    wallet: Arc<WalletDb>,
    tokenlist: Value,
    // clientdb:
    // mint_params:
    // spend_params:
}

impl Darkfid {
    fn new(verbose: bool, config_path: PathBuf) -> Result<Self> {
        let config: DarkfidConfig = Config::<DarkfidConfig>::load(config_path)?;
        let wallet_path = join_config_path(&PathBuf::from("walletdb.db"))?;
        let wallet = WalletDb::new(&PathBuf::from(wallet_path.clone()), config.password.clone())?;
        let file_contents = fs::read_to_string("token/solanatokenlist.json")?;
        let tokenlist: Value = serde_json::from_str(&file_contents)?;

        Ok(Self {
            verbose,
            config,
            wallet,
            tokenlist,
        })
    }

    // TODO: ServerError codes should be part of the lib.
    async fn handle_request(self, req: JsonRequest) -> JsonResult {
        if req.params.as_array().is_none() {
            return JsonResult::Err(jsonerr(InvalidParams, None, req.id));
        }

        debug!(target: "RPC", "--> {:#?}", serde_json::to_string(&req).unwrap());

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
            Some(_) => {}
            None => {}
        };

        return JsonResult::Err(jsonerr(MethodNotFound, None, req.id));
    }

    // --> {"method": "say_hello", "params": []}
    // <-- {"result": "hello world"}
    async fn say_hello(self, id: Value, _params: Value) -> JsonResult {
        JsonResult::Resp(jsonresp(json!("hello world"), id))
    }

    // --> {"method": "create_wallet", "params": []}
    // <-- {"result": true}
    async fn create_wallet(self, id: Value, _params: Value) -> JsonResult {
        match self.wallet.init_db() {
            Ok(()) => return JsonResult::Resp(jsonresp(json!(true), id)),
            Err(e) => {
                return JsonResult::Err(jsonerr(ServerError(-32001), Some(e.to_string()), id))
            }
        }
    }

    // --> {"method": "key_gen", "params": []}
    // <-- {"result": true}
    async fn key_gen(self, id: Value, _params: Value) -> JsonResult {
        match self.wallet.key_gen() {
            Ok((_, _)) => return JsonResult::Resp(jsonresp(json!(true), id)),
            Err(e) => {
                return JsonResult::Err(jsonerr(ServerError(-32002), Some(e.to_string()), id))
            }
        }
    }

    // --> {"method": "get_key", "params": []}
    // <-- {"result": "vdNS7oBj7KvsMWWmo9r96SV4SqATLrGsH2a3PGpCfJC"}
    async fn get_key(self, id: Value, _params: Value) -> JsonResult {
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

    // --> {"jsonrpc": "2.0", "method": "get_token_id",
    //      "params": [token],
    //      "id": 42}
    // <-- {"result": "Ht5G1RhkcKnpLVLMhqJc5aqZ4wYUEbxbtZwGCVbgU7DL"}
    async fn get_token_id(self, id: Value, params: Value) -> JsonResult {
        let args = params.as_array().unwrap();
        let symbol = &args[0];

        if symbol.as_str().is_none() {
            return JsonResult::Err(jsonerr(InvalidParams, None, id));
        };

        let symbol = symbol.as_str().unwrap();

        let token_id = self.search_id(&symbol);
        return JsonResult::Resp(jsonresp(json!(token_id), id));
    }

    // TODO: proper error handling here
    fn search_id(self, symbol: &str) -> Value {
        debug!(target: "DARKFID", "SEARCHING FOR {}", symbol);
        let tokens = self.tokenlist["tokens"]
            .as_array()
            .expect("Can't find 'tokens' in file");
        for item in tokens {
            if item["symbol"] == symbol.to_uppercase() {
                let address = item["address"].clone();
                return address;
            }
        }
        unreachable!();
    }

    // --> {"jsonrpc": "2.0", "method": "features", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": ["network": "btc", "sol"], "id": 42}
    async fn features(self, id: Value, _params: Value) -> JsonResult {
        // TODO: return a dictionary of features
        let req = jsonreq(json!("features"), json!([]));
        let rep: JsonResult;
        match send_request(self.config.cashier_url, json!(req)).await {
            Ok(v) => rep = v,
            Err(e) => {
                return JsonResult::Err(jsonerr(ServerError(-32004), Some(e.to_string()), id))
            }
        }

        match rep {
            JsonResult::Resp(r) => return JsonResult::Resp(r),
            JsonResult::Err(e) => return JsonResult::Err(e),
            JsonResult::Notif(_n) => return JsonResult::Err(jsonerr(InternalError, None, id)),
        }
    }

    // --> {"jsonrpc": "2.0", "method": "deposit",
    //      "params": [network, token, publickey],
    //      "id": 42}
    // The publickey sent here is used so the cashier can know where to send
    // assets once the deposit is received.
    // <-- {"result": "Ht5G1RhkcKnpLVLMhqJc5aqZ4wYUEbxbtZwGCVbgU7DL"}
    async fn deposit(self, id: Value, params: Value) -> JsonResult {
        let args = params.as_array().unwrap();
        if args.len() != 2 {
            return JsonResult::Err(jsonerr(InvalidParams, None, id));
        }

        let network = &args[0];
        let token = &args[1];

        if token.as_str().is_none() {
            return JsonResult::Err(jsonerr(InvalidParams, None, id));
        }

        let tkn_str = token.as_str().unwrap();

        // check if the token input is an ID
        // if not, find the associated ID
        let _token_id = self.clone().parse_token(tkn_str);

        // TODO: Optional sanity checking here, but cashier *must* do so too.

        let pubkey: String;
        match self.wallet.get_keypairs() {
            Ok(v) => {
                let pk = v[0].public;
                pubkey = bs58::encode(serialize(&pk)).into_string();
            }
            Err(e) => {
                return JsonResult::Err(jsonerr(ServerError(-32003), Some(e.to_string()), id))
            }
        }

        // Send request to cashier. If the cashier supports the requested network
        // (and token), it shall return a valid address where assets can be deposited.
        // If not, an error is returned, and forwarded to the method caller.
        let req = jsonreq(json!("deposit"), json!([network, token, pubkey]));
        let rep: JsonResult;
        match send_request(self.config.cashier_url, json!(req)).await {
            Ok(v) => rep = v,
            Err(e) => {
                return JsonResult::Err(jsonerr(ServerError(-32004), Some(e.to_string()), id))
            }
        }

        match rep {
            JsonResult::Resp(r) => return JsonResult::Resp(r),
            JsonResult::Err(e) => return JsonResult::Err(e),
            JsonResult::Notif(_n) => return JsonResult::Err(jsonerr(InternalError, None, id)),
        }
    }

    fn parse_token(self, token: &str) -> Value {
        let vec: Vec<char> = token.chars().collect();
        let mut counter = 0;
        for c in vec {
            if c.is_alphabetic() {
                counter += 1;
            }
        }
        if counter == token.len() {
            let token_id = self.search_id(token);
            return token_id;
        } else {
            let token_id: Value = serde_json::from_str(token).unwrap();
            return token_id;
        }
    }

    // --> {"method": "withdraw", "params": [network, token, publickey, amount]}
    // The publickey sent here is the address where the caller wants to receive
    // the tokens they plan to withdraw.
    // On request, send request to cashier to get deposit address, and then transfer
    // dark assets to the cashier's wallet. Following that, the cashier should return
    // a transaction ID of them sending the funds that are requested for withdrawal.
    // <-- {"result": "txID"}
    async fn withdraw(self, id: Value, params: Value) -> JsonResult {
        let args = params.as_array().unwrap();
        if args.len() != 4 {
            return JsonResult::Err(jsonerr(InvalidParams, None, id));
        }

        let _network = &args[0];
        let _token = &args[1];
        let _address = &args[2];
        let _amount = &args[3];

        // 1. Send request to cashier.
        // 2. Cashier checks if they support the network, and if so,
        //    return adeposit address.
        // 3. We issue a transfer of $amount to the given address.

        return JsonResult::Err(jsonerr(
            ServerError(-32005),
            Some("failed to withdraw".to_string()),
            id,
        ));
    }

    // --> {"method": "transfer", [dToken, address, amount]}
    // <-- {"result": "txID"}
    async fn transfer(self, id: Value, _params: Value) -> JsonResult {
        return JsonResult::Err(jsonerr(
            ServerError(-32006),
            Some("failed to transfer".to_string()),
            id,
        ));
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = clap_app!(darkfid =>
        (@arg CONFIG: -c --config +takes_value "Sets a custom config file")
        (@arg verbose: -v --verbose "Increase verbosity")
    )
    .get_matches();

    let config_path: PathBuf;
    if args.is_present("CONFIG") {
        config_path = PathBuf::from(args.value_of("CONFIG").unwrap());
    } else {
        config_path = join_config_path(&PathBuf::from("darkfid.toml"))?;
    }

    let darkfid = Darkfid::new(args.clone().is_present("verbose"), config_path)?;
    // TODO: TLS
    let listener = TcpListener::bind(darkfid.clone().config.rpc_url).await?;
    debug!(target: "RPC SERVER", "Listening on {}", darkfid.clone().config.rpc_url);

    let logger_config = ConfigBuilder::new().set_time_format_str("%T%.6f").build();
    let debug_level = if args.is_present("verbose") {
        LevelFilter::Debug
    } else {
        LevelFilter::Off
    };

    let log_path = darkfid.clone().config.log_path;
    CombinedLogger::init(vec![
        TermLogger::new(debug_level, logger_config, TerminalMode::Mixed).unwrap(),
        WriteLogger::new(
            LevelFilter::Debug,
            SimLogConfig::default(),
            std::fs::File::create(log_path).unwrap(),
        ),
    ])
    .unwrap();

    loop {
        debug!(target: "RPC SERVER", "waiting for client");

        let (mut socket, _) = listener.accept().await?;
        let darkfid = darkfid.clone();

        debug!(target: "RPC SERVER", "accepted client");

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

                let reply = darkfid.clone().handle_request(r).await;
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

mod tests {

    #[test]
    fn test_token_parsing() {
        let token = "usdc";

        let vec: Vec<char> = token.chars().collect();
        let mut counter = 0;
        for c in vec {
            if c.is_alphabetic() {
                counter += 1;
                println!("Found letter: {}", c)
            }
        }
        if counter == token.len() {
            println!("Every character is a letter");
        }
    }
}
