/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

use std::{
    collections::HashMap,
    fs::File,
    io::Write,
    sync::{atomic::Ordering, Arc},
};

use darkfi::{
    async_daemonize, cli_desc,
    event_graph::{
        proto::ProtocolEventGraph, rln::GENESIS_USER_MSG_LIMIT, EventGraph, EventGraphConfig,
        EventGraphPtr,
    },
    net::{session::SESSION_DEFAULT, settings::SettingsOpt, P2p, P2pPtr},
    rpc::{
        jsonrpc::JsonSubscriber,
        server::{listen_and_serve, RequestHandler},
        settings::{RpcSettings, RpcSettingsOpt},
        util::JsonValue,
    },
    system::{sleep, StoppableTask, Subscription},
    util::{
        memory::log_memory,
        path::{expand_path, get_config_path},
    },
    Error, Result,
};
use darkfi_sdk::crypto::pasta_prelude::PrimeField;
use darkfi_serial::serialize;

use irc2::{
    crypto::{bcrypt::bcrypt_hash_password, rln::RlnIdentity},
    genesis_commits,
    irc::server::IrcServer,
    rpc,
    settings::list_configured_contacts,
    DarkIrc,
};

use rand::rngs::OsRng;
use sled_overlay::sled;
use smol::{fs, stream::StreamExt, Executor};
use structopt_toml::{serde::Deserialize, structopt::StructOpt, StructOptToml};
use tracing::{debug, error, info};
use url::Url;

const CONFIG_FILE: &str = "darkirc_config.toml";
const CONFIG_FILE_CONTENTS: &str = include_str!("../darkirc_config.toml");

// =====================================================================
// DarkIRC consensus parameters.
//
// These define the EventGraph configuration that EVERY DarkIRC node
// in the network must agree on. Changing any of them is a hard fork.
// They are passed verbatim to `EventGraph::new` at startup.
// =====================================================================

/// Epoch origin for DAG rotation (UTC midnight, 1 March 2025).
/// Rotation boundaries are computed as offsets from this point.
const DARKIRC_INITIAL_GENESIS: u64 = 1_740_787_200_000;

/// DAG rotation period, in hours.
const DARKIRC_HOURS_ROTATION: u64 = 1;

/// Genesis payload. Two protocols MUST use distinct values; this
/// also feeds into `RlnAppId::from_genesis` so RLN signals from one
/// deployment never appear valid on another.
const DARKIRC_GENESIS_CONTENTS: &[u8] = b"darkirc-v1";

const BYTES_PER_MIB: u64 = 1024 * 1024;

/// Per-epoch limit printed by `--gen-rln-identity`.
fn generated_rln_identity_user_msg_limit() -> u64 {
    GENESIS_USER_MSG_LIMIT
}

fn sled_cache_capacity_bytes(name: &str, cache_mb: u64) -> Result<u64> {
    if cache_mb == 0 {
        return Err(Error::Custom(format!("{name} must be greater than 0")))
    }

    cache_mb
        .checked_mul(BYTES_PER_MIB)
        .ok_or_else(|| Error::Custom(format!("{name} overflows bytes")))
}

fn validate_history_window(dags_count: usize, history_retention_dags: usize) -> Result<()> {
    if dags_count == 0 {
        return Err(Error::Custom("dags_count must be greater than 0".to_string()))
    }

    if history_retention_dags == 0 {
        return Err(Error::Custom("history_retention_dags must be greater than 0".to_string()))
    }

    if dags_count > history_retention_dags {
        return Err(Error::Custom(format!(
            "dags_count ({dags_count}) cannot exceed history_retention_dags \
             ({history_retention_dags})",
        )))
    }

    Ok(())
}

fn panic_hook(panic_info: &std::panic::PanicHookInfo) {
    error!("panic occurred: {panic_info}");
    error!("{}", std::backtrace::Backtrace::force_capture());
    std::process::abort()
}

#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[serde(default)]
#[structopt(
    name = "darkirc",
    about = cli_desc!(),
    version = concat!(env!("CARGO_PKG_VERSION"), "-", env!("COMMITISH"))
)]
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

    #[structopt(long, default_value = "tcp://127.0.0.1:6667")]
    /// IRC server listen address
    irc_listen: Url,

    /// Optional TLS certificate file path if `irc_listen` uses TLS
    irc_tls_cert: Option<String>,

    /// Optional TLS certificate key file path if `irc_listen` uses TLS
    irc_tls_secret: Option<String>,

    /// How many recent DAGs to sync at startup.
    #[structopt(long, default_value = "8")]
    dags_count: usize,

    #[structopt(long, default_value = "24")]
    /// How many rotating DAGs to retain locally
    history_retention_dags: usize,

    #[structopt(long, default_value = "~/.local/share/darkfi/darkirc_db")]
    /// Datastore (DB) path
    datastore: String,

    #[structopt(long, default_value = "64")]
    /// Sled cache capacity for the datastore, in MiB
    sled_cache_mb: u64,

    #[structopt(long, default_value = "~/.local/share/darkfi/darkirc_zk_keys")]
    /// Datastore path for RLN proving and verifying keys
    zk_key_datastore: String,

    #[structopt(long, default_value = "16")]
    /// Sled cache capacity for the RLN key datastore, in MiB
    zk_key_sled_cache_mb: u64,

    #[structopt(long)]
    /// Enable RLN proof generation and verification
    rln_enabled: Option<bool>,

    #[structopt(short, long, default_value = "~/.local/share/darkfi/replayed_darkirc_db")]
    /// Replay logs (DB) path
    replay_datastore: String,

    #[structopt(long)]
    /// Flag to store Sled DB instructions
    replay_mode: bool,

    #[structopt(long)]
    /// Generate a new NaCl keypair and exit
    gen_chacha_keypair: bool,

    #[structopt(long)]
    /// Generate N genesis RLN identities
    gen_genesis_rln_identities: Option<u64>,

    #[structopt(long)]
    /// Generate a new encrypted channel NaCl secret and exit
    gen_channel_secret: bool,

    #[structopt(long = "get-chacha-pubkey")]
    /// Recover NaCl public key from a secret key
    chacha_secret: Option<String>,

    #[structopt(long)]
    /// Generate a new RLN identity
    gen_rln_identity: bool,

    #[structopt(long)]
    /// Flag to skip syncing the DAG (no history)
    skip_dag_sync: bool,

    #[structopt(long)]
    // Whether to sync headers only or full sync
    fast_mode: bool,

    #[structopt(long)]
    /// IRC Password (Encrypted with bcrypt-2b)
    password: Option<String>,

    #[structopt(long)]
    /// Encrypt a given password for the IRC server connection
    encrypt_password: bool,

    #[structopt(long)]
    /// List configured contacts.
    list_contacts: bool,

    #[structopt(flatten)]
    /// P2P network settings
    net: SettingsOpt,

    #[structopt(flatten)]
    /// JSON-RPC settings
    rpc: RpcSettingsOpt,
}

#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

#[allow(non_upper_case_globals)]
#[export_name = "malloc_conf"]
pub static malloc_conf: &[u8] = b"dirty_decay_ms:1000,muzzy_decay_ms:1000\0";

async_daemonize!(realmain);
async fn realmain(args: Args, ex: Arc<Executor<'static>>) -> Result<()> {
    if args.fast_mode {
        info!("fast mode enabled");
    }
    // Abort the application on panic right away
    std::panic::set_hook(Box::new(panic_hook));

    if args.gen_chacha_keypair {
        let secret = crypto_box::SecretKey::generate(&mut OsRng);
        let public = secret.public_key();
        let secret = bs58::encode(secret.to_bytes()).into_string();
        let public = bs58::encode(public.to_bytes()).into_string();

        println!(
            "Place this in your config file under your contact, you can reuse this keypair for multiple contacts\n"
        );
        println!("[contact.\"satoshi\"]");
        println!("dm_chacha_public = \"YOUR_CONTACT_PUBLIC_KEY\"");
        println!("my_dm_chacha_secret = \"{secret}\"");
        println!("#my_dm_chacha_public = \"{public}\"");
        return Ok(());
    }

    if args.gen_channel_secret {
        let secret = crypto_box::SecretKey::generate(&mut OsRng);
        let secret = bs58::encode(secret.to_bytes()).into_string();
        println!("Place this in your config file:\n");
        println!("[channel.\"#yourchannelname\"]");
        println!("secret = \"{secret}\"");
        return Ok(());
    }

    if args.gen_rln_identity {
        let identity = RlnIdentity::new(&mut OsRng);
        let nullifier = bs58::encode(identity.nullifier.to_repr()).into_string();
        let trapdoor = bs58::encode(identity.trapdoor.to_repr()).into_string();
        // This value is part of the RLN commitment. It must match
        // the genesis budget used for pregenerated identities.
        let user_msg_limit = generated_rln_identity_user_msg_limit();

        println!("Generated a fresh RLN identity.\n");
        println!(
            "Current DarkIRC registration accepts only identities whose commitments are in \
             the configured pregenerated set. Use this output for a genesis bundle or future \
             staked-registration testing; it will not register on the live network unless its \
             commitment is pregenerated.\n"
        );
        println!("Local account import command:\n");
        println!(
            "  /msg NickServ REGISTER <account_name> {nullifier} {trapdoor} {user_msg_limit}\n"
        );
        println!(
            "Replace <account_name> with any local label you like (\"alice\", \"throwaway\", etc)."
        );
        println!(
            "Do not change user_msg_limit: it is part of the RLN commitment and must be \
             GENESIS_USER_MSG_LIMIT ({user_msg_limit}) for pregenerated genesis identities."
        );
        println!(
            "Keep the nullifier and trapdoor secret - they ARE the identity. \
             A `darkirc --gen-rln-identity` run is NOT idempotent; treat the \
             output like a freshly-minted password."
        );
        return Ok(())
    }

    if let Some(n_identities) = args.gen_genesis_rln_identities {
        // We'll generate n_identities and hold them in a map
        // `k=commitment, v=(nullifier, trapdoor, used)`
        // We'll export the commitments to be used in the genesis event,
        // and the rest as a JSON file.
        let mut identities_map = HashMap::new();
        for _ in 0..n_identities {
            let identity = RlnIdentity::new(&mut OsRng);
            let commitment = identity.commitment();
            identities_map.insert(
                commitment.to_repr(),
                (identity.nullifier.to_repr(), identity.trapdoor.to_repr(), false),
            );
        }

        let mut commits = String::from(
            r#"
use darkfi_sdk::{crypto::pasta_prelude::PrimeField, pasta::pallas};

/// Return DarkIRC's configured pregenerated RLN commitment set.
pub fn pregenerated_identity_commitments() -> Vec<[u8; 32]> {
    DARKIRC_GENESIS_COMMITMENTS_REPR.to_vec()
}

/// Check whether an RLN commitment belongs to DarkIRC's pregenerated set.
pub fn is_pregenerated_commitment(commitment: &pallas::Base) -> bool {
    DARKIRC_GENESIS_COMMITMENTS_REPR.contains(&commitment.to_repr())
}

pub const DARKIRC_GENESIS_COMMITMENTS_REPR: &[[u8; 32]] = &[
"#,
        );

        for commitment in identities_map.keys() {
            commits.push_str(&format!("{:?},\n", commitment));
        }

        commits.push_str("];\n");

        let mut file = File::create("genesis_commits.rs")?;
        file.write_all(commits.as_bytes())?;

        let mut file = File::create("darkirc_rln_commits.bin")?;
        let buf = serialize(&identities_map);
        file.write_all(&buf)?;

        return Ok(())
    }

    if let Some(chacha_secret) = args.chacha_secret {
        let bytes = match bs58::decode(chacha_secret).into_vec() {
            Ok(v) => v,
            Err(e) => {
                println!("Error: {e}");
                return Err(Error::ParseFailed("Secret key parsing failed"));
            }
        };

        if bytes.len() != 32 {
            return Err(Error::ParseFailed("Decoded base58 is not 32 bytes long"));
        }

        let secret: [u8; 32] = bytes.try_into().unwrap();
        let secret = crypto_box::SecretKey::from(secret);
        println!("{}", bs58::encode(secret.public_key().to_bytes()).into_string());
        return Ok(());
    }

    if args.list_contacts {
        let config_path = match get_config_path(args.config, CONFIG_FILE) {
            Ok(path) => path,
            Err(e) => {
                error!("Unable to get config path: {e}");
                return Err(e);
            }
        };
        let contents = match fs::read_to_string(&config_path).await {
            Ok(c) => c,
            Err(e) => {
                error!("Unable read path `{config_path:?}`: {e}");
                return Err(e.into());
            }
        };
        let contents = match toml::from_str(&contents) {
            Ok(v) => v,
            Err(e) => {
                error!("Failed parsing TOML config: {e}");
                return Err(Error::ParseFailed("Failed parsing TOML config"));
            }
        };

        // Parse configured contacts
        let contacts = match list_configured_contacts(&contents) {
            Ok(c) => c,
            Err(e) => {
                error!("List contacts failed `{config_path:?}`: {e}");
                return Err(e);
            }
        };

        for (name, (public_key, my_secret_key)) in contacts {
            let public_key = bs58::encode(public_key.to_bytes()).into_string();
            let my_public_key = my_secret_key.public_key();
            let my_secret_key = bs58::encode(my_secret_key.to_bytes()).into_string();
            let my_public_key = bs58::encode(my_public_key.to_bytes()).into_string();
            println!("{name}: {public_key} using key {my_secret_key}({my_public_key})")
        }
        return Ok(());
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

        return Ok(());
    }

    info!("Initializing DarkIRC node");
    let rln_enabled = args.rln_enabled.unwrap_or(false);

    // Create datastore path if not there already.
    let datastore = match expand_path(&args.datastore) {
        Ok(v) => v,
        Err(e) => {
            error!("Bad datastore path `{}`: {e}", args.datastore);
            return Err(e);
        }
    };
    if let Err(e) = fs::create_dir_all(&datastore).await {
        error!("Failed to create data store path `{datastore:?}`: {e}");
        return Err(e.into());
    }

    let zk_key_datastore = if rln_enabled {
        let zk_key_datastore = match expand_path(&args.zk_key_datastore) {
            Ok(v) => v,
            Err(e) => {
                error!("Bad RLN key datastore path `{}`: {e}", args.zk_key_datastore);
                return Err(e);
            }
        };
        if let Err(e) = fs::create_dir_all(&zk_key_datastore).await {
            error!("Failed to create RLN key datastore path `{zk_key_datastore:?}`: {e}");
            return Err(e.into());
        }
        Some(zk_key_datastore)
    } else {
        info!("RLN disabled; skipping RLN key datastore setup");
        None
    };

    let replay_datastore = match expand_path(&args.replay_datastore) {
        Ok(v) => v,
        Err(e) => {
            error!("Bad replay datastore path `{}`: {e}", args.replay_datastore);
            return Err(e);
        }
    };
    let replay_mode = args.replay_mode;

    validate_history_window(args.dags_count, args.history_retention_dags)?;
    info!(
        "Retaining {} DAG(s) of local history; syncing {} DAG(s) at startup",
        args.history_retention_dags, args.dags_count,
    );

    let sled_cache_capacity = sled_cache_capacity_bytes("sled_cache_mb", args.sled_cache_mb)?;
    info!("Instantiating event DAG with {} MiB sled cache", args.sled_cache_mb);
    let sled_db = match sled::Config::new()
        .path(datastore.clone())
        .cache_capacity(sled_cache_capacity)
        .open()
    {
        Ok(v) => v,
        Err(e) => {
            error!("Failed to open datastore database `{datastore:?}`: {e}");
            return Err(e.into());
        }
    };
    log_memory("after sled open");

    let zk_key_db = if let Some(zk_key_datastore) = zk_key_datastore.as_ref() {
        let zk_key_sled_cache_capacity =
            sled_cache_capacity_bytes("zk_key_sled_cache_mb", args.zk_key_sled_cache_mb)?;
        info!("Opening RLN key datastore with {} MiB sled cache", args.zk_key_sled_cache_mb);
        Some(
            match sled::Config::new()
                .path(zk_key_datastore.clone())
                .cache_capacity(zk_key_sled_cache_capacity)
                .open()
            {
                Ok(v) => v,
                Err(e) => {
                    error!("Failed to open RLN key datastore `{zk_key_datastore:?}`: {e}");
                    return Err(e.into());
                }
            },
        )
    } else {
        None
    };

    let p2p_settings: darkfi::net::Settings =
        (env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"), args.net).try_into()?;
    let p2p = match P2p::new(p2p_settings, ex.clone()).await {
        Ok(p2p) => p2p,
        Err(e) => {
            error!("Unable to create P2P network: {e}");
            return Err(e);
        }
    };
    // Consensus config. Every node must use exactly these values.
    let eg_config = EventGraphConfig {
        initial_genesis: DARKIRC_INITIAL_GENESIS,
        hours_rotation: DARKIRC_HOURS_ROTATION,
        genesis_contents: DARKIRC_GENESIS_CONTENTS.to_vec(),
        rln_enabled,
        pregenerated_identity_commitments: if rln_enabled {
            genesis_commits::pregenerated_identity_commitments()
        } else {
            Vec::new()
        },
        max_dags: Some(args.history_retention_dags),
    };
    let event_graph = match if let Some(zk_key_db) = zk_key_db.clone() {
        EventGraph::new_with_zk_key_db(
            p2p.clone(),
            sled_db.clone(),
            zk_key_db,
            replay_datastore.clone(),
            replay_mode,
            eg_config,
            ex.clone(),
        )
        .await
    } else {
        EventGraph::new(
            p2p.clone(),
            sled_db.clone(),
            replay_datastore.clone(),
            replay_mode,
            eg_config,
            ex.clone(),
        )
        .await
    } {
        Ok(v) => v,
        Err(e) => {
            error!("Event graph failed to start: {e}");
            return Err(e);
        }
    };
    log_memory("after EventGraph construction");

    // The prune task is only spawned when `hours_rotation > 0`. We
    // require rotation here, so the unwrap is safe.
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
                debug!("Got dnet event: {event:?}");
                dnet_sub_.notify(vec![event.into()].into()).await;
            }
        },
        |res| async {
            match res {
                Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                Err(e) => panic!("{e}"),
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
                debug!("Got deg event: {event:?}");
                let json = deg_event_to_json(&event);
                deg_sub_.notify(vec![json].into()).await;
            }
        },
        |res| async {
            match res {
                Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                Err(e) => panic!("{e}"),
            }
        },
        Error::DetachedTaskStopped,
        ex.clone(),
    );

    info!("Starting Gource subs task");
    let gource_sub = JsonSubscriber::new("gource.subscribe_events");
    let gource_sub_ = gource_sub.clone();
    let event_graph_gource = event_graph.clone();
    let gource_task = StoppableTask::new();
    gource_task.clone().start(
        async move {
            let event_pub = event_graph_gource.event_pub.clone().subscribe().await;
            loop {
                let ev = event_pub.receive().await;
                if let Some(json) = rpc::privmsg_event_to_gource(&ev).await {
                    gource_sub_.notify(vec![json].into()).await;
                }
            }
        },
        |res| async {
            match res {
                Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                Err(e) => panic!("{e}"),
            }
        },
        Error::DetachedTaskStopped,
        ex.clone(),
    );

    info!("Starting JSON-RPC server");
    let rpc_settings: RpcSettings = args.rpc.into();
    let darkirc = Arc::new(DarkIrc::new(
        p2p.clone(),
        sled_db.clone(),
        event_graph.clone(),
        dnet_sub,
        deg_sub,
        gource_sub,
        replay_datastore.clone(),
    ));
    let darkirc_ = Arc::clone(&darkirc);
    let rpc_task = StoppableTask::new();
    rpc_task.clone().start(
        listen_and_serve(rpc_settings, darkirc.clone(), None, ex.clone()),
        |res| async move {
            match res {
                Ok(()) | Err(Error::RpcServerStopped) => darkirc_.stop_connections().await,
                Err(e) => error!("Failed stopping JSON-RPC server: {e}"),
            }
        },
        Error::RpcServerStopped,
        ex.clone(),
    );

    info!("Starting IRC server");
    let password = args.password.unwrap_or_default();
    let config_path = match get_config_path(args.config.clone(), CONFIG_FILE) {
        Ok(v) => v,
        Err(e) => {
            error!("Cannot get config path `{:?}`: {e}", args.config);
            return Err(e);
        }
    };
    let irc_server = match IrcServer::new(
        darkirc.clone(),
        args.irc_listen,
        args.irc_tls_cert,
        args.irc_tls_secret,
        config_path,
        password,
    )
    .await
    {
        Ok(v) => v,
        Err(e) => {
            error!("Unable to create IRC server: {e}");
            return Err(e);
        }
    };

    let irc_task = StoppableTask::new();
    let ex_ = ex.clone();
    irc_task.clone().start(
        irc_server.clone().listen(ex_),
        |res| async move {
            match res {
                Ok(()) | Err(Error::DetachedTaskStopped) => { /* TODO: */ }
                Err(e) => error!("Failed stopping IRC server: {e}"),
            }
        },
        Error::DetachedTaskStopped,
        ex.clone(),
    );

    info!("Starting P2P network");
    if let Err(e) = p2p.clone().start().await {
        error!("P2P failed to start: {e}");
        return Err(e);
    }

    // Initial DAG sync
    if let Err(e) =
        sync_task(&p2p, &event_graph, args.skip_dag_sync, args.fast_mode, args.dags_count).await
    {
        error!("DAG sync task failed to start: {e}");
        return Err(e);
    };

    // Stoppable task to monitor network and resync on disconnect.
    let sync_mon_task = StoppableTask::new();
    sync_mon_task.clone().start(
        sync_and_monitor(
            p2p.clone(),
            event_graph.clone(),
            args.skip_dag_sync,
            args.fast_mode,
            args.dags_count,
        ),
        |res| async move {
            match res {
                Ok(()) | Err(Error::DetachedTaskStopped) => { /* TODO: */ }
                Err(e) => error!("Failed sync task: {e}"),
            }
        },
        Error::DetachedTaskStopped,
        ex.clone(),
    );

    // Drain pending static broadcasts whenever the EG transitions
    // from unsynced to synced.
    //
    // NickServ REGISTER while the local DAG is unsynced will queue
    // the (event, blob) pair on `IrcServer::pending_static_broadcasts`
    // instead of broadcasting (a pre-sync broadcast goes nowhere -
    // peers gate `handle_static_put` AND `handle_tip_req` on their
    // own is_synced state). This task watches for the rising edge
    // of `is_synced()` and re-issues the queued broadcasts.
    let drain_task = StoppableTask::new();
    let irc_server_for_drain = irc_server.clone();
    let event_graph_for_drain = event_graph.clone();
    drain_task.clone().start(
        async move {
            let mut last_state = event_graph_for_drain.is_synced();
            loop {
                sleep(1).await;
                let now_state = event_graph_for_drain.is_synced();
                // Rising edge: unsynced -> synced.
                if now_state && !last_state {
                    match irc_server_for_drain.drain_pending_static_broadcasts().await {
                        Ok(0) => { /* nothing pending; common case */ }
                        Ok(n) => {
                            info!("Drained {n} pending static broadcasts after sync");
                        }
                        Err(e) => {
                            error!("Failed to drain pending broadcasts: {e}");
                        }
                    }
                }
                last_state = now_state;
            }
        },
        |res| async move {
            match res {
                Ok(()) | Err(Error::DetachedTaskStopped) => { /* normal shutdown */ }
                Err(e) => error!("Drain task failed: {e}"),
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
    gource_task.stop().await;

    info!("Stopping IRC server");
    irc_task.stop().await;
    drain_task.stop().await;
    prune_task.stop().await;

    info!("Flushing sled database...");
    let flushed_bytes = sled_db.flush_async().await?;
    info!("Flushed {flushed_bytes} bytes");

    if let Some(zk_key_db) = zk_key_db {
        info!("Flushing RLN key sled database...");
        let flushed_key_bytes = zk_key_db.flush_async().await?;
        info!("Flushed {flushed_key_bytes} RLN key bytes");
    }

    info!("Shut down successfully");
    Ok(())
}

/// Async task to monitor network disconnections.
async fn monitor_network(subscription: &Subscription<Error>) -> Result<()> {
    Err(subscription.receive().await)
}

/// Async task to endlessly try to sync DAG, returns Ok if done.
async fn sync_task(
    p2p: &P2pPtr,
    event_graph: &EventGraphPtr,
    skip_dag_sync: bool,
    fast_mode: bool,
    dags_count: usize,
) -> Result<()> {
    // skip_dag_sync means "this node opts out of syncing entirely".
    if skip_dag_sync {
        event_graph.synced.store(true, Ordering::Release);
        info!("DAG sync skipped; marking synced immediately");
        return Ok(())
    }

    let comms_timeout = p2p.settings().read_arc().await.outbound_connect_timeout_max();

    loop {
        if p2p.is_connected() {
            info!("Got peer connection");
            info!("Syncing static DAG");
            match event_graph.static_sync().await {
                Ok(()) => {
                    info!("Static synced successfully");
                    log_memory("after static sync");
                }
                Err(e) => {
                    error!("Failed syncing static graph: {e}");
                    p2p.stop().await;
                    return Err(Error::StaticDagSyncFailed)
                }
            }
            info!("Syncing event DAG");
            // Sync mode is now per-call: full sync replays
            // every event (heavy, used by archival nodes), fast
            // sync only fetches headers (light, used by clients
            // that don't need to re-verify history).
            let sync_result = if fast_mode {
                event_graph.sync_selected_headers(dags_count).await
            } else {
                event_graph.sync_selected(dags_count).await
            };
            match sync_result {
                Ok(()) => {
                    info!(
                        "Event DAG synced successfully ({} mode, {} dag(s))",
                        if fast_mode { "fast" } else { "full" },
                        dags_count,
                    );
                    break
                }
                Err(e) => {
                    // TODO: Maybe at this point we should prune or something?
                    // TODO: Or maybe just tell the user to delete the DAG from FS.
                    error!("Failed syncing DAG ({e}), retrying in {comms_timeout}s...");
                    sleep(comms_timeout).await;
                }
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
    fast_mode: bool,
    dags_count: usize,
) -> Result<()> {
    // If sync is skipped entirely there's nothing to monitor.
    if skip_dag_sync {
        return Ok(())
    }

    loop {
        let net_subscription = p2p.hosts().subscribe_disconnect().await;
        let result = monitor_network(&net_subscription).await;
        net_subscription.unsubscribe().await;

        match result {
            Ok(_) => return Ok(()),
            Err(Error::NetworkNotConnected) => {
                // Sync node again
                info!("Network disconnection detected, resyncing...");
                event_graph.synced.store(false, Ordering::Release);
                sync_task(&p2p, &event_graph, skip_dag_sync, fast_mode, dags_count).await?;
            }
            Err(e) => return Err(e),
        }
    }
}

fn deg_event_to_json(ev: &darkfi::event_graph::deg::DegEvent) -> JsonValue {
    use darkfi::{
        event_graph::deg::{DegEvent, MessageInfo},
        rpc::util::json_map,
    };

    fn info_to_json(direction: &str, info: &MessageInfo) -> JsonValue {
        let info_arr: Vec<JsonValue> = info.info.iter().cloned().map(JsonValue::String).collect();
        json_map([
            ("direction", JsonValue::String(direction.into())),
            ("cmd", JsonValue::String(info.cmd.clone())),
            // NanoTimestamp's Display is the human-readable form;
            // emit it as a string to avoid losing precision through
            // the JSON number type (f64 can't hold nanos cleanly).
            ("time", JsonValue::String(format!("{}", info.time))),
            ("info", JsonValue::Array(info_arr)),
        ])
    }

    match ev {
        DegEvent::SendMessage(info) => info_to_json("send", info),
        DegEvent::RecvMessage(info) => info_to_json("recv", info),
    }
}

#[cfg(test)]
mod tests {
    use darkfi::event_graph::rln::MAX_MSG_LIMIT;
    use rand::rngs::OsRng;

    use super::{sled_cache_capacity_bytes, validate_history_window, RlnIdentity, BYTES_PER_MIB};

    #[test]
    fn generated_rln_identity_limit_matches_genesis_budget() {
        let identity = RlnIdentity::new(&mut OsRng);

        assert_eq!(super::generated_rln_identity_user_msg_limit(), super::GENESIS_USER_MSG_LIMIT);
        assert_eq!(super::generated_rln_identity_user_msg_limit(), MAX_MSG_LIMIT);
        assert_eq!(identity.user_message_limit, super::generated_rln_identity_user_msg_limit());
    }

    #[test]
    fn sled_cache_capacity_rejects_zero() {
        assert!(sled_cache_capacity_bytes("test_cache_mb", 0).is_err());
    }

    #[test]
    fn sled_cache_capacity_converts_mib() {
        assert_eq!(sled_cache_capacity_bytes("test_cache_mb", 64).unwrap(), 64 * BYTES_PER_MIB);
    }

    #[test]
    fn history_window_rejects_zero_startup_sync() {
        assert!(validate_history_window(0, 24).is_err());
    }

    #[test]
    fn history_window_rejects_zero_retention() {
        assert!(validate_history_window(1, 0).is_err());
    }

    #[test]
    fn history_window_rejects_sync_beyond_retention() {
        assert!(validate_history_window(25, 24).is_err());
    }

    #[test]
    fn history_window_allows_sync_inside_retention() {
        assert!(validate_history_window(48, 168).is_ok());
    }
}
