use async_std::sync::Arc;
use std::path::PathBuf;

use clap::clap_app;
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use drk::{
    cli::{Config, DarkfidConfig},
    rpc::{
        jsonrpc,
        jsonrpc::{JsonRequest, JsonResult},
    },
    serial::serialize,
    util::join_config_path,
    wallet::WalletDb,
    Error,
};

#[derive(Clone)]
struct Darkfid {
    verbose: bool,
    config: DarkfidConfig,
    wallet: Arc<WalletDb>,
    // clientdb:
    // mint_params:
    // spend_params:
}

impl Darkfid {
    fn new(verbose: bool, config_path: PathBuf) -> Result<Self, Error> {
        let config: DarkfidConfig = Config::<DarkfidConfig>::load(config_path)?;
        let wallet = WalletDb::new(
            &PathBuf::from(config.walletdb_path.clone()),
            config.password.clone(),
        )?;

        Ok(Self {
            verbose,
            config,
            wallet,
        })
    }

    async fn handle_request(self, req: JsonRequest) -> JsonResult {
        if req.params.as_array().is_none() {
            return JsonResult::Err(jsonrpc::error(
                -69,
                "invalid parameters".to_string(),
                req.id,
            ));
        }

        match req.method.as_str() {
            Some("say_hello") => return self.say_hello(req.id, req.params).await,
            Some("create_wallet") => return self.create_wallet(req.id, req.params).await,
            Some("key_gen") => return self.key_gen(req.id, req.params).await,
            Some("get_key") => return self.get_key(req.id, req.params).await,
            Some("deposit") => return self.deposit(req.id, req.params).await,
            Some("withdraw") => return self.withdraw(req.id, req.params).await,
            Some("transfer") => return self.transfer(req.id, req.params).await,
            Some(_) => {}
            None => {}
        };

        return JsonResult::Err(jsonrpc::error(
            -69,
            "method not implemented".to_string(),
            req.id,
        ));
    }

    async fn say_hello(self, id: Value, _params: Value) -> JsonResult {
        JsonResult::Resp(jsonrpc::response(json!("hello world"), id))
    }

    async fn create_wallet(self, id: Value, _params: Value) -> JsonResult {
        match self.wallet.init_db() {
            Ok(()) => return JsonResult::Resp(jsonrpc::response(json!(true), id)),
            Err(e) => return JsonResult::Err(jsonrpc::error(-69, e.to_string(), id)),
        }
    }

    async fn key_gen(self, id: Value, _params: Value) -> JsonResult {
        match self.wallet.key_gen() {
            Ok((_, _)) => return JsonResult::Resp(jsonrpc::response(json!(true), id)),
            Err(e) => return JsonResult::Err(jsonrpc::error(-69, e.to_string(), id)),
        }
    }

    async fn get_key(self, id: Value, _params: Value) -> JsonResult {
        match self.wallet.get_keypairs() {
            Ok(v) => {
                let pk = v[0].public;
                let b58 = bs58::encode(serialize(&pk)).into_string();
                return JsonResult::Resp(jsonrpc::response(json!(b58), id));
            }
            Err(e) => return JsonResult::Err(jsonrpc::error(-69, e.to_string(), id)),
        }
    }

    // --> {"jsonrpc": "2.0", "method": "deposit",
    //      "params": [network, token, publickey],
    //      "id": 42}
    // The publickey sent here is used so the cashier can know where to send
    // assets once the deposit is received.
    async fn deposit(self, id: Value, params: Value) -> JsonResult {
        let args = params.as_array().unwrap();
        if args.len() != 2 {
            return JsonResult::Err(jsonrpc::error(-69, "missing parameters".to_string(), id));
        }

        let network = &args[0];
        let token = &args[1];
        // TODO: Optional sanity checking here, but cashier *must* do so too.

        let pubkey: String;
        match self.wallet.get_keypairs() {
            Ok(v) => {
                let pk = v[0].public;
                pubkey = bs58::encode(serialize(&pk)).into_string();
            }
            Err(e) => return JsonResult::Err(jsonrpc::error(-69, e.to_string(), id)),
        }

        // Send request to cashier. If the cashier supports the requested network
        // (and token), it shall return a valid address where assets can be deposited.
        // If not, an error is returned, and forwarded to the method caller.
        let req = jsonrpc::request(json!("deposit"), json!([network, token, pubkey]));
        let rep: JsonResult;
        match jsonrpc::send_request(self.config.cashier_url, json!(req)).await {
            Ok(v) => rep = v,
            Err(e) => return JsonResult::Err(jsonrpc::error(-69, e.to_string(), id)),
        }

        match rep {
            JsonResult::Resp(r) => return JsonResult::Resp(r),
            JsonResult::Err(e) => return JsonResult::Err(e),
            JsonResult::Notif(_n) => {
                return JsonResult::Err(jsonrpc::error(
                    -69,
                    "invalid reply from cashier".to_string(),
                    id,
                ))
            }
        }
    }

    async fn withdraw(self, id: Value, params: Value) -> JsonResult {
        let args = params.as_array().unwrap();
        if args.len() != 4 {
            return JsonResult::Err(jsonrpc::error(-69, "missing parameters".to_string(), id));
        }

        let network = &args[0];
        let token = &args[1];
        let address = &args[2];
        let amount = &args[3];

        // 1. Send request to cashier.
        // 2. Cashier checks if they support the network, and if so,
        //    return adeposit address.
        // 3. We issue a transfer of $amount to the given address.

        return JsonResult::Err(jsonrpc::error(-69, "failed to withdraw".to_string(), id));
    }

    async fn transfer(self, id: Value, _params: Value) -> JsonResult {
        return JsonResult::Err(jsonrpc::error(-69, "failed to transfer".to_string(), id));
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
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

    loop {
        let (mut socket, _) = listener.accept().await?;

        println!("Accepted client");
        let darkfid = darkfid.clone();

        tokio::spawn(async move {
            let mut buf = [0; 2048];

            loop {
                let n = match socket.read(&mut buf).await {
                    Ok(n) if n == 0 => {
                        println!("Closed connection");
                        return;
                    }
                    Ok(n) => n,
                    Err(e) => {
                        eprintln!("failed to read from socket; err = {:?}", e);
                        return;
                    }
                };

                let r: JsonRequest = match serde_json::from_slice(&buf[0..n]) {
                    Ok(r) => r,
                    Err(_) => {
                        eprintln!("received invalid json");
                        return;
                    }
                };

                let reply = darkfid.clone().handle_request(r).await;
                let j = serde_json::to_string(&reply).unwrap();

                // Write the data back
                if let Err(e) = socket.write_all(j.as_bytes()).await {
                    eprintln!("failed to writeto socket; err = {:?}", e);
                    return;
                }
            }
        });
    }
}
