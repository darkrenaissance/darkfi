use std::{net::SocketAddr, path::PathBuf, sync::Arc, thread};

use async_executor::Executor;
use async_trait::async_trait;
use easy_parallel::Parallel;
use log::{debug, error, info};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use simplelog::{ColorChoice, TermLogger, TerminalMode};
use structopt::StructOpt;
use structopt_toml::StructOptToml;

use darkfi::{
    consensus::{
        block::{BlockOrder, BlockResponse},
        blockchain::{ForkOrder, ForkResponse},
        participant::Participant,
        state::{ValidatorState, ValidatorStatePtr},
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
    protocol_participant::ProtocolParticipant, protocol_proposal::ProtocolProposal,
    protocol_sync::ProtocolSync, protocol_sync_forks::ProtocolSyncForks, protocol_tx::ProtocolTx,
    protocol_vote::ProtocolVote,
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
    #[structopt(long, default_value = "0.0.0.0:12000")]
    /// Consensus accept address
    caccept: SocketAddr,
    #[structopt(long)]
    /// Seed nodes
    seeds: Vec<SocketAddr>,
    #[structopt(long)]
    /// Consensus seed nodes
    cseeds: Vec<SocketAddr>,
    #[structopt(long)]
    /// Manual connections
    connect: Vec<SocketAddr>,
    #[structopt(long, default_value = "5")]
    /// Connection slots
    slots: u32,
    #[structopt(long, default_value = "127.0.0.1:11000")]
    /// External address
    external: SocketAddr,
    #[structopt(long, default_value = "127.0.0.1:12000")]
    /// Consensus accept address
    cexternal: SocketAddr,
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
    #[structopt(long, default_value = "1648383795")]
    /// Timestamp of the genesis block creation
    genesis: i64,
    #[structopt(long, default_value = "~/.config/darkfi/validatord_db_0")]
    /// Path to the sled database folder
    database: String,
    #[structopt(long, default_value = "0")]
    /// Node ID, used only for testing
    id: u64,
    #[structopt(short, long, default_value = "0")]
    /// How many threads to utilize
    threads: usize,
    #[structopt(short, long, parse(from_occurrences))]
    /// Multiple levels can be used (-vv)
    verbose: u8,
}

async fn syncing_task(p2p: net::P2pPtr, state: ValidatorStatePtr) -> Result<()> {
    info!("Node starts syncing blockchain...");
    // We retrieve p2p network connected channels, so we can use it to parallelize downloads
    // Using len here because is_empty() uses unstable library feature 'exact_size_is_empty'
    if p2p.channels().lock().await.values().len() != 0 {
        // Currently we will use just the last channel
        let channel = p2p.channels().lock().await.values().last().unwrap().clone();

        // Communication setup
        let message_subsytem = channel.get_message_subsystem();
        message_subsytem.add_dispatch::<BlockResponse>().await;
        let response_sub = channel
            .subscribe_msg::<BlockResponse>()
            .await
            .expect("Missing BlockResponse dispatcher!");

        // Nodes sends the last known block hash of the canonical blockchain
        // and loops until the respond is the same block (used to utilize batch requests)
        let mut last = state.read().unwrap().blockchain.last()?.unwrap();
        info!("Last known block: {:?} - {:?}", last.0, last.1);
        loop {
            // Node creates a BlockOrder and sends it
            let order = BlockOrder { sl: last.0, block: last.1 };
            channel.send(order).await?;

            // Node stores responce data. Extra validations can be added here.
            let response = response_sub.receive().await?;
            for info in &response.blocks {
                state.write().unwrap().blockchain.add_by_info(info.clone())?;
            }
            let last_received = state.read().unwrap().blockchain.last()?.unwrap();
            info!("Last received block: {:?} - {:?}", last_received.0, last_received.1);
            if last == last_received {
                break
            }
            last = last_received;
        }
    } else {
        info!("Node is not connected to other nodes.");
    }

    info!("Node synced!");
    Ok(())
}

async fn syncing_forks_task(p2p: net::P2pPtr, state: ValidatorStatePtr) -> Result<()> {
    info!("Node starts syncing forks...");
    // Using len here because is_empty() uses unstable library feature 'exact_size_is_empty'
    if p2p.channels().lock().await.values().len() != 0 {
        // Nodes ask for the fork chains of the last channel peer
        let channel = p2p.channels().lock().await.values().last().unwrap().clone();

        // Communication setup
        let message_subsytem = channel.get_message_subsystem();
        message_subsytem.add_dispatch::<ForkResponse>().await;
        let response_sub = channel
            .subscribe_msg::<ForkResponse>()
            .await
            .expect("Missing ForkResponse dispatcher!");

        // Node creates a BlockOrder and sends it
        let order = ForkOrder { id: state.read().unwrap().id };
        channel.send(order).await?;

        // Node stores responce data. Extra validations can be added here.
        let response = response_sub.receive().await?;
        state.write().unwrap().consensus.proposals = response.proposals.clone();
    } else {
        info!("Node is not connected to other nodes, resetting consensus state.");
        state.write().unwrap().reset_consensus_state()?;
    }

    info!("Node synced!");
    Ok(())
}

async fn proposal_task(p2p: net::P2pPtr, state: ValidatorStatePtr) {
    // Node syncs its fork chains
    let result = syncing_forks_task(p2p.clone(), state.clone()).await;
    match result {
        Ok(()) => (),
        Err(e) => error!("Sync forks failed. Error: {:?}", e),
    }

    // Node signals the network that it starts participating
    let participant =
        Participant::new(state.read().unwrap().id, state.read().unwrap().current_epoch());
    state.write().unwrap().append_participant(participant.clone());
    let result = p2p.broadcast(participant).await;
    match result {
        Ok(()) => info!("Participation message broadcasted successfuly."),
        Err(e) => error!("Broadcast failed. Error: {:?}", e),
    }

    // After initialization node should wait for next epoch
    let seconds_until_next_epoch = state.read().unwrap().next_epoch_start();
    info!("Waiting for next epoch({:?} sec)...", seconds_until_next_epoch);
    thread::sleep(seconds_until_next_epoch);

    loop {
        // Node refreshes participants records
        state.write().unwrap().refresh_participants();

        // Node checks if its the epoch leader to generate a new proposal for that epoch
        let result = if state.write().unwrap().is_epoch_leader() {
            state.read().unwrap().propose()
        } else {
            Ok(None)
        };
        match result {
            Ok(proposal) => {
                if proposal.is_none() {
                    info!("Node is not the epoch leader. Sleeping till next epoch...");
                } else {
                    // Leader creates a vote for the proposal and broadcasts them both
                    let unwrapped = proposal.unwrap();
                    info!("Node is the epoch leader. Proposed block: {:?}", unwrapped);
                    let vote = state.write().unwrap().receive_proposal(&unwrapped);
                    match vote {
                        Ok(x) => {
                            if x.is_none() {
                                error!("Node did not vote for the proposed block.");
                            } else {
                                let vote = x.unwrap();
                                let result = state.write().unwrap().receive_vote(&vote);
                                match result {
                                    Ok(_) => info!("Vote saved successfuly."),
                                    Err(e) => error!("Vote save failed. Error: {:?}", e),
                                }
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
            Err(e) => error!("Block proposal failed. Error: {:?}", e),
        }

        // Current node state is flushed to sled database
        let result = state.read().unwrap().save_consensus_state();
        match result {
            Ok(()) => (),
            Err(e) => {
                error!("State could not be flushed: {:?}", e)
            }
        };

        // Node waits untile next epoch
        let seconds_until_next_epoch = state.read().unwrap().next_epoch_start();
        info!("Waiting for next epoch({:?} sec)...", seconds_until_next_epoch);
        thread::sleep(seconds_until_next_epoch);
    }
}

async fn start(executor: Arc<Executor<'_>>, opts: &Opt) -> Result<()> {
    let rpc_server_config = RpcServerConfig {
        socket_addr: opts.rpc,
        use_tls: opts.tls,
        identity_path: opts.identity.clone(),
        identity_pass: opts.password.clone(),
    };

    // Main subnet settings
    let subnet_settings = net::Settings {
        inbound: Some(opts.accept),
        outbound_connections: opts.slots,
        external_addr: Some(opts.external),
        peers: opts.connect.clone(),
        seeds: opts.seeds.clone(),
        ..Default::default()
    };

    // Consensus subnet settings
    let consensus_subnet_settings = net::Settings {
        inbound: Some(opts.caccept),
        outbound_connections: opts.slots,
        external_addr: Some(opts.cexternal),
        peers: opts.connect.clone(),
        seeds: opts.cseeds.clone(),
        ..Default::default()
    };

    // State setup
    let genesis = opts.genesis;
    let database_path = expand_path(&opts.database).unwrap();
    let id = opts.id.clone();
    let state = ValidatorState::new(database_path, id, genesis).unwrap();

    // Main P2P registry setup
    let main_p2p = net::P2p::new(subnet_settings).await;
    let registry = main_p2p.protocol_registry();

    // Adding ProtocolSync to the registry
    let state2 = state.clone();
    let consensus_mode = false; // This flag should be based on staking
    registry
        .register(net::SESSION_ALL, move |channel, main_p2p| {
            let state = state2.clone();
            async move { ProtocolSync::init(channel, state, main_p2p, consensus_mode).await }
        })
        .await;

    // Adding ProtocolTx to the registry
    let state2 = state.clone();
    registry
        .register(net::SESSION_ALL, move |channel, main_p2p| {
            let state = state2.clone();
            async move { ProtocolTx::init(channel, state, main_p2p).await }
        })
        .await;

    // Performs seed session
    main_p2p.clone().start(executor.clone()).await?;
    // Actual main p2p session
    let ex2 = executor.clone();
    let p2p = main_p2p.clone();
    executor
        .spawn(async move {
            if let Err(err) = p2p.run(ex2).await {
                error!("Error: p2p run failed {}", err);
            }
        })
        .detach();

    // RPC interface
    let ex2 = executor.clone();
    let ex3 = ex2.clone();
    let rpc_interface = Arc::new(JsonRpcInterface {
        state: state.clone(),
        p2p: main_p2p.clone(),
        _rpc_listen_addr: opts.rpc,
    });
    executor
        .spawn(async move { listen_and_serve(rpc_server_config, rpc_interface, ex3).await })
        .detach();

    // Node starts syncing
    let state2 = state.clone();
    syncing_task(main_p2p.clone(), state2).await?;

    // Consensus P2P registry setup
    let consensus_p2p = net::P2p::new(consensus_subnet_settings).await;
    let registry = consensus_p2p.protocol_registry();

    // Adding PropotolVote to the registry
    let p2p = main_p2p.clone();
    let state2 = state.clone();
    registry
        .register(net::SESSION_ALL, move |channel, consensus_p2p| {
            let state = state2.clone();
            let main_p2p = p2p.clone();
            async move { ProtocolVote::init(channel, state, main_p2p, consensus_p2p).await }
        })
        .await;

    // Adding ProtocolProposal to the registry
    let state2 = state.clone();
    registry
        .register(net::SESSION_ALL, move |channel, consensus_p2p| {
            let state = state2.clone();
            async move { ProtocolProposal::init(channel, state, consensus_p2p).await }
        })
        .await;

    // Adding ProtocolParticipant to the registry
    let state2 = state.clone();
    registry
        .register(net::SESSION_ALL, move |channel, consensus_p2p| {
            let state = state2.clone();
            async move { ProtocolParticipant::init(channel, state, consensus_p2p).await }
        })
        .await;

    // Adding ProtocolSyncForks to the registry
    let state2 = state.clone();
    registry
        .register(net::SESSION_ALL, move |channel, _consensus_p2p| {
            let state = state2.clone();
            async move { ProtocolSyncForks::init(channel, state).await }
        })
        .await;

    // Performs seed session
    consensus_p2p.clone().start(executor.clone()).await?;
    // Actual consensus p2p session
    let ex2 = executor.clone();
    let p2p = consensus_p2p.clone();
    executor
        .spawn(async move {
            if let Err(err) = p2p.run(ex2).await {
                error!("Error: p2p run failed {}", err);
            }
        })
        .detach();

    proposal_task(consensus_p2p, state).await;

    Ok(())
}

struct JsonRpcInterface {
    state: ValidatorStatePtr,
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

        let payload = String::from(args[0].as_str().unwrap());
        let tx = Tx { payload };

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
