use async_std::sync::{Arc, Mutex};
use std::{
    fs::{create_dir_all, read_dir},
    mem::discriminant,
    path::{Path, PathBuf},
};

use async_executor::Executor;
use futures::{select, FutureExt};
use fxhash::FxHashMap;
use log::{error, info, warn};
use serde::Deserialize;
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
        gen_id,
        path::get_config_path,
    },
    Error, Result,
};

mod error;
mod jsonrpc;
mod patch;

use error::{DarkWikiError, DarkWikiResult};
use jsonrpc::JsonRpcInterface;
use patch::{OpMethod, Patch};

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

fn get_from_index(local_path: &Path, title: &str) -> DarkWikiResult<String> {
    let index = load_json_file::<FxHashMap<String, String>>(&local_path.join("index"))?;

    for (i, t) in index {
        if t == title {
            return Ok(i)
        }
    }

    Err(DarkWikiError::FileNotFound)
}

fn save_to_index(local_path: &Path, id: &str, title: &str) -> DarkWikiResult<()> {
    let mut index = load_json_file::<FxHashMap<String, String>>(&local_path.join("index"))
        .unwrap_or(FxHashMap::default());

    index.insert(id.to_owned(), title.to_owned());
    save_json_file(&local_path.join("index"), &index)?;
    Ok(())
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

fn on_receive_patch(received_patch: &Patch, settings: &DarkWikiSettings) -> DarkWikiResult<()> {
    let sync_id_path = settings.datastore_path.join("sync").join(&received_patch.id());
    let local_id_path = settings.datastore_path.join("local").join(&received_patch.id());

    if let Ok(mut sync_patch) = load_json_file::<Patch>(&sync_id_path) {
        if let Ok(local_edit) = load_file(&local_id_path) {
            let local_edit = local_edit.trim();

            if local_edit == sync_patch.to_string() {
                sync_patch.set_ops(received_patch.ops());
            } else {
                sync_patch.extend_ops(received_patch.ops());
            }
        }

        save_json_file::<Patch>(&sync_id_path, &sync_patch)?;
    } else if !received_patch.base_empty() {
        save_json_file::<Patch>(&sync_id_path, received_patch)?;
    }

    Ok(())
}

fn on_receive_update(settings: &DarkWikiSettings) -> DarkWikiResult<Vec<Patch>> {
    let mut patches: Vec<Patch> = vec![];

    let local_path = settings.datastore_path.join("local");
    let sync_path = settings.datastore_path.join("sync");
    let docs_path = settings.docs_path.clone();

    // save and compare docs in darkwiki and local dirs
    // then merged with sync patches if any received
    let docs = read_dir(&docs_path).map_err(Error::from)?;
    for doc in docs {
        let doc_title = doc.as_ref().unwrap().file_name();
        let doc_title = doc_title.to_str().unwrap();

        let doc_id = match get_from_index(&local_path, doc_title) {
            Ok(id) => id,
            Err(_) => {
                let id = gen_id(30);
                save_to_index(&local_path, &id, doc_title)?;
                id
            }
        };

        // load doc content
        let edit = load_file(&docs_path.join(doc_title)).map_err(Error::from)?;
        let edit = edit.trim();

        // create new patch
        let mut new_patch = Patch::new(doc_title, &doc_id, &settings.author);

        // check for any changes found with local doc and darkwiki doc
        if let Ok(local_edit) = load_file(&local_path.join(&doc_id)) {
            let local_edit = local_edit.trim();

            // check the differences with LCS algorithm
            let lcs_ops = lcs(local_edit, edit);

            let retains_len = lcs_ops
                .iter()
                .filter(|&o| discriminant(&OpMethod::Retain(0)) == discriminant(o))
                .count();

            // if all the ops in lcs_ops are Reatin then no changes found
            if retains_len == lcs_ops.len() {
                continue
            }

            // add the change ops to the new patch
            for op in lcs_ops {
                new_patch.add_op(&op);
            }

            // check if the same doc has received patch from the network
            if let Ok(sync_patch) = load_json_file::<Patch>(&sync_path.join(&doc_id)) {
                if sync_patch.to_string() != local_edit {
                    let sync_patch_t = new_patch.transform(&sync_patch);
                    new_patch = new_patch.merge(&sync_patch_t);
                    save_file(&docs_path.join(doc_title), &new_patch.to_string())?;
                }
            }
        } else {
            new_patch.set_base(edit);
        };

        save_file(&local_path.join(&doc_id), &new_patch.to_string())?;
        save_json_file(&sync_path.join(doc_id), &new_patch)?;
        patches.push(new_patch);
    }

    // check if a new patch received
    // and save the new changes in both local and darkwiki dirs
    let sync_files = read_dir(&sync_path).map_err(Error::from)?;
    for file in sync_files {
        let file_id = file.as_ref().unwrap().file_name();
        let file_id = file_id.to_str().unwrap();
        let file_path = sync_path.join(&file_id);
        let sync_patch: Patch = load_json_file(&file_path)?;

        if let Ok(local_edit) = load_file(&local_path.join(&file_id)) {
            if local_edit.trim() == sync_patch.to_string() {
                continue
            }
        }

        let sync_patch_str = sync_patch.to_string();
        let file_title = sync_patch.title();
        save_to_index(&local_path, file_id, &file_title)?;

        save_file(&docs_path.join(&file_title), &sync_patch_str)?;
        save_file(&local_path.join(file_id), &sync_patch_str)?;
    }

    Ok(patches)
}

async fn start(
    update_notifier_rv: async_channel::Receiver<()>,
    raft_sender: async_channel::Sender<Patch>,
    raft_receiver: async_channel::Receiver<Patch>,
    settings: DarkWikiSettings,
) -> DarkWikiResult<()> {
    loop {
        select! {
            _ = update_notifier_rv.recv().fuse() => {
                let patches = on_receive_update(&settings)?;
                for patch in patches {
                    info!("Send a patch to Raft {:?}", patch);
                    raft_sender.send(patch).await.map_err(Error::from)?;
                }
            }
            patch = raft_receiver.recv().fuse() => {
                let patch = patch.map_err(Error::from)?;
                info!("Receive new patch from Raft {:?}", patch);
                on_receive_patch(&patch, &settings)?;
            }

        }
    }
}

async_daemonize!(realmain);
async fn realmain(settings: Args, executor: Arc<Executor<'_>>) -> Result<()> {
    let datastore_path = expand_path(&settings.datastore)?;
    let docs_path = expand_path(&settings.docs)?;

    create_dir_all(docs_path.clone())?;
    create_dir_all(datastore_path.join("local"))?;
    create_dir_all(datastore_path.join("sync"))?;

    let (update_notifier_sx, update_notifier_rv) = async_channel::unbounded::<()>();

    //
    // RPC
    //
    let rpc_interface = Arc::new(JsonRpcInterface::new(update_notifier_sx));
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
    let darkwiki_settings = DarkWikiSettings { author: settings.author, datastore_path, docs_path };
    executor
        .spawn(start(update_notifier_rv, raft.sender(), raft.receiver(), darkwiki_settings))
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
