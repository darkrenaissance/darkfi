use async_std::sync::{Arc, Mutex};
use std::{
    fs::{create_dir_all, read_dir},
    path::PathBuf,
};

use async_executor::Executor;
use futures::{select, FutureExt};
use fxhash::FxHashMap;
use log::{error, info, warn};
use serde::Deserialize;
use sha2::Digest;
use smol::future;
use structopt::StructOpt;
use structopt_toml::StructOptToml;
use unicode_segmentation::UnicodeSegmentation;
use url::Url;

use darkfi::{
    async_daemonize,
    net::{self, settings::SettingsOpt},
    raft::{NetMsg, ProtocolRaft, Raft, RaftSettings},
    rpc::server::listen_and_serve,
    util::{
        cli::{get_log_config, get_log_level, spawn_config},
        expand_path,
        file::{load_file, load_json_file, save_file, save_json_file},
        path::get_config_path,
    },
    Error, Result,
};

mod error;
mod jsonrpc;
mod patch;

use error::DarkWikiResult;
use jsonrpc::JsonRpcInterface;
use patch::{OpMethod, Patch};

type Patches = (Vec<Patch>, Vec<Patch>, Vec<Patch>, Vec<Patch>);

pub const CONFIG_FILE: &str = "darkwiki.toml";
pub const CONFIG_FILE_CONTENTS: &str = include_str!("../darkwiki.toml");

/// darkwikid cli
#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[serde(default)]
#[structopt(name = "darkwikid")]
pub struct Args {
    /// Sets a custom config file
    #[structopt(long)]
    pub config: Option<String>,
    /// Sets Datastore Path
    #[structopt(long, default_value = "~/.config/darkfi/darkwiki")]
    pub datastore: String,
    /// Sets Docs Path
    #[structopt(long, default_value = "~/darkwiki")]
    pub docs: String,
    /// Sets Author Name for Patch
    #[structopt(long, default_value = "NONE")]
    pub author: String,
    /// JSON-RPC listen URL
    #[structopt(long = "rpc", default_value = "tcp://127.0.0.1:13055")]
    pub rpc_listen: Url,
    #[structopt(flatten)]
    pub net: SettingsOpt,
    /// Increase verbosity
    #[structopt(short, parse(from_occurrences))]
    pub verbose: u8,
}

pub struct DarkWikiSettings {
    author: String,
    docs_path: PathBuf,
    datastore_path: PathBuf,
}

fn str_to_chars(s: &str) -> Vec<&str> {
    s.graphemes(true).collect::<Vec<&str>>()
}

fn lcs(a: &str, b: &str) -> Vec<OpMethod> {
    let a: Vec<_> = str_to_chars(a);
    let b: Vec<_> = str_to_chars(b);
    let (na, nb) = (a.len(), b.len());

    let mut lengths = vec![vec![0; nb + 1]; na + 1];

    for (i, ci) in a.iter().enumerate() {
        for (j, cj) in b.iter().enumerate() {
            lengths[i + 1][j + 1] =
                if ci == cj { lengths[i][j] + 1 } else { lengths[i][j + 1].max(lengths[i + 1][j]) }
        }
    }

    let mut result = Vec::new();
    let (mut i, mut j) = (na, nb);
    while i > 0 && j > 0 {
        if a[i - 1] == b[j - 1] {
            result.push(OpMethod::Retain((1) as _));
            i -= 1;
            j -= 1;
        } else if lengths[i - 1][j] > lengths[i][j - 1] {
            result.push(OpMethod::Delete((1) as _));
            i -= 1;
        } else {
            result.push(OpMethod::Insert(b[j - 1].to_string()));
            j -= 1;
        }
    }

    result.reverse();
    result
}

fn title_to_id(title: &str) -> String {
    let mut hasher = sha2::Sha256::new();
    hasher.update(title);
    hex::encode(hasher.finalize())
}

struct Darkwiki {
    settings: DarkWikiSettings,
    rpc: (
        async_channel::Sender<Vec<Vec<(String, String)>>>,
        async_channel::Receiver<(String, bool, Vec<String>)>,
    ),
    raft: (async_channel::Sender<Patch>, async_channel::Receiver<Patch>),
}

impl Darkwiki {
    async fn start(&self) -> DarkWikiResult<()> {
        loop {
            select! {
                val = self.rpc.1.recv().fuse() => {
                    let (cmd, dry, files) = val.map_err(Error::from)?;
                    match cmd.as_str() {
                        "update" => {
                            self.on_receive_update(dry, files).await?;
                        },
                        "restore" => {
                            self.on_receive_restore(dry, files).await?;
                        },
                        _ => {}
                    }
                }
                patch = self.raft.1.recv().fuse() => {
                    let patch = patch.map_err(Error::from)?;
                    info!("Receive new patch from Raft {:?}", patch);
                    self.on_receive_patch(&patch)?;
                }

            }
        }
    }

    fn on_receive_patch(&self, received_patch: &Patch) -> DarkWikiResult<()> {
        let sync_id_path = self.settings.datastore_path.join("sync").join(&received_patch.id);
        let local_id_path = self.settings.datastore_path.join("local").join(&received_patch.id);

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

    async fn on_receive_update(&self, dry: bool, files: Vec<String>) -> DarkWikiResult<()> {
        let (patches, local, sync, merge) = self.update(dry, files)?;

        if !dry {
            for patch in patches {
                info!("Send a patch to Raft {:?}", patch);
                self.raft.0.send(patch.clone()).await.map_err(Error::from)?;
            }
        }

        let local: Vec<(String, String)> =
            local.iter().map(|p| (p.title.to_owned(), p.colorize())).collect();

        let sync: Vec<(String, String)> =
            sync.iter().map(|p| (p.title.to_owned(), p.colorize())).collect();

        let merge: Vec<(String, String)> =
            merge.iter().map(|p| (p.title.to_owned(), p.colorize())).collect();

        self.rpc.0.send(vec![local, sync, merge]).await.map_err(Error::from)?;

        Ok(())
    }

    async fn on_receive_restore(&self, dry: bool, files_name: Vec<String>) -> DarkWikiResult<()> {
        let patches = self.restore(dry, files_name)?;
        let patches: Vec<(String, String)> =
            patches.iter().map(|p| (p.title.to_owned(), p.to_string())).collect();

        self.rpc.0.send(vec![patches]).await.map_err(Error::from)?;

        Ok(())
    }

    fn restore(&self, dry: bool, files_name: Vec<String>) -> DarkWikiResult<Vec<Patch>> {
        let local_path = self.settings.datastore_path.join("local");
        let docs_path = self.settings.docs_path.clone();
        let local_files = read_dir(&local_path).map_err(Error::from)?;

        let mut patches = vec![];

        for file in local_files {
            let file_id = file.as_ref().unwrap().file_name();
            let file_id = file_id.to_str().unwrap();
            let file_path = local_path.join(&file_id);
            let local_patch: Patch = load_json_file(&file_path)?;

            if !files_name.is_empty() && !files_name.contains(&local_patch.title.to_string()) {
                continue
            }

            if let Ok(doc) = load_file(&docs_path.join(&local_patch.title)) {
                if local_patch.to_string() == doc {
                    continue
                }
            }

            if !dry {
                save_file(&docs_path.join(&local_patch.title), &local_patch.to_string())?;
            }

            patches.push(local_patch);
        }

        Ok(patches)
    }

    fn update(&self, dry: bool, files_name: Vec<String>) -> DarkWikiResult<Patches> {
        let mut patches: Vec<Patch> = vec![];
        let mut local_patches: Vec<Patch> = vec![];
        let mut sync_patches: Vec<Patch> = vec![];
        let mut merge_patches: Vec<Patch> = vec![];

        let local_path = self.settings.datastore_path.join("local");
        let sync_path = self.settings.datastore_path.join("sync");
        let docs_path = self.settings.docs_path.clone();

        // save and compare docs in darkwiki and local dirs
        // then merged with sync patches if any received
        let docs = read_dir(&docs_path).map_err(Error::from)?;
        for doc in docs {
            let doc_title = doc.as_ref().unwrap().file_name();
            let doc_title = doc_title.to_str().unwrap();

            if !files_name.is_empty() && !files_name.contains(&doc_title.to_string()) {
                continue
            }

            // load doc content
            let edit = load_file(&docs_path.join(doc_title)).map_err(Error::from)?;
            let edit = edit.trim();

            let doc_id = title_to_id(doc_title);

            // create new patch
            let mut new_patch = Patch::new(doc_title, &doc_id, &self.settings.author);

            // check for any changes found with local doc and darkwiki doc
            if let Ok(local_patch) = load_json_file::<Patch>(&local_path.join(&doc_id)) {
                // no changes found
                if local_patch.to_string() == edit {
                    continue
                }

                // check the differences with LCS algorithm
                let lcs_ops = lcs(&local_patch.to_string(), edit);

                // add the change ops to the new patch
                for op in lcs_ops {
                    new_patch.add_op(&op);
                }

                new_patch.base = local_patch.to_string();

                local_patches.push(new_patch.clone());

                let mut b_patch = new_patch.clone();
                b_patch.base = "".to_string();
                patches.push(b_patch);

                // check if the same doc has received patch from the network
                if let Ok(sync_patch) = load_json_file::<Patch>(&sync_path.join(&doc_id)) {
                    if sync_patch.timestamp != local_patch.timestamp {
                        sync_patches.push(sync_patch.clone());

                        let sync_patch_t = new_patch.transform(&sync_patch);
                        new_patch = new_patch.merge(&sync_patch_t);
                        if !dry {
                            save_file(&docs_path.join(doc_title), &new_patch.to_string())?;
                        }
                        merge_patches.push(new_patch.clone());
                    }
                }
            } else {
                new_patch.base = edit.to_string();
                local_patches.push(new_patch.clone());
                patches.push(new_patch.clone());
            };

            if !dry {
                save_json_file(&local_path.join(&doc_id), &new_patch)?;
                save_json_file(&sync_path.join(doc_id), &new_patch)?;
            }
        }

        // check if a new patch received
        // and save the new changes in both local and darkwiki dirs
        let sync_files = read_dir(&sync_path).map_err(Error::from)?;
        for file in sync_files {
            let file_id = file.as_ref().unwrap().file_name();
            let file_id = file_id.to_str().unwrap();
            let file_path = sync_path.join(&file_id);
            let sync_patch: Patch = load_json_file(&file_path)?;

            if let Ok(local_patch) = load_json_file::<Patch>(&local_path.join(&file_id)) {
                if local_patch.timestamp == sync_patch.timestamp {
                    continue
                }
            }

            if !files_name.is_empty() && !files_name.contains(&sync_patch.title.to_string()) {
                continue
            }

            if !dry {
                save_file(&docs_path.join(&sync_patch.title), &sync_patch.to_string())?;
                save_json_file(&local_path.join(file_id), &sync_patch)?;
            }

            if !sync_patches.contains(&sync_patch) {
                sync_patches.push(sync_patch);
            }
        }

        Ok((patches, local_patches, sync_patches, merge_patches))
    }
}

async_daemonize!(realmain);
async fn realmain(settings: Args, executor: Arc<Executor<'_>>) -> Result<()> {
    let datastore_path = expand_path(&settings.datastore)?;
    let docs_path = expand_path(&settings.docs)?;

    create_dir_all(docs_path.clone())?;
    create_dir_all(datastore_path.join("local"))?;
    create_dir_all(datastore_path.join("sync"))?;

    let (rpc_sx, rpc_rv) = async_channel::unbounded::<(String, bool, Vec<String>)>();
    let (notify_sx, notify_rv) = async_channel::unbounded::<Vec<Vec<(String, String)>>>();

    //
    // RPC
    //
    let rpc_interface = Arc::new(JsonRpcInterface::new(rpc_sx, notify_rv));
    executor.spawn(listen_and_serve(settings.rpc_listen.clone(), rpc_interface)).detach();

    //
    // Raft
    //
    let net_settings = settings.net;
    let seen_net_msgs = Arc::new(Mutex::new(FxHashMap::default()));

    let datastore_raft = datastore_path.join("darkwiki.db");
    let raft_settings = RaftSettings { datastore_path: datastore_raft, ..RaftSettings::default() };

    let mut raft = Raft::<Patch>::new(raft_settings, seen_net_msgs.clone())?;

    //
    // P2p setup
    //
    let (p2p_send_channel, p2p_recv_channel) = async_channel::unbounded::<NetMsg>();

    let p2p = net::P2p::new(net_settings.into()).await;
    let p2p = p2p.clone();

    let registry = p2p.protocol_registry();

    let raft_node_id = raft.id();
    registry
        .register(net::SESSION_ALL, move |channel, p2p| {
            let raft_node_id = raft_node_id.clone();
            let sender = p2p_send_channel.clone();
            let seen_net_msgs_cloned = seen_net_msgs.clone();
            async move {
                ProtocolRaft::init(raft_node_id, channel, sender, p2p, seen_net_msgs_cloned).await
            }
        })
        .await;

    p2p.clone().start(executor.clone()).await?;

    executor.spawn(p2p.clone().run(executor.clone())).detach();

    //
    // Darkwiki start
    //

    let raft_sx = raft.sender();
    let raft_rv = raft.receiver();
    executor
        .spawn(async move {
            let darkwiki_settings =
                DarkWikiSettings { author: settings.author, datastore_path, docs_path };
            let darkwiki = Darkwiki {
                settings: darkwiki_settings,
                raft: (raft_sx, raft_rv),
                rpc: (notify_sx, rpc_rv),
            };
            darkwiki.start().await.unwrap_or(());
        })
        .detach();

    //
    // Waiting Exit signal
    //
    let (signal, shutdown) = async_channel::bounded::<()>(1);
    ctrlc_async::set_async_handler(async move {
        warn!(target: "darkwiki", "Catch exit signal");
        // cleaning up tasks running in the background
        if let Err(e) = signal.send(()).await {
            error!("Error on sending exit signal: {}", e);
        }
    })
    .unwrap();

    raft.run(p2p.clone(), p2p_recv_channel.clone(), executor.clone(), shutdown.clone()).await?;

    Ok(())
}
