use std::{net::SocketAddr, path::PathBuf, sync::Arc, thread, time};

use async_executor::Executor;
use async_trait::async_trait;
use easy_parallel::Parallel;
use log::{debug, error, info};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use simplelog::{ColorChoice, TermLogger, TerminalMode};
use structopt::StructOpt;
use structopt_toml::StructOptToml;

use darkfi::{
    net,
    rpc::{
        jsonrpc,
        jsonrpc::{
            response as jsonresp,
            ErrorCode::{InvalidParams, MethodNotFound, ServerError},
            JsonRequest, JsonResult,
        },
        rpcserver::{listen_and_serve, RequestHandler, RpcServerConfig},
    },
    util::{
        cli::{log_config, spawn_config},
        path::get_config_path,
    },
    Result,
};

mod protocol_tx_pool;
mod tx_pool;

use crate::{
    protocol_tx_pool::ProtocolTxPool,
    tx_pool::{SeenTxHashes, SeenTxHashesPtr, Tx},
};

const CONFIG_FILE: &str = r"validatord_config.toml";
const CONFIG_FILE_CONTENTS: &[u8] = include_bytes!("../validatord_config.toml");

#[derive(Debug, Deserialize, Serialize, StructOpt, StructOptToml)]
#[serde(default)]
struct Opt {
    #[structopt(short, long, default_value = CONFIG_FILE)]
    /// Configuration file to use
    config: String,
    #[structopt(long)]
    /// Accept address
    accept: Option<SocketAddr>,
    #[structopt(long)]
    /// Seed nodes
    seeds: Vec<SocketAddr>,
    #[structopt(long)]
    /// Manual connections
    connect: Vec<SocketAddr>,
    #[structopt(long, default_value = "0")]
    /// Connection slots
    slots: u32,
    #[structopt(long)]
    /// External address
    external: Option<SocketAddr>,
    #[structopt(long, default_value = "/tmp/darkfid.log")]
    /// Logfile path
    log: String,
    #[structopt(long, default_value = "127.0.0.1:9000")]
    /// The endpoint where validatord will bind its RPC socket
    rpc: SocketAddr,
    #[structopt(long)]
    /// Whether to listen with TLS or plain TCP
    serve_tls: bool,
    #[structopt(long, default_value = "~/.config/darkfi/validatord_identity.pfx")]
    /// TLS certificate to use
    tls_identity_path: PathBuf,
    #[structopt(long, default_value = "FOOBAR")]
    /// Password for the created TLS identity
    tls_identity_password: String,
    #[structopt(short, long, default_value = "0")]
    /// How many threads to utilize
    threads: usize,
    #[structopt(short, long, parse(from_occurrences))]
    /// Multiple levels can be used (-vv)
    verbose: u8,
}

fn proposal_task() {
    loop {
        info!("Waiting for next epoch({:?} sec)...", 20);
        thread::sleep(time::Duration::from_secs(20));
    }
}

async fn start(executor: Arc<Executor<'_>>, opts: &Opt) -> Result<()> {
    let rpc_server_config = RpcServerConfig {
        socket_addr: opts.rpc,
        use_tls: opts.serve_tls,
        identity_path: opts.tls_identity_path.clone(),
        identity_pass: opts.tls_identity_password.clone(),
    };

    let network_settings = net::Settings {
        inbound: opts.accept,
        outbound_connections: opts.slots,
        external_addr: opts.external,
        peers: opts.connect.clone(),
        seeds: opts.seeds.clone(),
        ..Default::default()
    };

    let seen_tx_hashes = SeenTxHashes::new();

    // P2P registry setup
    let p2p = net::P2p::new(network_settings).await;
    let registry = p2p.protocol_registry();

    let (sender, _) = async_channel::unbounded();
    let seen_tx_hashes2 = seen_tx_hashes.clone();
    let sender2 = sender.clone();

    // Adding ProtocolTxPool to the registry
    registry
        .register(!net::SESSION_SEED, move |channel, p2p| {
            let sender = sender2.clone();
            let seen_tx_hashes = seen_tx_hashes2.clone();
            async move { ProtocolTxPool::init(channel, sender, seen_tx_hashes, p2p).await }
        })
        .await;
    // TODO: Add protocols for rest message types (block, vote)

    // Performs seed session
    p2p.clone().start(executor.clone()).await?;
    // Actual main p2p session
    let ex2 = executor.clone();
    let p2p2 = p2p.clone();
    executor
        .spawn(async move {
            if let Err(err) = p2p2.run(ex2).await {
                error!("Error: p2p run failed {}", err);
            }
        })
        .detach();

    // RPC interface
    let ex2 = executor.clone();
    let ex3 = ex2.clone();
    let rpc_interface = Arc::new(JsonRpcInterface {
        seen_tx_hashes: seen_tx_hashes.clone(),
        p2p: p2p.clone(),
        _rpc_listen_addr: opts.rpc,
    });
    executor
        .spawn(async move { listen_and_serve(rpc_server_config, rpc_interface, ex3).await })
        .detach();

    proposal_task();

    // TODO:
    // - Add protocols for tx message type - DONE
    // - Add p2p impl - DONE
    // - Add prc impl (to receive network staff) - DONE
    // - Add block proposal task impl
    // - Add tx receival like irc - DONE

    Ok(())
}

struct JsonRpcInterface {
    seen_tx_hashes: SeenTxHashesPtr,
    p2p: net::P2pPtr,
    _rpc_listen_addr: SocketAddr,
}

#[async_trait]
impl RequestHandler for JsonRpcInterface {
    async fn handle_request(&self, req: JsonRequest, _executor: Arc<Executor<'_>>) -> JsonResult {
        if req.params.as_array().is_none() {
            return jsonrpc::error(InvalidParams, None, req.id).into()
        }

        debug!(target: "RPC", "--> {}", serde_json::to_string(&req).unwrap());

        return match req.method.as_str() {
            Some("ping") => self.pong(req.id, req.params).await,
            Some("get_info") => self.get_info(req.id, req.params).await,
            Some("get_tx_pool") => self.get_tx_pool(req.id, req.params).await,
            Some("receive_tx") => self.receive_tx(req.id, req.params).await,
            Some(_) | None => jsonrpc::error(MethodNotFound, None, req.id).into(),
        }
    }
}

impl JsonRpcInterface {
    // --> {"jsonrpc": "2.0", "method": "ping", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": "pong", "id": 42}
    async fn pong(&self, id: Value, _params: Value) -> JsonResult {
        JsonResult::Resp(jsonresp(json!("pong"), id))
    }

    // --> {"jsonrpc": "2.0", "method": "get_info", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": {"nodeID": [], "nodeinfo" [], "id": 42}
    async fn get_info(&self, id: Value, _params: Value) -> JsonResult {
        let resp = self.p2p.get_info().await;
        JsonResult::Resp(jsonresp(resp, id))
    }

    // --> {"jsonrpc": "2.0", "method": "get_tx_pool", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": {"nodeID": [], "nodeinfo" [], "id": 42}
    async fn get_tx_pool(&self, id: Value, _params: Value) -> JsonResult {
        let pool = format!("{:?}", self.seen_tx_hashes);
        JsonResult::Resp(jsonresp(json!(pool), id))
    }

    // --> {"jsonrpc": "2.0", "method": "receive_tx", "params": ["tx"], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 0}
    async fn receive_tx(&self, id: Value, params: Value) -> JsonResult {
        let args = params.as_array().unwrap();

        if args.len() != 1 {
            return jsonrpc::error(InvalidParams, None, id).into()
        }

        let random_id = OsRng.next_u32();
        self.seen_tx_hashes.add_seen(random_id).await;
        let protocol_tx = Tx { hash: random_id, payload: args[0].to_string() };
        let result = self.p2p.broadcast(protocol_tx).await;

        match result {
            Ok(()) => JsonResult::Resp(jsonresp(json!(true), id)),
            Err(e) => jsonrpc::error(ServerError(-32603), Some(e.to_string()), id).into(),
        }
    }
}

#[async_std::main]
async fn main() -> Result<()> {
    let opts = Opt::from_args_with_toml(&String::from_utf8(CONFIG_FILE_CONTENTS.to_vec()).unwrap())
        .unwrap();
    let config_path = get_config_path(Some(opts.config.clone()), CONFIG_FILE)?;
    spawn_config(&config_path, CONFIG_FILE_CONTENTS)?;
    let opts = Opt::from_args_with_toml(&String::from_utf8(CONFIG_FILE_CONTENTS.to_vec()).unwrap())
        .unwrap();

    let (lvl, conf) = log_config(opts.verbose.into())?;
    TermLogger::init(lvl, conf, TerminalMode::Mixed, ColorChoice::Auto)?;

    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = async_channel::unbounded::<()>();
    let ex2 = ex.clone();
    let nthreads = if opts.threads == 0 { num_cpus::get() } else { opts.threads };

    debug!(target: "VALIDATOR DAEMON", "Executing with opts: {:?}", opts);
    debug!(target: "VALIDATOR DAEMON", "Run {} executor threads", nthreads);
    let (_, result) = Parallel::new()
        .each(0..nthreads, |_| smol::future::block_on(ex.run(shutdown.recv())))
        // Run the main future on the current thread.
        .finish(|| {
            smol::future::block_on(async move {
                start(ex2.clone(), &opts).await?;
                drop(signal);
                Ok::<(), darkfi::Error>(())
            })
        });

    result
}
