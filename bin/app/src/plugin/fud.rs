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

use darkfi::{
    net::{
        session::{SESSION_DIRECT, SESSION_INBOUND},
        settings::{MagicBytes, NetworkProfile, Settings as NetSettings},
        P2p, P2pPtr,
    },
    system::{sleep, Publisher, PublisherPtr},
};
use darkfi_serial::{Decodable, Encodable};
use fud::{
    event::FudEvent, proto::ProtocolFud, settings::Args as FudSettings, util::hash_to_string, Fud,
};
use sled_overlay::sled;
use smol::lock::Mutex;
use std::{
    collections::HashSet,
    io::Cursor,
    path::PathBuf,
    sync::{Arc, OnceLock, Weak},
};
use url::Url;

use crate::{
    error::{Error, Result},
    prop::{BatchGuardPtr, PropertyAtomicGuard, PropertyBool, Role},
    scene::{MethodCallSub, Pimpl, SceneNode, SceneNodePtr, SceneNodeType, SceneNodeWeak},
    ui::OnModify,
    ExecutorPtr,
};

use super::PluginSettings;

const P2P_RETRY_TIME: u64 = 20;

#[cfg(target_os = "android")]
mod paths {
    use crate::android::{get_appdata_path, get_external_storage_path};
    use std::path::PathBuf;

    pub fn get_base_path() -> PathBuf {
        get_external_storage_path().join("fud")
    }
    pub fn get_db_path() -> PathBuf {
        get_external_storage_path().join("fud/db")
    }
    pub fn get_downloads_path() -> PathBuf {
        get_external_storage_path().join("fud/downloads")
    }
    pub fn get_use_tor_filename() -> PathBuf {
        get_external_storage_path().join("use_tor.txt")
    }

    pub fn p2p_datastore_path() -> PathBuf {
        get_appdata_path().join("fud/p2p")
    }
    pub fn hostlist_path() -> PathBuf {
        get_appdata_path().join("fud/hostlist.tsv")
    }
}

#[cfg(not(target_os = "android"))]
mod paths {
    use std::path::PathBuf;

    pub fn get_base_path() -> PathBuf {
        dirs::data_local_dir().unwrap().join("darkfi/app/fud")
    }
    pub fn get_db_path() -> PathBuf {
        dirs::data_local_dir().unwrap().join("darkfi/app/fud/db")
    }
    pub fn get_downloads_path() -> PathBuf {
        dirs::data_local_dir().unwrap().join("darkfi/app/fud/downloads")
    }
    pub fn get_use_tor_filename() -> PathBuf {
        dirs::data_local_dir().unwrap().join("darkfi/app/use_tor.txt")
    }

    pub fn p2p_datastore_path() -> PathBuf {
        dirs::cache_dir().unwrap().join("darkfi/app/fud/p2p")
    }
    pub fn hostlist_path() -> PathBuf {
        dirs::cache_dir().unwrap().join("darkfi/app/fud/hostlist.tsv")
    }
}

use paths::*;

macro_rules! t { ($($arg:tt)*) => { trace!(target: "plugin::fud", $($arg)*); } }
macro_rules! d { ($($arg:tt)*) => { debug!(target: "plugin::fud", $($arg)*); } }
macro_rules! i { ($($arg:tt)*) => { info!(target: "plugin::fud", $($arg)*); } }
macro_rules! e { ($($arg:tt)*) => { error!(target: "plugin::fud", $($arg)*); } }

pub type FudPluginPtr = Arc<FudPlugin>;

pub struct FudPlugin {
    node: SceneNodeWeak,
    sg_root: SceneNodePtr,
    tasks: OnceLock<Vec<smol::Task<()>>>,

    p2p: P2pPtr,
    event_pub: PublisherPtr<FudEvent>,
    fud: Arc<Fud>,

    download_on_ready: Arc<Mutex<HashSet<Url>>>,

    settings: PluginSettings,
}

impl FudPlugin {
    pub async fn new(node: SceneNodeWeak, sg_root: SceneNodePtr, ex: ExecutorPtr) -> Result<Pimpl> {
        let node_ref = &node.upgrade().unwrap();
        // let fud_node_id = PropertyStr::wrap(node_ref, Role::Internal, "node_id", 0).unwrap();
        let fud_ready = PropertyBool::wrap(node_ref, Role::Internal, "ready", 0).unwrap();
        fud_ready.set(&mut PropertyAtomicGuard::none(), false);

        let setting_root = Arc::new(SceneNode::new("setting", SceneNodeType::SettingRoot));
        node_ref.clone().link(setting_root.clone());

        let basedir = get_base_path();

        i!("Starting Fud backend");
        let db_path = get_db_path();
        let db = match sled::open(&db_path) {
            Ok(db) => db,
            Err(err) => {
                e!("Sled database '{}' failed to open: {err}!", db_path.display());
                return Err(Error::SledDbErr)
            }
        };

        let setting_tree = db.open_tree("settings")?;
        let settings = PluginSettings { setting_root, sled_tree: setting_tree };

        let mut fud_settings: FudSettings = Default::default();
        fud_settings.base_dir = basedir.to_string_lossy().to_string();
        let mut p2p_settings: NetSettings = Default::default();
        p2p_settings.magic_bytes = MagicBytes([73, 59, 41, 23]);
        p2p_settings.app_version = semver::Version::parse("0.5.0").unwrap();
        p2p_settings.app_name = "fud".to_string();
        if get_use_tor_filename().exists() {
            i!("Setup P2P network [tor]");
            let mut tor_profile = NetworkProfile::tor_default();
            tor_profile.outbound_connect_timeout = 60;
            p2p_settings.profiles.insert("tor".to_string(), tor_profile);
            p2p_settings.outbound_peer_discovery_cooloff_time = 60;

            p2p_settings.seeds.push(
                url::Url::parse(
                    "tor://g7fxelebievvpr27w7gt24lflptpw3jeeuvafovgliq5utdst6xyruyd.onion:24442",
                )
                .unwrap(),
            );
            p2p_settings.seeds.push(
                url::Url::parse(
                    "tor://yvklzjnfmwxhyodhrkpomawjcdvcaushsj6torjz2gyd7e25f3gfunyd.onion:24442",
                )
                .unwrap(),
            );
            p2p_settings.active_profiles = vec!["tor".to_string()];

            fud_settings.pow.btc_electrum_nodes.push(
                url::Url::parse(
                    "tor://hezojf7rda2c33yxgcgcvvsxflechdz5vkm64gwlszgx2r4gc5e42kqd.onion:50001",
                )
                .unwrap(),
            );
            fud_settings.pow.btc_electrum_nodes.push(
                url::Url::parse(
                    "tor://n4widoxtm3xpo2fjvtdffhb63q5td3utaxkolaegnpzb5khbwxvdrlad.onion:50001",
                )
                .unwrap(),
            );
            fud_settings.pow.btc_electrum_nodes.push(
                url::Url::parse(
                    "tor://duras25aqnp3tnn2zgma7pusms6c7umtunyu2sp6e5byotr3c4c6rzad.onion:50001",
                )
                .unwrap(),
            );
            fud_settings.pow.btc_electrum_nodes.push(
                url::Url::parse(
                    "tor://n3dz6thzxobyphuosoftgtf36rnsxlsjknke4yrbdys55zvd7nsx7qid.onion:50001",
                )
                .unwrap(),
            );
        } else {
            i!("Setup P2P network [clearnet]");
            let mut profile = NetworkProfile::default();
            profile.outbound_connect_timeout = 40;
            profile.channel_handshake_timeout = 30;
            p2p_settings.profiles.insert("tcp+tls".to_string(), profile);
            p2p_settings.active_profiles = vec!["tcp+tls".to_string()];

            p2p_settings.seeds.push(url::Url::parse("tcp+tls://lilith0.dark.fi:24441").unwrap());
            p2p_settings.seeds.push(url::Url::parse("tcp+tls://lilith1.dark.fi:24441").unwrap());

            fud_settings
                .pow
                .btc_electrum_nodes
                .push(url::Url::parse("tcp+tls://erbium1.sytes.net:50002").unwrap());
            fud_settings
                .pow
                .btc_electrum_nodes
                .push(url::Url::parse("tcp+tls://ecdsa.net:110").unwrap());
            fud_settings
                .pow
                .btc_electrum_nodes
                .push(url::Url::parse("tcp+tls://electrum.no-ip.org:50002").unwrap());
            fud_settings
                .pow
                .btc_electrum_nodes
                .push(url::Url::parse("tcp+tls://electrumx.not.fyi:50002").unwrap());
        }
        p2p_settings.p2p_datastore = p2p_datastore_path().into_os_string().into_string().ok();
        p2p_settings.hostlist = hostlist_path().into_os_string().into_string().ok();

        settings.add_p2p_settings(&p2p_settings);
        // TODO: add other fud settings

        settings.load_settings();
        settings.update_p2p_settings(&mut p2p_settings);

        let p2p = match P2p::new(p2p_settings.clone(), ex.clone()).await {
            Ok(p2p) => p2p,
            Err(err) => {
                e!("Create p2p network failed: {err}!");
                return Err(Error::ServiceFailed)
            }
        };

        p2p.session_direct().start_peer_discovery();

        let event_pub = Publisher::new();
        let fud: Arc<Fud> =
            match Fud::new(fud_settings, p2p.clone(), &db, event_pub.clone(), ex.clone()).await {
                Ok(fud) => fud,
                Err(err) => {
                    e!("Cannot create fud instance: {err}");
                    return Err(Error::ServiceFailed)
                }
            };

        let self_ = Arc::new(Self {
            node: node.clone(),
            sg_root,
            tasks: OnceLock::new(),
            p2p,
            event_pub,
            fud,
            download_on_ready: Arc::new(Mutex::new(HashSet::new())),
            settings,
        });
        self_.clone().start(ex).await;
        Ok(Pimpl::Fud(self_))
    }

    async fn apply_settings(self_: Arc<Self>, _batch: BatchGuardPtr) {
        self_.settings.save_settings();

        let p2p_settings = self_.p2p.settings();
        let mut write_guard = p2p_settings.write().await;
        self_.settings.update_p2p_settings(&mut write_guard);

        // TODO: add other fud settings
    }

    async fn start(self: Arc<Self>, ex: ExecutorPtr) {
        i!("Registering Fud protocol");
        let registry = self.p2p.protocol_registry();
        let fud = self.fud.clone();
        let p2p = self.p2p.clone();
        registry
            .register(SESSION_DIRECT | SESSION_INBOUND, move |channel, _| {
                let fud_ = fud.clone();
                let p2p_ = p2p.clone();
                async move { ProtocolFud::init(fud_, channel, p2p_).await.unwrap() }
            })
            .await;

        let me = Arc::downgrade(&self);

        let node = &self.node.upgrade().unwrap();

        let method_sub = node.subscribe_method_call("get").unwrap();
        let me2 = me.clone();
        let get_method_task =
            ex.spawn(async move { while Self::process_get(&me2, &method_sub).await {} });

        let event_pub = self.event_pub.clone();
        let me2 = me.clone();
        let ev_task = ex.spawn(async move {
            Self::process_events(&me2, event_pub).await;
        });

        let mut on_modify = OnModify::new(ex.clone(), self.node.clone(), me.clone());

        // `apply_settings` is triggered if any setting changes
        for setting_node in self.settings.setting_root.get_children().iter() {
            on_modify.when_change(
                setting_node.get_property("value").clone().unwrap(),
                Self::apply_settings,
            );
        }

        let fud = self.fud.clone();
        let start_task = ex.spawn(async move {
            while fud.start().await.is_err() {
                sleep(10).await;
            }
        });

        let mut tasks = vec![get_method_task, ev_task, start_task];
        tasks.append(&mut on_modify.tasks);
        self.tasks.set(tasks).unwrap();

        i!("Starting Fud P2P");
        while let Err(err) = self.p2p.clone().start().await {
            // This usually means we cannot listen on the inbound ports
            e!("Failed to start fud's p2p network: {err}!");
            e!("Usually this means there is another process listening on the same ports.");
            e!("Trying again in {P2P_RETRY_TIME} secs");
            sleep(P2P_RETRY_TIME).await;
        }
    }

    fn string_to_hash(str: &str) -> std::io::Result<blake3::Hash> {
        let mut hash_buf = vec![];

        match bs58::decode(str).onto(&mut hash_buf) {
            Ok(_) => {}
            Err(_) => {
                return Err(std::io::Error::new(std::io::ErrorKind::Other, "Invalid fud hash"))
            }
        }

        if hash_buf.len() != 32 {
            return Err(std::io::Error::new(std::io::ErrorKind::Other, "Invalid fud hash"))
        }

        let mut hash_buf_arr = [0u8; 32];
        hash_buf_arr.copy_from_slice(&hash_buf);

        Ok(blake3::Hash::from_bytes(hash_buf_arr))
    }

    async fn process_get(me: &Weak<Self>, sub: &MethodCallSub) -> bool {
        let Ok(method_call) = sub.receive().await else {
            d!("Fud event relayer closed");
            return false
        };

        t!("method called: get({method_call:?})");
        assert!(method_call.send_res.is_none());

        fn decode_data(data: &[u8]) -> std::io::Result<(String, Url)> {
            let mut cur = Cursor::new(&data);
            let url = Url::decode(&mut cur)?;
            let Some(hash_string) = url.host_str().clone() else {
                return Err(std::io::Error::new(std::io::ErrorKind::Other, "Missing fud hash"))
            };
            let hash_string = hash_string.to_string();

            Ok((hash_string, url))
        }

        let Some(self_) = me.upgrade() else {
            // Should not happen
            panic!("self destroyed before get_method_task was stopped!");
        };

        let Ok((hash_string, url)) = decode_data(&method_call.data) else {
            e!("get() method invalid arg data");
            return true
        };

        let Ok(hash) = FudPlugin::string_to_hash(&hash_string) else {
            let mut data = vec![];
            "invalid fud url".encode(&mut data).unwrap();
            self_.update_file(hash_string, "error", data).await;
            return true
        };

        if self_.node.upgrade().unwrap().get_property_bool("ready").unwrap() {
            let file_selection = match url.path() {
                "/" | "" => fud::util::FileSelection::All,
                path => {
                    let mut selection = HashSet::new();
                    selection.insert(PathBuf::from(path.strip_prefix("/").unwrap_or(path)));
                    fud::util::FileSelection::Set(selection)
                }
            };
            let _ = self_
                .fud
                .get(&hash, &get_downloads_path().join(&hash_string), file_selection)
                .await;
        } else {
            self_.download_on_ready.lock().await.insert(url);
        }

        true
    }

    async fn update_file(self: &Arc<Self>, hash: String, status: &str, encoded_data: Vec<u8>) {
        let window = self.sg_root.lookup_node("/window");
        if window.is_none() {
            return
        }
        let window = window.unwrap();

        for child in window.get_children() {
            if let Some(chatty) = child.lookup_node("/content/chatty") {
                let mut data = vec![];
                hash.encode(&mut data).unwrap();
                status.encode(&mut data).unwrap();
                data.extend(encoded_data.clone());
                let _ = chatty.call_method("update_file", data).await;
            }
        }
    }

    async fn process_events(me: &Weak<Self>, publisher: PublisherPtr<FudEvent>) {
        let Some(self_) = me.upgrade() else {
            // Should not happen
            panic!("self destroyed before ev_task was stopped!");
        };

        let sub = publisher.subscribe().await;

        loop {
            match sub.receive().await {
                FudEvent::Ready => {
                    let atom = &mut PropertyAtomicGuard::none();
                    self_
                        .node
                        .upgrade()
                        .unwrap()
                        .set_property_bool(atom, Role::App, "ready", true)
                        .unwrap();

                    let window = self_.sg_root.lookup_node("/window");
                    if window.is_none() {
                        continue
                    }

                    for url in self_.download_on_ready.lock().await.iter() {
                        let mut data = vec![];
                        url.encode(&mut data).unwrap();
                        let _ = self_.node.upgrade().unwrap().call_method("get", data).await;
                    }
                }
                FudEvent::DownloadStarted(ev) => {
                    let mut data = vec![];
                    let bytes_downloaded = ev.resource.target_bytes_downloaded as f32;
                    let bytes_size = ev.resource.target_bytes_size as f32;
                    let progress =
                        if bytes_size != 0.0 { bytes_downloaded / bytes_size * 100.0 } else { 0.0 };
                    progress.encode(&mut data).unwrap();
                    self_.update_file(hash_to_string(&ev.resource.hash), "downloading", data).await;
                }
                FudEvent::ChunkDownloadCompleted(ev) => {
                    let mut data = vec![];
                    let bytes_downloaded = ev.resource.target_bytes_downloaded as f32;
                    let bytes_size = ev.resource.target_bytes_size as f32;
                    let progress =
                        if bytes_size != 0.0 { bytes_downloaded / bytes_size * 100.0 } else { 0.0 };
                    progress.encode(&mut data).unwrap();
                    self_.update_file(hash_to_string(&ev.resource.hash), "downloading", data).await;
                }
                FudEvent::DownloadCompleted(ev) => {
                    let mut data = vec![];
                    let path_string = ev.resource.path.to_string_lossy().to_string();
                    path_string.encode(&mut data).unwrap();
                    self_.update_file(hash_to_string(&ev.resource.hash), "downloaded", data).await;
                }
                FudEvent::DownloadError(ev) => {
                    let mut data = vec![];
                    ev.error.encode(&mut data).unwrap();
                    self_.update_file(hash_to_string(&ev.hash), "error", data).await;
                }
                FudEvent::MissingChunks(ev) => {
                    let mut data = vec![];
                    "missing chunks".encode(&mut data).unwrap();
                    self_.update_file(hash_to_string(&ev.hash), "error", data).await;
                }
                FudEvent::MetadataNotFound(ev) => {
                    let mut data = vec![];
                    "missing metadata".encode(&mut data).unwrap();
                    self_.update_file(hash_to_string(&ev.hash), "error", data).await;
                }
                _ => {}
            };
        }
    }
}
