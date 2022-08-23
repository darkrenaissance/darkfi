use async_std::sync::{Arc, Mutex};
use std::{
    fs::{create_dir_all, read_dir, remove_file},
    path::{Path, PathBuf},
};

use async_executor::Executor;
use crypto_box::{
    aead::{Aead, AeadCore},
    rand_core::OsRng,
    SalsaBox, SecretKey,
};
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
        serial::{deserialize, serialize, SerialDecodable, SerialEncodable},
    },
    Error, Result,
};

mod jsonrpc;
mod patch;

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
    /// Secret Key To Encrypt/Decrypt Patches
    #[structopt(long, default_value = "")]
    pub secret: String,
    /// Generate A New Secret Key
    #[structopt(long)]
    pub keygen: bool,
    /// JSON-RPC Listen URL
    #[structopt(long = "rpc", default_value = "tcp://127.0.0.1:13055")]
    pub rpc_listen: Url,
    #[structopt(flatten)]
    pub net: SettingsOpt,
    /// Increase Verbosity
    #[structopt(short, parse(from_occurrences))]
    pub verbose: u8,
}

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct EncryptedPatch {
    nonce: Vec<u8>,
    payload: Vec<u8>,
}

fn encrypt_patch(
    patch: &Patch,
    salsa_box: &SalsaBox,
    rng: &mut crypto_box::rand_core::OsRng,
) -> Result<EncryptedPatch> {
    let nonce = SalsaBox::generate_nonce(rng);
    let payload = &serialize(patch)[..];
    let payload = salsa_box
        .encrypt(&nonce, payload)
        .map_err(|_| Error::ParseFailed("Encrypting Patch failed"))?;

    let nonce = nonce.to_vec();
    Ok(EncryptedPatch { nonce, payload })
}

fn decrypt_patch(encrypt_patch: &EncryptedPatch, salsa_box: &SalsaBox) -> Result<Patch> {
    let nonce = encrypt_patch.nonce.as_slice();
    let decrypted_patch = salsa_box
        .decrypt(nonce.into(), &encrypt_patch.payload[..])
        .map_err(|_| Error::ParseFailed("Decrypting Patch failed"))?;

    let patch = deserialize(&decrypted_patch)?;

    Ok(patch)
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

fn path_to_id(path: &str) -> String {
    let mut hasher = sha2::Sha256::new();
    hasher.update(path);
    bs58::encode(hex::encode(hasher.finalize())).into_string()
}

fn get_docs_paths(files: &mut Vec<PathBuf>, path: &Path, parent: Option<&Path>) -> Result<()> {
    let docs = read_dir(&path)?;
    let docs = docs.filter(|d| d.is_ok()).map(|d| d.unwrap().path()).collect::<Vec<PathBuf>>();

    for doc in docs {
        if let Some(file_name) = doc.file_name() {
            let file_name = PathBuf::from(file_name);
            let file_name =
                if let Some(parent) = parent { parent.join(file_name) } else { file_name };
            if doc.is_file() {
                files.push(file_name);
            } else if doc.is_dir() {
                get_docs_paths(files, &doc, Some(&file_name))?;
            }
        }
    }

    Ok(())
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

    return false
}

struct Darkwiki {
    settings: DarkWikiSettings,
    rpc: (
        async_channel::Sender<Vec<Vec<(String, String)>>>,
        async_channel::Receiver<(String, bool, Vec<String>)>,
    ),
    raft: (async_channel::Sender<EncryptedPatch>, async_channel::Receiver<EncryptedPatch>),
    salsa_box: SalsaBox,
}

impl Darkwiki {
    async fn start(&self) -> Result<()> {
        let mut rng = crypto_box::rand_core::OsRng;
        loop {
            select! {
                val = self.rpc.1.recv().fuse() => {
                    let (cmd, dry, files) = val?;
                    match cmd.as_str() {
                        "update" => {
                            self.on_receive_update(dry, files, &mut rng).await?;
                        },
                        "restore" => {
                            self.on_receive_restore(dry, files).await?;
                        },
                        _ => {}
                    }
                }
                patch = self.raft.1.recv().fuse() => {
                    self.on_receive_patch(&patch?)?;
                }

            }
        }
    }

    fn on_receive_patch(&self, received_patch: &EncryptedPatch) -> Result<()> {
        let received_patch = decrypt_patch(received_patch, &self.salsa_box)?;

        info!("Receive a {:?}", received_patch);
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
            } else {
                sync_patch.extend_ops(received_patch.ops());
            }

            sync_patch.timestamp = received_patch.timestamp;
            sync_patch.author = received_patch.author;
            save_json_file::<Patch>(&sync_id_path, &sync_patch)?;
        } else if !received_patch.base.is_empty() {
            save_json_file::<Patch>(&sync_id_path, &received_patch)?;
        }

        Ok(())
    }

    async fn on_receive_update(
        &self,
        dry: bool,
        files: Vec<String>,
        rng: &mut OsRng,
    ) -> Result<()> {
        let (patches, local, sync, merge) = self.update(dry, files)?;

        if !dry {
            for patch in patches {
                info!("Send a {:?}", patch);
                let encrypt_patch = encrypt_patch(&patch, &self.salsa_box, rng)?;
                self.raft.0.send(encrypt_patch).await?;
            }
        }

        let local: Vec<(String, String)> =
            local.iter().map(|p| (p.path.to_owned(), p.colorize())).collect();

        let sync: Vec<(String, String)> =
            sync.iter().map(|p| (p.path.to_owned(), p.colorize())).collect();

        let merge: Vec<(String, String)> =
            merge.iter().map(|p| (p.path.to_owned(), p.colorize())).collect();

        self.rpc.0.send(vec![local, sync, merge]).await?;

        Ok(())
    }

    async fn on_receive_restore(&self, dry: bool, files_name: Vec<String>) -> Result<()> {
        let patches = self.restore(dry, files_name)?;
        let patches: Vec<(String, String)> =
            patches.iter().map(|p| (p.path.to_owned(), p.to_string())).collect();

        self.rpc.0.send(vec![patches]).await?;

        Ok(())
    }

    fn restore(&self, dry: bool, files_name: Vec<String>) -> Result<Vec<Patch>> {
        let local_path = self.settings.datastore_path.join("local");
        let docs_path = self.settings.docs_path.clone();

        let mut patches = vec![];

        let local_files = read_dir(&local_path)?;
        for file in local_files {
            let file_id = file?.file_name();
            let file_path = local_path.join(&file_id);
            let local_patch: Patch = load_json_file(&file_path)?;

            if !files_name.is_empty() && !files_name.contains(&local_patch.path.to_string()) {
                continue
            }

            if let Ok(doc) = load_file(&docs_path.join(&local_patch.path)) {
                if local_patch.to_string() == doc {
                    continue
                }
            }

            if !dry {
                self.save_doc(&local_patch.path, &local_patch.to_string())?;
            }

            patches.push(local_patch);
        }

        Ok(patches)
    }

    fn update(&self, dry: bool, files_name: Vec<String>) -> Result<Patches> {
        let mut patches: Vec<Patch> = vec![];
        let mut local_patches: Vec<Patch> = vec![];
        let mut sync_patches: Vec<Patch> = vec![];
        let mut merge_patches: Vec<Patch> = vec![];

        let local_path = self.settings.datastore_path.join("local");
        let sync_path = self.settings.datastore_path.join("sync");
        let docs_path = self.settings.docs_path.clone();

        // save and compare docs in darkwiki and local dirs
        // then merged with sync patches if any received
        let mut docs = vec![];
        get_docs_paths(&mut docs, &docs_path, None)?;
        for doc in docs {
            let doc_path = doc.to_str().unwrap();

            if !files_name.is_empty() && !files_name.contains(&doc_path.to_string()) {
                continue
            }

            // load doc content
            let edit = load_file(&docs_path.join(doc_path))?;

            if edit.is_empty() {
                continue
            }

            let edit = edit.trim();

            let doc_id = path_to_id(doc_path);

            // create new patch
            let mut new_patch = Patch::new(doc_path, &doc_id, &self.settings.author);

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
                    if !is_delete_patch(&sync_patch) {
                        if sync_patch.timestamp != local_patch.timestamp {
                            sync_patches.push(sync_patch.clone());

                            let sync_patch_t = new_patch.transform(&sync_patch);
                            new_patch = new_patch.merge(&sync_patch_t);
                            if !dry {
                                self.save_doc(doc_path, &new_patch.to_string())?;
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
                save_json_file(&sync_path.join(doc_id), &new_patch)?;
            }
        }

        // check if a new patch received
        // and save the new changes in both local and darkwiki dirs
        let sync_files = read_dir(&sync_path)?;
        for file in sync_files {
            let file_id = file?.file_name();
            let file_path = sync_path.join(&file_id);
            let sync_patch: Patch = load_json_file(&file_path)?;

            if is_delete_patch(&sync_patch) {
                if local_path.join(&sync_patch.id).exists() {
                    sync_patches.push(sync_patch.clone());
                }

                if !dry {
                    remove_file(docs_path.join(&sync_patch.path)).unwrap_or(());
                    remove_file(local_path.join(&sync_patch.id)).unwrap_or(());
                    remove_file(file_path).unwrap_or(());
                }

                continue
            }

            if let Ok(local_patch) = load_json_file::<Patch>(&local_path.join(&file_id)) {
                if local_patch.timestamp == sync_patch.timestamp {
                    continue
                }
            }

            if !files_name.is_empty() && !files_name.contains(&sync_patch.path.to_string()) {
                continue
            }

            if !dry {
                self.save_doc(&sync_patch.path, &sync_patch.to_string())?;
                save_json_file(&local_path.join(file_id), &sync_patch)?;
            }

            if !sync_patches.contains(&sync_patch) {
                sync_patches.push(sync_patch);
            }
        }

        // check if any doc removed from ~/darkwiki
        let local_files = read_dir(&local_path)?;
        for file in local_files {
            let file_id = file?.file_name();
            let file_path = local_path.join(&file_id);
            let local_patch: Patch = load_json_file(&file_path)?;

            if !files_name.is_empty() && !files_name.contains(&local_patch.path.to_string()) {
                continue
            }

            if !docs_path.join(&local_patch.path).exists() {
                let mut new_patch =
                    Patch::new(&local_patch.path, &local_patch.id, &self.settings.author);
                new_patch.add_op(&OpMethod::Delete(local_patch.to_string().len() as u64));
                patches.push(new_patch.clone());

                new_patch.base = local_patch.base;
                local_patches.push(new_patch);

                if !dry {
                    remove_file(file_path).unwrap_or(());
                }
            }
        }

        Ok((patches, local_patches, sync_patches, merge_patches))
    }

    fn save_doc(&self, path: &str, edit: &str) -> Result<()> {
        let path = self.settings.docs_path.join(path);
        if let Some(p) = path.parent() {
            if !p.exists() && !p.to_str().unwrap().is_empty() {
                create_dir_all(p)?;
            }
        }
        save_file(&path, edit)
    }
}

async_daemonize!(realmain);
async fn realmain(settings: Args, executor: Arc<Executor<'_>>) -> Result<()> {
    let datastore_path = expand_path(&settings.datastore)?;
    let docs_path = expand_path(&settings.docs)?;

    if settings.keygen {
        info!("Generating a new secret key");
        let mut rng = crypto_box::rand_core::OsRng;
        let secret_key = SecretKey::generate(&mut rng);
        let encoded = bs58::encode(secret_key.as_bytes());
        println!("Secret key: {}", encoded.into_string());
        return Ok(())
    }

    let bytes: [u8; 32] = bs58::decode(settings.secret)
        .into_vec()?
        .try_into()
        .map_err(|_| Error::ParseFailed("Parse secret key failed"))?;
    let secret = crypto_box::SecretKey::from(bytes);
    let public = secret.public_key();
    let salsa_box = crypto_box::SalsaBox::new(&public, &secret);

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

    let mut raft = Raft::<EncryptedPatch>::new(raft_settings, seen_net_msgs.clone())?;

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
                salsa_box,
            };
            darkwiki.start().await.unwrap_or(());
        })
        .detach();

    //
    // Waiting Exit signal
    //
    let (signal, shutdown) = async_channel::bounded::<()>(1);
    ctrlc::set_handler(move || {
        warn!(target: "darkwiki", "Catch exit signal");
        // cleaning up tasks running in the background
        if let Err(e) = async_std::task::block_on(signal.send(())) {
            error!("Error on sending exit signal: {}", e);
        }
    })
    .unwrap();

    raft.run(p2p.clone(), p2p_recv_channel.clone(), executor.clone(), shutdown.clone()).await?;

    Ok(())
}
