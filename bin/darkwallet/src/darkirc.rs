/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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

use std::sync::{Arc, Mutex as SyncMutex};

use darkfi::{
    event_graph::{
        self,
        proto::{EventPut, ProtocolEventGraph},
        EventGraph, EventGraphPtr,
    },
    net::{session::SESSION_DEFAULT, settings::Settings as NetSettings, P2p, P2pPtr},
    system::{sleep, Subscription},
    Error,
};
use darkfi_serial::{
    async_trait, deserialize_async, serialize_async, Encodable, SerialDecodable, SerialEncodable,
};

use crate::{scene::SceneGraphPtr2, ExecutorPtr};

#[cfg(target_os = "android")]
const EVGRDB_PATH: &str = "/data/data/darkfi.darkwallet/evgrdb/";
#[cfg(target_os = "linux")]
const EVGRDB_PATH: &str = "evgrdb";

#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct Privmsg {
    pub channel: String,
    pub nick: String,
    pub msg: String,
}

async fn relay_darkirc_events(sg: SceneGraphPtr2, ev_sub: Subscription<event_graph::Event>) {
    loop {
        let ev = ev_sub.receive().await;

        // Try to deserialize the `Event`'s content into a `Privmsg`
        let privmsg: Privmsg = match deserialize_async(ev.content()).await {
            Ok(v) => v,
            Err(e) => {
                error!("[IRC CLIENT] Failed deserializing incoming Privmsg event: {}", e);
                continue
            }
        };

        if privmsg.channel != "#random" {
            continue
        }

        info!(target: "darkirc", "ev_id={:?}", ev.id());
        info!(target: "darkirc", "ev: {:?}", ev);
        info!(target: "darkirc", "privmsg: {:?}", privmsg);
        info!(target: "darkirc", "");

        let response_fn = Box::new(|_| {});

        let mut arg_data = vec![];
        ev.timestamp.encode(&mut arg_data).unwrap();
        ev.id().as_bytes().encode(&mut arg_data).unwrap();
        privmsg.nick.encode(&mut arg_data).unwrap();
        privmsg.msg.encode(&mut arg_data).unwrap();

        let mut sg = sg.lock().await;
        let chatview_node = sg.lookup_node_mut("/window/view/chatty").unwrap();
        chatview_node.call_method("insert_line", arg_data, response_fn).unwrap();
        drop(sg);
    }
}

pub type DarkIrcBackendPtr = Arc<DarkIrcBackend>;

struct DarkIrcData {
    p2p: P2pPtr,
    event_graph: EventGraphPtr,
    #[allow(dead_code)]
    ev_task: smol::Task<()>,
    db: sled::Db,
}

pub struct DarkIrcBackend(SyncMutex<Option<DarkIrcData>>);

impl DarkIrcBackend {
    pub fn new() -> Arc<Self> {
        Arc::new(Self(SyncMutex::new(None)))
    }

    pub async fn start(&self, sg: SceneGraphPtr2, ex: ExecutorPtr) -> darkfi::Result<()> {
        info!(target: "darkirc", "Starting DarkIRC backend");
        let sled_db = sled::open(EVGRDB_PATH)?;

        let mut p2p_settings: NetSettings = Default::default();
        p2p_settings.app_version = semver::Version::parse("0.5.0").unwrap();
        p2p_settings.seeds.push(url::Url::parse("tcp+tls://lilith1.dark.fi:5262").unwrap());

        let p2p = P2p::new(p2p_settings, ex.clone()).await?;

        let event_graph = EventGraph::new(
            p2p.clone(),
            sled_db.clone(),
            std::path::PathBuf::new(),
            false,
            "darkirc_dag",
            1,
            ex.clone(),
        )
        .await?;

        //self.prune_task.lock().unwrap() = Some(event_graph.prune_task.get().unwrap());

        info!(target: "darkirc", "Registering EventGraph P2P protocol");
        let event_graph_ = Arc::clone(&event_graph);
        let registry = p2p.protocol_registry();
        registry
            .register(SESSION_DEFAULT, move |channel, _| {
                let event_graph_ = event_graph_.clone();
                async move { ProtocolEventGraph::init(event_graph_, channel).await.unwrap() }
            })
            .await;

        let ev_sub = event_graph.event_pub.clone().subscribe().await;
        let ev_task = ex.spawn(relay_darkirc_events(sg, ev_sub));

        info!(target: "darkirc", "Starting P2P network");
        p2p.clone().start().await?;

        info!(target: "darkirc", "Waiting for some P2P connections...");
        sleep(5).await;

        // We'll attempt to sync {sync_attempts} times
        let sync_attempts = 4;
        for i in 1..=sync_attempts {
            info!(target: "darkirc", "Syncing event DAG (attempt #{})", i);
            match event_graph.dag_sync().await {
                Ok(()) => break,
                Err(e) => {
                    if i == sync_attempts {
                        error!("Failed syncing DAG. Exiting.");
                        p2p.stop().await;
                        return Err(Error::DagSyncFailed)
                    } else {
                        // TODO: Maybe at this point we should prune or something?
                        // TODO: Or maybe just tell the user to delete the DAG from FS.
                        error!("Failed syncing DAG ({}), retrying in {}s...", e, 4);
                        sleep(4).await;
                    }
                }
            }
        }

        *self.0.lock().unwrap() = Some(DarkIrcData { p2p, event_graph, ev_task, db: sled_db });

        Ok(())
    }

    pub async fn stop(&self) {
        info!(target: "darkirc", "Stopping DarkIRC backend");
        let self_ = self.0.lock().unwrap();
        let Some(self_) = &*self_ else {
            warn!(target: "darkirc", "Backend wasn't started");
            return
        };

        info!(target: "darkirc", "Stopping P2P network");
        self_.p2p.stop().await;

        info!(target: "darkirc", "Stopping IRC server");
        let prune_task = self_.event_graph.prune_task.get().unwrap();
        prune_task.stop().await;

        info!(target: "darkirc", "Flushing event graph sled database...");
        let Ok(flushed_bytes) = self_.db.flush_async().await else {
            error!(target: "darkirc", "Flushing event graph db failed");
            return
        };
        info!(target: "darkirc", "Flushed {} bytes", flushed_bytes);
        info!(target: "darkirc", "Shut down backend successfully");
    }

    pub async fn send(&self, privmsg: Privmsg) {
        let (p2p, evgr) = {
            let self_ = self.0.lock().unwrap();
            let data = self_.as_ref().expect("backend wasnt started");

            let evgr = data.event_graph.clone();
            let p2p = data.p2p.clone();
            (p2p, evgr)
        };
        let event = event_graph::Event::new(serialize_async(&privmsg).await, &evgr).await;
        if let Err(e) = evgr.dag_insert(&[event.clone()]).await {
            error!(target: "darkirc", "Failed inserting new event to DAG: {}", e);
        }

        p2p.broadcast(&EventPut(event)).await;
    }
}
