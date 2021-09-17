use async_std::sync::Arc;
use log::*;
use std::path::PathBuf;

use clap::clap_app;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use simplelog::{
    CombinedLogger, Config as SimLogConfig, ConfigBuilder, LevelFilter, TermLogger, TerminalMode,
    WriteLogger,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use drk::{
    cli::Config,
    rpc::{
        jsonrpc::{error as jsonerr, response as jsonresp},
        jsonrpc::{ErrorCode::*, JsonRequest, JsonResult},
    },
    serial::{deserialize, serialize},
    service::bridge,
    util::join_config_path,
    wallet::CashierDb,
    Result,
};

#[derive(Clone, Serialize, Deserialize, Debug)]
struct CashierdConfig {
    #[serde(rename = "accept_url")]
    pub accept_url: String,

    #[serde(rename = "rpc_url")]
    pub rpc_url: String,

    #[serde(rename = "gateway_url")]
    pub gateway_url: String,

    #[serde(rename = "log_path")]
    pub log_path: String,

    #[serde(rename = "cashierdb_path")]
    pub cashierdb_path: String,

    #[serde(rename = "password")]
    pub password: String,

    #[serde(rename = "client_password")]
    pub client_password: String,
}

#[derive(Clone)]
struct Cashierd {
    verbose: bool,
    config: CashierdConfig,
    wallet: Arc<CashierDb>,
    bridge: Arc<bridge::Bridge>,
    // clientdb:
    // mint_params:
    // spend_params:
}

impl Cashierd {
    fn new(verbose: bool, config_path: PathBuf) -> Result<Self> {
        let config: CashierdConfig = Config::<CashierdConfig>::load(config_path)?;
        let wallet = CashierDb::new(
            &PathBuf::from(config.cashierdb_path.clone()),
            config.password.clone(),
        )?;
        let bridge = bridge::Bridge::new();

        Ok(Self {
            verbose,
            config,
            wallet,
            bridge,
        })
    }

    //async fn bridge_subscribe() -> Result<()> {
    //    Ok(())
    //}

    async fn handle_request(self, req: JsonRequest) -> JsonResult {
        if req.params.as_array().is_none() {
            return JsonResult::Err(jsonerr(InvalidParams, None, req.id));
        }

        debug!(target: "RPC", "--> {:#?}", serde_json::to_string(&req).unwrap());

        match req.method.as_str() {
            //Some("say_hello") => return self.say_hello(req.id, req.params).await,
            //Some("create_wallet") => return self.create_wallet(req.id, req.params).await,
            //Some("key_gen") => return self.key_gen(req.id, req.params).await,
            //Some("get_key") => return self.get_key(req.id, req.params).await,
            Some("deposit") => return self.deposit(req.id, req.params).await,
            //Some("withdraw") => return self.withdraw(req.id, req.params).await,
            //Some("transfer") => return self.transfer(req.id, req.params).await,
            Some(_) => {}
            None => {}
        };

        return JsonResult::Err(jsonerr(MethodNotFound, None, req.id));
    }

    async fn deposit(self, id: Value, params: Value) -> JsonResult {
        debug!(target: "CASHIER", "Received deposit request");
        let args = params.as_array().unwrap();
        if args.len() != 2 {
            return JsonResult::Err(jsonerr(InvalidParams, None, id));
        }

        let _network = &args[0];
        let _token: jubjub::Fr = deserialize(&args[1].as_str().unwrap().as_bytes()).unwrap();
        let pubkey: jubjub::SubgroupPoint =
            deserialize(&args[2].as_str().unwrap().as_bytes()).unwrap();

        // TODO: Sanity check.
        let _check = self
            .wallet
            .get_deposit_coin_keys_by_dkey_public(&pubkey, &serialize(&1));

        // TODO: implement bridge communication
        //let bridge_subscription = bridge.subscribe(ex.clone()).await;
        //bridge_subscribtion
        //    .sender
        //    .send(bridge::BridgeRequests {
        //        token,
        //        payload: bridge::BridgeRequestsPayload::WatchRequest,
        //    })
        //    .await
        //    .unwrap();

        //    let bridge_res = bridge_subscribtion.receiver.recv().await?;

        //    match bridge_res.payload {
        //        bridge::BridgeResponsePayload::WatchResponse(coin_priv, coin_pub) => {
        //            // add pairings to db
        //            let _result = cashier_wallet.put_exchange_keys(
        //                &dpub,
        //                &coin_priv,
        //                &coin_pub,
        //                &serialize(&asset_id),
        //            );

        // TODO: read pubkey from wallet. This is just a stand-in
        let pubkey = bs58::encode(serialize(&pubkey)).into_string();

        JsonResult::Resp(jsonresp(json!(pubkey), json!(id)))
    }
}

#[tokio::main]
async fn main() -> Result<()> {
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
    // TODO: TLS
    let listener = TcpListener::bind(cashierd.clone().config.rpc_url).await?;
    debug!(target: "RPC SERVER", "Listening on {}", cashierd.clone().config.rpc_url);

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
    loop {
        debug!(target: "RPC SERVER", "waiting for client");

        let (mut socket, _) = listener.accept().await?;
        let cashierd = cashierd.clone();

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

                let reply = cashierd.clone().handle_request(r).await;
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
