use std::{net::SocketAddr, path::PathBuf, sync::Arc, thread};

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
    consensus::{
        state::{State, StatePtr},
        tx::Tx,
    },
    net,
    rpc::{
        jsonrpc,
        jsonrpc::{
            from_result,
            ErrorCode::{InternalError, InvalidParams, InvalidRequest, MethodNotFound},
            JsonRequest, JsonResult, ValueResult,
        },
        rpcserver::{listen_and_serve, RequestHandler, RpcServerConfig},
    },
    util::{
        cli::{log_config, spawn_config},
        expand_path,
        path::get_config_path,
    },
    Result,
};

use validatord::protocols::{
    protocol_proposal::ProtocolProposal, protocol_tx::ProtocolTx, protocol_vote::ProtocolVote,
};

const CONFIG_FILE: &str = r"validatord_config.toml";
const CONFIG_FILE_CONTENTS: &[u8] = include_bytes!("../validatord_config.toml");

#[derive(Debug, Deserialize, Serialize, StructOpt, StructOptToml)]
#[serde(default)]
struct Opt {
    #[structopt(short, long, default_value = CONFIG_FILE)]
    /// Configuration file to use
    config: String,
    #[structopt(long, default_value = "0.0.0.0:11000")]
    /// Accept address
    accept: SocketAddr,
    #[structopt(long)]
    /// Seed nodes
    seeds: Vec<SocketAddr>,
    #[structopt(long)]
    /// Manual connections
    connect: Vec<SocketAddr>,
    #[structopt(long, default_value = "5")]
    /// Connection slots
    slots: u32,
    #[structopt(long, default_value = "127.0.0.1:11000")]
    /// External address
    external: SocketAddr,
    #[structopt(long, default_value = "/tmp/darkfid.log")]
    /// Logfile path
    log: String,
    #[structopt(long, default_value = "127.0.0.1:6660")]
    /// The endpoint where validatord will bind its RPC socket
    rpc: SocketAddr,
    #[structopt(long)]
    /// Whether to listen with TLS or plain TCP
    tls: bool,
    #[structopt(long, default_value = "~/.config/darkfi/validatord_identity.pfx")]
    /// TLS certificate to use
    identity: PathBuf,
    #[structopt(long, default_value = "FOOBAR")]
    /// Password for the created TLS identity
    password: String,
    #[structopt(long, default_value = "~/.config/darkfi/validatord_state_0")]
    /// Path to the state file
    state: String,
    #[structopt(long, default_value = "0")]
    /// Node ID, used only for testing
    id: u64,
    #[structopt(long, default_value = "1")]
    /// Nodes count, used only for testing
    nodes: u64,
    #[structopt(short, long, default_value = "0")]
    /// How many threads to utilize
    threads: usize,
    #[structopt(short, long, parse(from_occurrences))]
    /// Multiple levels can be used (-vv)
    verbose: u8,
}

// TODO: 1. Nodes count retrieval.
async fn proposal_task(p2p: net::P2pPtr, state: StatePtr, state_path: &PathBuf, nodes_count: u64) {
    // After initialization node should wait for next epoch
    let seconds_until_next_epoch = state.read().unwrap().get_seconds_until_next_epoch_start();
    info!("Waiting for next epoch({:?} sec)...", seconds_until_next_epoch);
    thread::sleep(seconds_until_next_epoch);

    loop {
        let result = if state.read().unwrap().check_if_epoch_leader(nodes_count) {
            state.read().unwrap().propose_block()
        } else {
            Ok(None)
        };
        match result {
            Ok(proposal) => {
                if proposal.is_none() {
                    info!("Node is not the epoch leader. Sleeping till next epoch...");
                } else {
                    let unwrapped = proposal.unwrap();
                    info!("Node is the epoch leader. Proposed block: {:?}", unwrapped);
                    let vote = state.write().unwrap().receive_proposed_block(
                        &unwrapped,
                        nodes_count,
                        true,
                    );
                    match vote {
                        Ok(x) => {
                            if x.is_none() {
                                debug!("Node did not vote for the proposed block.");
                            } else {
                                let vote = x.unwrap();
                                state.write().unwrap().receive_vote(&vote, nodes_count as usize);
                                // Broadcasting block
                                let result = p2p.broadcast(unwrapped).await;
                                match result {
                                    Ok(()) => info!("Proposal broadcasted successfuly."),
                                    Err(e) => error!("Broadcast failed. Error: {:?}", e),
                                }
                                // Broadcasting leader vote
                                let result = p2p.broadcast(vote).await;
                                match result {
                                    Ok(()) => info!("Leader vote broadcasted successfuly."),
                                    Err(e) => error!("Broadcast failed. Error: {:?}", e),
                                }
                            }
                        }
                        Err(e) => {
                            error!("Error prosessing proposal: {:?}", e)
                        }
                    }
                }
            }
            Err(e) => error!("Broadcast failed. Error: {:?}", e),
        }

        let seconds_until_next_epoch = state.read().unwrap().get_seconds_until_next_epoch_start();
        info!("Waiting for next epoch({:?} sec)...", seconds_until_next_epoch);
        thread::sleep(seconds_until_next_epoch);
        let result = state.read().unwrap().save(state_path);
        match result {
            Ok(()) => (),
            Err(e) => {
                error!("State could not be flushed: {:?}", e)
            }
        };
    }
}

async fn start(executor: Arc<Executor<'_>>, opts: &Opt) -> Result<()> {
    let rpc_server_config = RpcServerConfig {
        socket_addr: opts.rpc,
        use_tls: opts.tls,
        identity_path: opts.identity.clone(),
        identity_pass: opts.password.clone(),
    };

    let network_settings = net::Settings {
        inbound: Some(opts.accept),
        outbound_connections: opts.slots,
        external_addr: Some(opts.external),
        peers: opts.connect.clone(),
        seeds: opts.seeds.clone(),
        ..Default::default()
    };

    // State setup
    let state_path = expand_path(&opts.state).unwrap();
    let id = opts.id.clone();
    let nodes_count = opts.nodes.clone();
    let state = State::load_current_state(id, &state_path).unwrap();

    // P2P registry setup
    let p2p = net::P2p::new(network_settings).await;
    let registry = p2p.protocol_registry();

    // Adding ProtocolTx to the registry
    let state2 = state.clone();
    registry
        .register(net::SESSION_ALL, move |channel, p2p| {
            let state = state2.clone();
            async move { ProtocolTx::init(channel, state, p2p).await }
        })
        .await;

    // Adding PropotolVote to the registry
    let state2 = state.clone();
    let nodes_count2 = nodes_count.clone() as usize;
    registry
        .register(net::SESSION_ALL, move |channel, p2p| {
            let state = state2.clone();
            let nodes_count = nodes_count2.clone();
            async move { ProtocolVote::init(channel, state, p2p, nodes_count).await }
        })
        .await;

    // Adding ProtocolProposal to the registry
    let state2 = state.clone();
    let nodes_count2 = nodes_count.clone();
    registry
        .register(net::SESSION_ALL, move |channel, p2p| {
            let state = state2.clone();
            let nodes_count = nodes_count2.clone();
            async move { ProtocolProposal::init(channel, state, p2p, nodes_count).await }
        })
        .await;

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
        state: state.clone(),
        p2p: p2p.clone(),
        _rpc_listen_addr: opts.rpc,
    });
    executor
        .spawn(async move { listen_and_serve(rpc_server_config, rpc_interface, ex3).await })
        .detach();

    proposal_task(p2p, state, &state_path, nodes_count).await;

    Ok(())
}

struct JsonRpcInterface {
    state: StatePtr,
    p2p: net::P2pPtr,
    _rpc_listen_addr: SocketAddr,
}

#[async_trait]
impl RequestHandler for JsonRpcInterface {
    async fn handle_request(&self, req: JsonRequest, _executor: Arc<Executor<'_>>) -> JsonResult {
        if req.params.as_array().is_none() {
            return jsonrpc::error(InvalidRequest, None, req.id).into()
        }

        debug!(target: "RPC", "--> {}", serde_json::to_string(&req).unwrap());

        from_result(
            match req.method.as_str() {
                Some("ping") => self.pong().await,
                Some("get_info") => self.get_info().await,
                Some("receive_tx") => self.receive_tx(req.params).await,
                Some(_) | None => Err(MethodNotFound),
            },
            req.id,
        )
    }
}

impl JsonRpcInterface {
    // --> {"jsonrpc": "2.0", "method": "ping", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": "pong", "id": 42}
    async fn pong(&self) -> ValueResult<Value> {
        Ok(json!("pong"))
    }

    // --> {"jsonrpc": "2.0", "method": "get_info", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": {"nodeID": [], "nodeinfo" [], "id": 42}
    async fn get_info(&self) -> ValueResult<Value> {
        Ok(self.p2p.get_info().await)
    }

    // --> {"jsonrpc": "2.0", "method": "receive_tx", "params": ["tx"], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 0}
    async fn receive_tx(&self, params: Value) -> ValueResult<Value> {
        let args = params.as_array().unwrap();

        if args.len() != 1 {
            return Err(InvalidParams)
        }

        // TODO: add proper tx hash here
        let random_id = OsRng.next_u32();
        let payload = String::from(args[0].as_str().unwrap());
        let tx = Tx { hash: random_id, payload };

        self.state.write().unwrap().append_tx(tx.clone());

        let result = self.p2p.broadcast(tx).await;
        match result {
            Ok(()) => Ok(json!(true)),
            Err(_) => Err(InternalError),
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
