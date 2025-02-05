/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
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

use std::{collections::HashSet, io::Write, path::PathBuf, sync::Arc};

use darkfi::{
    async_daemonize, cli_desc,
    event_graph::{proto::ProtocolEventGraph, EventGraph, EventGraphPtr},
    net::{session::SESSION_DEFAULT, settings::SettingsOpt, P2p, P2pPtr},
    rpc::{
        jsonrpc::JsonSubscriber,
        server::{listen_and_serve, RequestHandler},
    },
    system::{sleep, StoppableTask, StoppableTaskPtr, Subscription},
    util::path::{expand_path, get_config_path},
    Error, Result,
};
use darkfi_sdk::crypto::pasta_prelude::PrimeField;

use log::{debug, error, info};
use rand::rngs::OsRng;
use settings::list_configured_contacts;
use sled_overlay::sled;
use smol::{fs, lock::Mutex, stream::StreamExt, Executor};
use structopt_toml::{serde::Deserialize, structopt::StructOpt, StructOptToml};
use url::Url;

const CONFIG_FILE: &str = "darkirc_config.toml";
const CONFIG_FILE_CONTENTS: &str = include_str!("../darkirc_config.toml");

/// IRC server and client handler implementation
mod irc;
use irc::server::IrcServer;

/// Cryptography utilities
mod crypto;
use crypto::{bcrypt::bcrypt_hash_password, rln::RlnIdentity};

/// JSON-RPC methods
mod rpc;

/// Settings utilities
mod settings;

fn panic_hook(panic_info: &std::panic::PanicHookInfo) {
    error!("panic occurred: {panic_info}");
    error!("{}", std::backtrace::Backtrace::force_capture().to_string());
    std::process::abort()
}

#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[serde(default)]
#[structopt(name = "darkirc", about = cli_desc!())]
struct Args {
    #[structopt(short, parse(from_occurrences))]
    /// Increase verbosity (-vvv supported)
    verbose: u8,

    #[structopt(short, long)]
    /// Configuration file to use
    config: Option<String>,

    #[structopt(long)]
    /// Set log file output
    log: Option<String>,

    #[structopt(long, default_value = "tcp://127.0.0.1:26660")]
    /// RPC server listen address
    rpc_listen: Url,

    #[structopt(long, default_value = "tcp://127.0.0.1:6667")]
    /// IRC server listen address
    irc_listen: Url,

    /// Optional TLS certificate file path if `irc_listen` uses TLS
    irc_tls_cert: Option<String>,

    /// Optional TLS certificate key file path if `irc_listen` uses TLS
    irc_tls_secret: Option<String>,

    #[structopt(short, long, default_value = "~/.local/share/darkfi/darkirc_db")]
    /// Datastore (DB) path
    datastore: String,

    #[structopt(short, long, default_value = "~/.local/share/darkfi/replayed_darkirc_db")]
    /// Replay logs (DB) path
    replay_datastore: String,

    /// Flag to store Sled DB instructions
    #[structopt(long)]
    replay_mode: bool,

    /// Generate a new NaCl keypair and exit
    #[structopt(long)]
    gen_chacha_keypair: bool,

    /// Generate a new encrypted channel NaCl secret and exit
    #[structopt(long)]
    gen_channel_secret: bool,

    /// Recover NaCl public key from a secret key
    #[structopt(long = "get-chacha-pubkey")]
    chacha_secret: Option<String>,

    /// Generate a new RLN identity
    #[structopt(long)]
    gen_rln_identity: bool,

    /// Flag to skip syncing the DAG (no history).
    #[structopt(long)]
    skip_dag_sync: bool,

    /// IRC Password (Encrypted with bcrypt-2b)
    #[structopt(long)]
    pub password: Option<String>,

    /// Encrypt a given password for the IRC server connection
    #[structopt(long)]
    encrypt_password: bool,

    /// List configured contacts.
    #[structopt(long)]
    list_contacts: bool,

    /// P2P network settings
    #[structopt(flatten)]
    net: SettingsOpt,
}

pub struct DarkIrc {
    /// P2P network pointer
    p2p: P2pPtr,
    /// Sled DB (also used in event_graph and for RLN)
    sled: sled::Db,
    /// Event Graph instance
    event_graph: EventGraphPtr,
    /// JSON-RPC connection tracker
    rpc_connections: Mutex<HashSet<StoppableTaskPtr>>,
    /// dnet JSON-RPC subscriber
    dnet_sub: JsonSubscriber,
    /// deg JSON-RPC subscriber
    deg_sub: JsonSubscriber,
    /// Replay logs (DB) path
    replay_datastore: PathBuf,
}

impl DarkIrc {
    fn new(
        p2p: P2pPtr,
        sled: sled::Db,
        event_graph: EventGraphPtr,
        dnet_sub: JsonSubscriber,
        deg_sub: JsonSubscriber,
        replay_datastore: PathBuf,
    ) -> Self {
        Self {
            p2p,
            sled,
            event_graph,
            rpc_connections: Mutex::new(HashSet::new()),
            dnet_sub,
            deg_sub,
            replay_datastore,
        }
    }
}

async_daemonize!(realmain);
async fn realmain(args: Args, ex: Arc<Executor<'static>>) -> Result<()> {
    // Abort the application on panic right away
    std::panic::set_hook(Box::new(panic_hook));

    if args.gen_chacha_keypair {
        let secret = crypto_box::SecretKey::generate(&mut OsRng);
        let public = secret.public_key();
        let secret = bs58::encode(secret.to_bytes()).into_string();
        let public = bs58::encode(public.to_bytes()).into_string();
        println!("Place this in your config file:\n");
        println!("[crypto]");
        println!("#dm_chacha_public = \"{}\"", public);
        println!("dm_chacha_secret = \"{}\"", secret);
        return Ok(())
    }

    if args.gen_channel_secret {
        let secret = crypto_box::SecretKey::generate(&mut OsRng);
        let secret = bs58::encode(secret.to_bytes()).into_string();
        println!("Place this in your config file:\n");
        println!("[channel.\"#yourchannelname\"]");
        println!("secret = \"{}\"", secret);
        return Ok(())
    }

    if args.gen_rln_identity {
        let identity = RlnIdentity::new(&mut OsRng);
        let nullifier = bs58::encode(identity.nullifier.to_repr()).into_string();
        let trapdoor = bs58::encode(identity.trapdoor.to_repr()).into_string();
        println!("Place this in your config file:\n");
        println!("[rln]");
        println!("nullifier = \"{}\"", nullifier);
        println!("trapdoor = \"{}\"", trapdoor);
        return Ok(())
    }

    if let Some(chacha_secret) = args.chacha_secret {
        let bytes = match bs58::decode(chacha_secret).into_vec() {
            Ok(v) => v,
            Err(e) => {
                println!("Error: {}", e);
                return Err(Error::ParseFailed("Secret key parsing failed"))
            }
        };

        if bytes.len() != 32 {
            return Err(Error::ParseFailed("Decoded base58 is not 32 bytes long"))
        }

        let secret: [u8; 32] = bytes.try_into().unwrap();
        let secret = crypto_box::SecretKey::from(secret);
        println!("{}", bs58::encode(secret.public_key().to_bytes()).into_string());
        return Ok(())
    }

    if args.list_contacts {
        let config_path = get_config_path(args.config, CONFIG_FILE)?;
        let contents = fs::read_to_string(&config_path).await?;
        let contents = match toml::from_str(&contents) {
            Ok(v) => v,
            Err(e) => {
                error!("Failed parsing TOML config: {}", e);
                return Err(Error::ParseFailed("Failed parsing TOML config"))
            }
        };

        // Parse configured contacts
        let contacts = list_configured_contacts(&contents)?;

        for (name, public_key) in contacts {
            println!("{}: {}", name, bs58::encode(public_key.as_bytes()).into_string())
        }
        return Ok(())
    }

    if args.encrypt_password {
        let mut pw = String::new();

        print!("Enter password: ");
        std::io::stdout().flush()?;
        std::io::stdin().read_line(&mut pw)?;

        if let Some('\n') = pw.chars().next_back() {
            pw.pop();
        }
        if let Some('\r') = pw.chars().next_back() {
            pw.pop();
        }

        println!("{}", bcrypt_hash_password(pw));
        std::io::stdout().flush()?;

        return Ok(())
    }

    info!("Initializing DarkIRC node");

    // Create datastore path if not there already.
    let datastore = expand_path(&args.datastore)?;
    fs::create_dir_all(&datastore).await?;

    let replay_datastore = expand_path(&args.replay_datastore)?;
    let replay_mode = args.replay_mode;

    info!("Instantiating event DAG");
    let sled_db = sled::open(datastore)?;
    let mut p2p_settings: darkfi::net::Settings = args.net.into();
    p2p_settings.app_version = semver::Version::parse(env!("CARGO_PKG_VERSION")).unwrap();
    let p2p = P2p::new(p2p_settings, ex.clone()).await?;
    let event_graph = EventGraph::new(
        p2p.clone(),
        sled_db.clone(),
        replay_datastore.clone(),
        replay_mode,
        "darkirc_dag",
        1,
        ex.clone(),
    )
    .await?;

    let prune_task = event_graph.prune_task.get().unwrap();

    info!("Registering EventGraph P2P protocol");
    let event_graph_ = Arc::clone(&event_graph);
    let registry = p2p.protocol_registry();
    registry
        .register(SESSION_DEFAULT, move |channel, _| {
            let event_graph_ = event_graph_.clone();
            async move { ProtocolEventGraph::init(event_graph_, channel).await.unwrap() }
        })
        .await;

    info!("Starting dnet subs task");
    let dnet_sub = JsonSubscriber::new("dnet.subscribe_events");
    let dnet_sub_ = dnet_sub.clone();
    let p2p_ = p2p.clone();
    let dnet_task = StoppableTask::new();
    dnet_task.clone().start(
        async move {
            let dnet_sub = p2p_.dnet_subscribe().await;
            loop {
                let event = dnet_sub.receive().await;
                debug!("Got dnet event: {:?}", event);
                dnet_sub_.notify(vec![event.into()].into()).await;
            }
        },
        |res| async {
            match res {
                Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                Err(e) => panic!("{}", e),
            }
        },
        Error::DetachedTaskStopped,
        ex.clone(),
    );

    info!("Starting deg subs task");
    let deg_sub = JsonSubscriber::new("deg.subscribe_events");
    let deg_sub_ = deg_sub.clone();
    let event_graph_ = event_graph.clone();
    let deg_task = StoppableTask::new();
    deg_task.clone().start(
        async move {
            let deg_sub = event_graph_.deg_subscribe().await;
            loop {
                let event = deg_sub.receive().await;
                debug!("Got deg event: {:?}", event);
                deg_sub_.notify(vec![event.into()].into()).await;
            }
        },
        |res| async {
            match res {
                Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                Err(e) => panic!("{}", e),
            }
        },
        Error::DetachedTaskStopped,
        ex.clone(),
    );

    info!("Starting JSON-RPC server");
    let darkirc = Arc::new(DarkIrc::new(
        p2p.clone(),
        sled_db.clone(),
        event_graph.clone(),
        dnet_sub,
        deg_sub,
        replay_datastore.clone(),
    ));
    let darkirc_ = Arc::clone(&darkirc);
    let rpc_task = StoppableTask::new();
    rpc_task.clone().start(
        listen_and_serve(args.rpc_listen, darkirc.clone(), None, ex.clone()),
        |res| async move {
            match res {
                Ok(()) | Err(Error::RpcServerStopped) => darkirc_.stop_connections().await,
                Err(e) => error!("Failed stopping JSON-RPC server: {}", e),
            }
        },
        Error::RpcServerStopped,
        ex.clone(),
    );

    info!("Starting IRC server");
    let password = args.password.unwrap_or_default();
    let config_path = get_config_path(args.config, CONFIG_FILE)?;
    let irc_server = IrcServer::new(
        darkirc.clone(),
        args.irc_listen,
        args.irc_tls_cert,
        args.irc_tls_secret,
        config_path,
        password,
    )
    .await?;

    let irc_task = StoppableTask::new();
    let ex_ = ex.clone();
    irc_task.clone().start(
        irc_server.clone().listen(ex_),
        |res| async move {
            match res {
                Ok(()) | Err(Error::DetachedTaskStopped) => { /* TODO: */ }
                Err(e) => error!("Failed stopping IRC server: {}", e),
            }
        },
        Error::DetachedTaskStopped,
        ex.clone(),
    );

    info!("Starting P2P network");
    p2p.clone().start().await?;

    // Initial DAG sync
    sync_task(&p2p, &event_graph, args.skip_dag_sync).await?;

    // Stoppable task to monitor network and resync on disconnect.
    let sync_mon_task = StoppableTask::new();
    sync_mon_task.clone().start(
        sync_and_monitor(p2p.clone(), event_graph.clone(), args.skip_dag_sync),
        |res| async move {
            match res {
                Ok(()) | Err(Error::DetachedTaskStopped) => { /* TODO: */ }
                Err(e) => error!("Failed sync task: {}", e),
            }
        },
        Error::DetachedTaskStopped,
        ex.clone(),
    );

    // Signal handling for graceful termination.
    let (signals_handler, signals_task) = SignalHandler::new(ex)?;
    signals_handler.wait_termination(signals_task).await?;
    info!("Caught termination signal, cleaning up and exiting...");

    info!("Stopping P2P network");
    p2p.stop().await;

    info!("Stopping JSON-RPC server");
    rpc_task.stop().await;
    dnet_task.stop().await;
    deg_task.stop().await;

    info!("Stopping IRC server");
    irc_task.stop().await;
    prune_task.stop().await;

    info!("Flushing sled database...");
    let flushed_bytes = sled_db.flush_async().await?;
    info!("Flushed {} bytes", flushed_bytes);

    info!("Shut down successfully");
    Ok(())
}

/// Async task to monitor network disconnections.
async fn monitor_network(subscription: &Subscription<Error>) -> Result<()> {
    Err(subscription.receive().await)
}

/// Async task to endlessly try to sync DAG, returns Ok if done.
async fn sync_task(p2p: &P2pPtr, event_graph: &EventGraphPtr, skip_dag_sync: bool) -> Result<()> {
    let comms_timeout = p2p.settings().read().await.outbound_connect_timeout;

    loop {
        if p2p.is_connected() {
            info!("Got peer connection");
            // We'll attempt to sync for ever
            if !skip_dag_sync {
                info!("Syncing event DAG");
                match event_graph.dag_sync().await {
                    Ok(()) => break,
                    Err(e) => {
                        // TODO: Maybe at this point we should prune or something?
                        // TODO: Or maybe just tell the user to delete the DAG from FS.
                        error!("Failed syncing DAG ({}), retrying in {}s...", e, comms_timeout);
                        sleep(comms_timeout).await;
                    }
                }
            } else {
                *event_graph.synced.write().await = true;
                break;
            }
        } else {
            info!("Waiting for some P2P connections...");
            sleep(comms_timeout).await;
        }
    }

    Ok(())
}

/// Async task to monitor the network and force resync on disconnections
async fn sync_and_monitor(
    p2p: P2pPtr,
    event_graph: EventGraphPtr,
    skip_dag_sync: bool,
) -> Result<()> {
    loop {
        let net_subscription = p2p.hosts().subscribe_disconnect().await;
        let result = monitor_network(&net_subscription).await;
        net_subscription.unsubscribe().await;

        match result {
            Ok(_) => return Ok(()),
            Err(Error::NetworkNotConnected) => {
                // Sync node again
                info!("Network disconnection detected, resyncing...");
                *event_graph.synced.write().await = false;
                sync_task(&p2p, &event_graph, skip_dag_sync).await?;
            }
            Err(e) => return Err(e),
        }
    }
}
