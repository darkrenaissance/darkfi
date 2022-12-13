/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2022 Dyne.org foundation
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
    fs::{create_dir_all, read_dir, remove_file},
    io::stdin,
    path::{Path, PathBuf},
    process::exit,
};

use async_std::{
    stream::StreamExt,
    sync::{Arc, Mutex, RwLock},
    task,
};
use dryoc::classic::crypto_secretbox::{crypto_secretbox_keygen, Key};
use futures::{select, FutureExt};
use lazy_static::lazy_static;
use log::{debug, error, info, warn};
use signal_hook::consts::{SIGHUP, SIGINT, SIGQUIT, SIGTERM};
use signal_hook_async_std::Signals;
use structopt_toml::{serde::Deserialize, structopt::StructOpt, StructOptToml};
use url::Url;

use darkfi::{
    async_daemonize, cli_desc, net,
    raft::{NetMsg, ProtocolRaft, Raft, RaftSettings},
    rpc::server::listen_and_serve,
    util::{
        file::{load_file, load_json_file, save_file, save_json_file},
        path::{expand_path, get_config_path},
    },
    Result,
};

mod jsonrpc;
use jsonrpc::JsonRpcInterface;
mod lcs;
use lcs::Lcs;
mod patch;
use patch::{EncryptedPatch, OpMethod, Patch};
mod util;
use util::{decrypt_patch, encrypt_patch, get_docs_paths, parse_workspaces, path_to_id};

type Patches = (Vec<Patch>, Vec<Patch>, Vec<Patch>, Vec<Patch>);

lazy_static! {
    /// This is where we hold our workspaces, so we are also able to refresh them on SIGHUP.
    static ref WORKSPACES: RwLock<HashMap<String, Key>> = RwLock::new(HashMap::new());
}

pub const CONFIG_FILE: &str = "darkwikid_config.toml";
pub const CONFIG_FILE_CONTENTS: &str = include_str!("../darkwikid_config.toml");

const SYNC_ID_PATH: &str = "sync";
const LOCAL_ID_PATH: &str = "local";

#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[serde(default)]
#[structopt(name = "darkwikid", about = cli_desc!())]
struct Args {
    /// Increase verbosity (-vvv supported)
    #[structopt(short, parse(from_occurrences))]
    verbose: u8,

    /// Configuration file to use
    #[structopt(short, long)]
    config: Option<String>,

    /// Workspace configuration (repeatable flag)
    #[structopt(short, long)]
    workspace: Vec<String>,

    /// Path where to store wiki's files
    #[structopt(short, long, default_value = "~/darkwiki")]
    docs: String,

    /// Sets author's name for patches
    #[structopt(long, default_value = "Anonymous")]
    author: String,

    /// Generate a new secret for a workspace
    #[structopt(long)]
    gen_secret: bool,

    /// JSON-RPC listen URL
    #[structopt(long, default_value = "tcp://localhost:24330")]
    rpc_listen: Url,

    /// Network settings
    #[structopt(flatten)]
    net: net::settings::SettingsOpt,
}

/// Settings struct used to hold some metadata for DarkWiki
struct DarkWikiSettings {
    author: String,
    docs_path: PathBuf,
    store_path: PathBuf,
}

/// DarkWiki object
struct DarkWiki {
    settings: DarkWikiSettings,
    #[allow(clippy::type_complexity)]
    rpc: (
        smol::channel::Sender<Vec<Vec<Patch>>>,
        smol::channel::Receiver<(String, bool, Vec<String>)>,
    ),
    raft: (smol::channel::Sender<EncryptedPatch>, smol::channel::Receiver<EncryptedPatch>),
}

impl DarkWiki {
    async fn start(&self) -> Result<()> {
        loop {
            select! {
                val = self.rpc.1.recv().fuse() => {
                    let (cmd, dry, files) = match val {
                        Ok(v) => v,
                        Err(e) => {
                            error!("Failed unwrapping val received from RPC: {}", e);
                            continue
                        }
                    };

                    match cmd.as_str() {
                        "update" => {
                            if let Err(e) = self.on_receive_update(dry, files).await {
                                error!("on_receive_update returned error: {}", e);
                                continue
                            }
                        }

                        "restore" => {
                            if let Err(e) = self.on_receive_restore(dry, files).await {
                                error!("on_receive_restore returned error: {}", e);
                                continue
                            }
                        }

                        x => {
                            warn!("Received unsupported command: {}", x);
                            continue
                        }
                    }
                }

                patch = self.raft.1.recv().fuse() => {
                    let patch = match patch {
                        Ok(v) => v,
                        Err(e) => {
                            error!("Failed unwrapping patch received from raft: {}", e);
                            continue
                        }
                    };

                    for (workspace, key) in WORKSPACES.read().await.iter() {
                        if let Ok(mut patch) = decrypt_patch(&patch, key) {
                            info!("[{}] Receive a {:?}", workspace, patch);
                            patch.workspace = workspace.clone();
                            if let Err(e) = self.on_receive_patch(&patch) {
                                error!("on_receive_patch returned error: {}", e);
                            }
                        }
                    }
                }
            }
        }
    }

    fn on_receive_patch(&self, received_patch: &Patch) -> Result<()> {
        let sync_id_path = self.settings.store_path.join(SYNC_ID_PATH).join(&received_patch.id);
        let local_id_path = self.settings.store_path.join(LOCAL_ID_PATH).join(&received_patch.id);

        if let Ok(mut sync_patch) = load_json_file::<Patch>(&sync_id_path) {
            if sync_patch.timestamp == received_patch.timestamp {
                return Ok(())
            }

            if let Ok(local_patch) = load_json_file::<Patch>(&local_id_path) {
                if local_patch.timestamp == sync_patch.timestamp {
                    sync_patch.base = local_patch.to_string();
                    sync_patch.set_ops(received_patch.ops());
                } else {
                    sync_patch.extend_ops(received_patch.ops());
                }
            }

            sync_patch.timestamp = received_patch.timestamp;
            sync_patch.author = received_patch.author.clone();
            save_json_file::<Patch>(&sync_id_path, &sync_patch)?;
        } else if !received_patch.base.is_empty() {
            save_json_file::<Patch>(&sync_id_path, received_patch)?;
        }

        Ok(())
    }

    async fn on_receive_update(&self, dry: bool, files: Vec<String>) -> Result<()> {
        let (mut local, mut sync, mut merge) = (vec![], vec![], vec![]);

        for (workspace, key) in WORKSPACES.read().await.iter() {
            let (patches, l, s, m) = self.update(
                dry,
                &self.settings.docs_path.join(workspace),
                files.clone(),
                workspace,
            )?;

            local.extend(l);
            sync.extend(s);
            merge.extend(m);

            if !dry {
                for patch in patches {
                    info!("Send a {:?}", patch);
                    let encrypt_patch = encrypt_patch(&patch, key)?;
                    self.raft.0.send(encrypt_patch).await?;
                }
            }
        }

        self.rpc.0.send(vec![local, sync, merge]).await?;
        Ok(())
    }

    async fn on_receive_restore(&self, dry: bool, filenames: Vec<String>) -> Result<()> {
        let mut patches = vec![];

        for (workspace, _) in WORKSPACES.read().await.iter() {
            patches.extend(self.restore(
                dry,
                &self.settings.docs_path.join(workspace),
                &filenames,
                workspace,
            )?);
        }

        self.rpc.0.send(vec![patches]).await?;
        Ok(())
    }

    fn restore(
        &self,
        dry: bool,
        docs_path: &Path,
        filenames: &[String],
        workspace: &str,
    ) -> Result<Vec<Patch>> {
        let local_path = self.settings.store_path.join(LOCAL_ID_PATH);
        let mut patches = vec![];

        let local_files = read_dir(&local_path)?;
        for file in local_files {
            let file_id = file?.file_name();
            let file_path = local_path.join(&file_id);
            let local_patch: Patch = load_json_file(&file_path)?;

            if local_patch.workspace != workspace {
                continue
            }

            // TODO: FIXME: Simplify this logic, what is this? Add comments.
            if !filenames.is_empty() && !filenames.contains(&local_patch.path.to_string()) {
                continue
            }

            if let Ok(doc) = load_file(&docs_path.join(&local_patch.path)) {
                if local_patch.to_string() == doc {
                    continue
                }
            }

            if !dry {
                self.save_doc(&local_patch.path, &local_patch.to_string(), workspace)?;
            }

            patches.push(local_patch);
        }

        Ok(patches)
    }

    // TODO: Add debug/info statements and refactor this function, there's too many things going on here.
    fn update(
        &self,
        dry: bool,
        docs_path: &Path,
        filenames: Vec<String>,
        workspace: &str,
    ) -> Result<Patches> {
        let (mut patches, mut local_patches, mut sync_patches, mut merge_patches) =
            (vec![], vec![], vec![], vec![]);

        let local_path = self.settings.store_path.join(LOCAL_ID_PATH);
        let sync_path = self.settings.store_path.join(SYNC_ID_PATH);

        // Save and compare docs in darkwiki and local dirs, then
        // merge with sync patches if any have been received.
        let mut docs = vec![];
        get_docs_paths(&mut docs, docs_path, None)?;
        for doc in docs {
            let doc_path = doc.to_str().unwrap();

            // FIXME: IDGI
            if !filenames.is_empty() && !filenames.contains(&doc_path.to_string()) {
                continue
            }

            // Load doc content
            let edit = load_file(&docs_path.join(doc_path))?;
            if edit.is_empty() {
                continue
            }

            let doc_id = path_to_id(doc_path, workspace);

            // Create new patch
            let mut new_patch = Patch::new(doc_path, &doc_id, &self.settings.author, workspace);

            // Check for any changes found with local doc and darkwiki doc
            if let Ok(local_patch) = load_json_file::<Patch>(&local_path.join(&doc_id)) {
                // No changes found
                if local_patch.to_string() == edit {
                    continue
                }

                // Check the differences with LCS algorithm
                let local_patch_str = local_patch.to_string();
                let lcs = Lcs::new(&local_patch_str, &edit);
                let lcs_ops = lcs.ops();

                // Add the change ops to the new patch
                for op in lcs_ops {
                    new_patch.add_op(&op);
                }

                new_patch.base = local_patch.to_string();
                local_patches.push(new_patch.clone());

                let mut b_patch = new_patch.clone();
                b_patch.base = "".to_string();
                patches.push(b_patch);

                // Check if the same doc has received a patch from the network
                if let Ok(sync_patch) = load_json_file::<Patch>(&sync_path.join(&doc_id)) {
                    if !Self::is_delete_patch(&sync_patch) {
                        if sync_patch.timestamp != local_patch.timestamp {
                            sync_patches.push(sync_patch.clone());

                            let sync_patch_t = new_patch.transform(&sync_patch);
                            new_patch = new_patch.merge(&sync_patch_t);
                            if !dry {
                                self.save_doc(doc_path, &new_patch.to_string(), workspace)?;
                            }
                            merge_patches.push(new_patch.clone());
                        }
                    } else {
                        merge_patches.push(sync_patch);
                        patches = vec![];
                    }
                }
            } else {
                new_patch.base = edit.to_string();
                local_patches.push(new_patch.clone());
                patches.push(new_patch.clone());
            };

            if !dry {
                save_json_file(&local_path.join(&doc_id), &new_patch)?;
                save_json_file(&sync_path.join(&doc_id), &new_patch)?;
            }
        }

        // Check if a new patch is received and save the new changes
        // in both local and darkwiki dirs.
        let sync_files = read_dir(&sync_path)?;
        for file in sync_files {
            let file_id = file?.file_name();
            let file_path = sync_path.join(&file_id);
            let sync_patch: Patch = load_json_file(&file_path)?;

            if sync_patch.workspace != workspace {
                continue
            }

            if Self::is_delete_patch(&sync_patch) {
                if local_path.join(&sync_patch.id).exists() {
                    sync_patches.push(sync_patch.clone());
                }

                if !dry {
                    remove_file(docs_path.join(&sync_patch.path))?;
                    remove_file(local_path.join(&sync_patch.id))?;
                    remove_file(file_path)?;
                }

                continue
            }

            if let Ok(local_patch) = load_json_file::<Patch>(&local_path.join(&file_id)) {
                if local_patch.timestamp == sync_patch.timestamp {
                    continue
                }
            }

            // TODO: FIXME: IDGI AGAIN, HALP
            if !filenames.is_empty() && !filenames.contains(&sync_patch.path.to_string()) {
                continue
            }

            if !dry {
                self.save_doc(&sync_patch.path, &sync_patch.to_string(), workspace)?;
                save_json_file(&local_path.join(file_id), &sync_patch)?;
            }

            if !sync_patches.contains(&sync_patch) {
                sync_patches.push(sync_patch);
            }
        }

        // Check if any doc is removed from darkwiki filesystem.
        let local_files = read_dir(&local_path)?;
        for file in local_files {
            let file_id = file?.file_name();
            let file_path = local_path.join(&file_id);
            let local_patch: Patch = load_json_file(&file_path)?;

            if local_patch.workspace != workspace {
                continue
            }

            // TODO: FIXME: Is it just supposed to check that filenames doesn't contain the local_patch?
            if !filenames.is_empty() && !filenames.contains(&local_patch.path.to_string()) {
                continue
            }

            if !docs_path.join(&local_patch.path).exists() {
                let mut new_patch = Patch::new(
                    &local_patch.path,
                    &local_patch.id,
                    &self.settings.author,
                    &local_patch.workspace,
                );
                new_patch.add_op(&OpMethod::Delete(local_patch.to_string().len() as u64));
                patches.push(new_patch.clone());

                new_patch.base = local_patch.base;
                local_patches.push(new_patch);

                if !dry {
                    remove_file(file_path)?;
                }
            }
        }

        Ok((patches, local_patches, sync_patches, merge_patches))
    }

    fn save_doc(&self, path: &str, edit: &str, workspace: &str) -> Result<()> {
        let path = self.settings.docs_path.join(workspace).join(path);
        if let Some(p) = path.parent() {
            if !p.exists() && !p.to_str().unwrap().is_empty() {
                create_dir_all(p)?;
            }
        }
        save_file(&path, edit)
    }

    fn is_delete_patch(patch: &Patch) -> bool {
        if patch.ops().0.len() != 1 {
            return false
        }

        if let OpMethod::Delete(d) = patch.ops().0[0] {
            if patch.base.len() as u64 == d {
                return true
            }
        }

        false
    }
}

async fn handle_signals(
    mut signals: Signals,
    cfg_path: PathBuf,
    term_tx: smol::channel::Sender<()>,
) {
    debug!("Started signal handler");
    while let Some(signal) = signals.next().await {
        match signal {
            SIGHUP => {
                info!("Caught SIGHUP");
                let toml_contents = match std::fs::read_to_string(cfg_path.clone()) {
                    Ok(v) => v,
                    Err(e) => {
                        error!("Couldn't load configuration file: {}", e);
                        continue
                    }
                };

                *WORKSPACES.write().await = parse_workspaces(&toml_contents);
                info!("Reloaded workspaces");
            }

            SIGTERM | SIGINT | SIGQUIT => {
                term_tx.send(()).await.unwrap();
            }

            _ => unreachable!(),
        }
    }
}

async_daemonize!(realmain);
async fn realmain(args: Args, executor: Arc<smol::Executor<'_>>) -> Result<()> {
    let cfg_path = get_config_path(args.config, CONFIG_FILE)?;
    let docs_path = expand_path(&args.docs)?;
    let store_path = expand_path(docs_path.join(".log").to_str().unwrap())?;

    create_dir_all(docs_path.clone())?;
    create_dir_all(store_path.clone())?;
    create_dir_all(store_path.join(LOCAL_ID_PATH))?;
    create_dir_all(store_path.join(SYNC_ID_PATH))?;

    if args.gen_secret {
        eprintln!("Generating a new workspace");
        loop {
            eprint!("Input the name for the new workspace (use ascii chars): ");
            let mut workspace = String::new();
            stdin().read_line(&mut workspace)?;
            // Non-exhaustive
            let workspace =
                workspace.replace(['\t', '\r', ' ', '/', '\\', '\'', '&', '~', ':'], "_");

            if workspace.is_empty() || workspace.len() < 3 {
                eprintln!("Error: Workspace name is empty or less than 3 characters. Try again.");
                continue
            }

            let secret = bs58::encode(crypto_secretbox_keygen()).into_string();
            create_dir_all(docs_path.join(workspace.clone()))?;

            println!("Created workspace: {}:{}", workspace, secret);
            eprintln!("Please add it to the config file.");
            return Ok(())
        }
    }

    // Signal handling for config reload and graceful termination.
    let signals = Signals::new([SIGHUP, SIGTERM, SIGINT, SIGQUIT])?;
    let handle = signals.handle();
    let (term_tx, term_rx) = smol::channel::bounded::<()>(1);
    let signals_task = task::spawn(handle_signals(signals, cfg_path.clone(), term_tx));
    info!("Set up signal handling");

    {
        info!("Parsing configuration file for workspaces");
        let toml_contents = std::fs::read_to_string(cfg_path.clone())?;
        *WORKSPACES.write().await = parse_workspaces(&toml_contents);
        if WORKSPACES.read().await.is_empty() {
            eprintln!("Please add atleast one workspace to the config file.");
            eprintln!("Run \"$ darkwikid --gen-secret\" to create a new workspace.");
            exit(1);
        }
    }

    let (rpc_tx, rpc_rx) = smol::channel::unbounded::<(String, bool, Vec<String>)>();
    let (notify_tx, notify_rx) = smol::channel::unbounded::<Vec<Vec<Patch>>>();

    // ===============
    // JSON-RPC server
    // ===============
    let rpc_iface = Arc::new(JsonRpcInterface::new(rpc_tx, notify_rx));
    let _ex = executor.clone();
    executor.spawn(listen_and_serve(args.rpc_listen, rpc_iface, _ex)).detach();

    // ====
    // Raft
    // ====
    let seen_net_msgs = Arc::new(Mutex::new(HashMap::new()));
    let store_raft = store_path.join("darkwiki.db");
    let raft_settings = RaftSettings { datastore_path: store_raft, ..RaftSettings::default() };
    // FIXME: This is a bad design, and needs a proper rework.
    let raft =
        Arc::new(Mutex::new(Raft::<EncryptedPatch>::new(raft_settings, seen_net_msgs.clone())?));

    // =========
    // P2P setup
    // =========
    let mut net_settings = args.net.clone();
    net_settings.app_version = Some(option_env!("CARGO_PKG_VERSION").unwrap_or("").to_string());
    let (p2p_tx, p2p_rx) = smol::channel::unbounded::<NetMsg>();
    let p2p = net::P2p::new(net_settings.into()).await;
    let registry = p2p.protocol_registry();

    let raft_node_id = raft.lock().await.id();
    registry.register(net::SESSION_ALL, move | channel, p2p| {
        let raft_node_id = raft_node_id.clone();
        let sender = p2p_tx.clone();
        let seen_net_msgs = seen_net_msgs.clone();
        async move {
            ProtocolRaft::init(raft_node_id, channel, sender, p2p, seen_net_msgs).await
        }
    }).await;

    p2p.clone().start(executor.clone()).await?;
    executor.spawn(p2p.clone().run(executor.clone())).detach();

    // ==============
    // Darkwiki start
    // ==============
    let raft_tx = raft.lock().await.sender();
    let raft_rx = raft.lock().await.receiver();
    executor
        .spawn(async move {
            let settings = DarkWikiSettings { author: args.author, store_path, docs_path };
            let dw = DarkWiki { settings, raft: (raft_tx, raft_rx), rpc: (notify_tx, rpc_rx) };
            dw.start().await.unwrap();
        })
        .detach();

    let (raft_term_tx, raft_term_rx) = smol::channel::bounded::<()>(1);
    let _p2p = p2p.clone();
    let _ex = executor.clone();
    executor
        .spawn(async move { raft.lock().await.run(_p2p, p2p_rx, _ex, raft_term_rx).await.unwrap() })
        .detach();

    // Wait for termination signal
    term_rx.recv().await?;
    eprint!("\r");
    info!("Caught termination signal, cleaning up and exiting...");
    handle.close();
    signals_task.await;

    info!("Stopping Raft...");
    raft_term_tx.send(()).await.unwrap();

    info!("Stopping P2P network...");
    p2p.stop().await;

    info!("Bye.");
    Ok(())
}
