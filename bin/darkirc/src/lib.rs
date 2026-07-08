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

use sled_overlay::sled;
use smol::lock::Mutex;
use std::{collections::HashSet, path::PathBuf};

use darkfi::{
    event_graph::EventGraphPtr, net::P2pPtr, rpc::jsonrpc::JsonSubscriber, system::StoppableTaskPtr,
};
use darkfi_serial::{async_trait, SerialDecodable, SerialEncodable};

/// IRC server and client handler implementation
pub mod irc;
use irc::server::IrcServer;

use crate::irc::server::MAX_NICK_LEN;

/// Cryptography utilities
pub mod crypto;

/// Pregenerated DarkIRC RLN identity commitments.
pub mod genesis_commits;

/// JSON-RPC methods
pub mod rpc;

/// Settings utilities
pub mod settings;

/// IRC PRIVMSG
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct Privmsg {
    pub version: u8,
    pub msg_type: u8,
    pub channel: String,
    pub nick: String,
    pub msg: String,
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
    /// Gource visualization JSON-RPC subscriber
    gource_sub: JsonSubscriber,
    /// Replay logs (DB) path
    replay_datastore: PathBuf,
}

impl DarkIrc {
    pub fn new(
        p2p: P2pPtr,
        sled: sled::Db,
        event_graph: EventGraphPtr,
        dnet_sub: JsonSubscriber,
        deg_sub: JsonSubscriber,
        gource_sub: JsonSubscriber,
        replay_datastore: PathBuf,
    ) -> Self {
        Self {
            p2p,
            sled,
            event_graph,
            rpc_connections: Mutex::new(HashSet::new()),
            dnet_sub,
            deg_sub,
            gource_sub,
            replay_datastore,
        }
    }
}

pub fn pad(string: &str) -> Vec<u8> {
    let mut bytes = string.as_bytes().to_vec();
    bytes.resize(MAX_NICK_LEN, 0x00);
    bytes
}

pub fn unpad(vec: &mut Vec<u8>) {
    if let Some(i) = vec.iter().rposition(|x| *x != 0) {
        let new_len = i + 1;
        vec.truncate(new_len);
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        sync::{
            atomic::{AtomicU16, Ordering},
            Arc,
        },
    };

    use darkfi::{
        event_graph::{
            proto::{ProtocolEventGraph, RangeCursor, SyncDirection},
            Event, EventGraph, EventGraphConfig, EventGraphPtr, Header, NULL_PARENTS,
        },
        net::{
            session::SESSION_DEFAULT,
            settings::{NetworkProfile, Settings},
            P2p, P2pPtr,
        },
        system::sleep,
    };
    use darkfi_serial::{deserialize_async_partial, serialize_async};
    use easy_parallel::Parallel;
    use sled_overlay::sled;
    use smol::{channel, future, Executor};
    use url::Url;

    use super::Privmsg;

    struct HistoryNode {
        p2p: P2pPtr,
        event_graph: EventGraphPtr,
    }

    fn alloc_port_base() -> u16 {
        static NEXT: AtomicU16 = AtomicU16::new(24_400);
        NEXT.fetch_add(2, Ordering::SeqCst)
    }

    fn history_test_config() -> EventGraphConfig {
        EventGraphConfig {
            initial_genesis: 1_704_067_200_000,
            hours_rotation: 1,
            genesis_contents: b"darkirc-mobile-history-test".to_vec(),
            rln_enabled: false,
            pregenerated_identity_commitments: Vec::new(),
            max_dags: Some(5),
        }
    }

    async fn spawn_history_node(
        port_base: u16,
        port_offset: u16,
        peer_offsets: &[u16],
        ex: Arc<Executor<'static>>,
    ) -> HistoryNode {
        let mut profiles = HashMap::new();
        profiles.insert(
            "tcp".to_string(),
            NetworkProfile { outbound_connect_timeout: 2, ..Default::default() },
        );

        let inbound =
            vec![Url::parse(&format!("tcp://127.0.0.1:{}", port_base + port_offset)).unwrap()];
        let peers = peer_offsets
            .iter()
            .map(|offset| Url::parse(&format!("tcp://127.0.0.1:{}", port_base + *offset)).unwrap())
            .collect();

        let settings = Settings {
            localnet: true,
            inbound_addrs: inbound,
            outbound_connections: 0,
            inbound_connections: usize::MAX,
            peers,
            active_profiles: vec!["tcp".to_string()],
            profiles,
            ..Default::default()
        };

        let p2p = P2p::new(settings, ex.clone()).await.unwrap();
        let sled_db = sled::Config::new().temporary(true).open().unwrap();
        let event_graph =
            EventGraph::new(p2p.clone(), sled_db, "/tmp".into(), false, history_test_config(), ex)
                .await
                .unwrap();
        event_graph.synced.store(true, Ordering::Release);

        let event_graph_weak = Arc::downgrade(&event_graph);
        p2p.protocol_registry()
            .register(SESSION_DEFAULT, move |channel, _| {
                let event_graph_weak = event_graph_weak.clone();
                async move {
                    let event_graph = event_graph_weak
                        .upgrade()
                        .expect("EventGraph dropped before protocol factory invoked");
                    ProtocolEventGraph::init(event_graph, channel).await.unwrap()
                }
            })
            .await;

        HistoryNode { p2p, event_graph }
    }

    async fn make_history_network(ex: Arc<Executor<'static>>) -> Vec<HistoryNode> {
        let port_base = alloc_port_base();
        let nodes = vec![
            spawn_history_node(port_base, 0, &[1], ex.clone()).await,
            spawn_history_node(port_base, 1, &[0], ex).await,
        ];

        for node in &nodes {
            node.p2p.clone().start().await.unwrap();
        }
        sleep(5).await;

        nodes
    }

    async fn shutdown_history_network(nodes: &[HistoryNode]) {
        for node in nodes {
            node.p2p.stop().await;
        }
    }

    fn run_history_test<F, Fut>(body: F)
    where
        F: FnOnce(Arc<Executor<'static>>) -> Fut,
        Fut: std::future::Future<Output = ()>,
    {
        let ex = Arc::new(Executor::new());
        let ex_ = ex.clone();
        let (signal, shutdown) = channel::unbounded::<()>();
        Parallel::new().each(0..2, |_| future::block_on(ex.run(shutdown.recv()))).finish(|| {
            future::block_on(async {
                body(ex_).await;
                drop(signal);
            })
        });
    }

    fn genesis_for(event_graph: &EventGraphPtr, dag_ts: u64) -> Event {
        let content = event_graph.config.genesis_contents.clone();
        Event {
            header: Header {
                timestamp: dag_ts,
                parents: NULL_PARENTS,
                layer: 0,
                content_hash: blake3::hash(&content),
            },
            content,
        }
    }

    async fn select_current_dag(event_graph: &EventGraphPtr, dag_ts: u64) {
        *event_graph.current_genesis.write().await = genesis_for(event_graph, dag_ts);
    }

    async fn append_chat_event(
        event_graph: &EventGraphPtr,
        dag_ts: u64,
        timestamp: u64,
        nick: &str,
        message: &str,
    ) -> Event {
        select_current_dag(event_graph, dag_ts).await;

        let privmsg = Privmsg {
            version: 0,
            msg_type: 0,
            channel: "#mobile".to_string(),
            nick: nick.to_string(),
            msg: message.to_string(),
        };
        let event = Event::with_timestamp(timestamp, serialize_async(&privmsg).await, event_graph)
            .await
            .unwrap();
        event_graph.insert_signal_with_blob(&event, &[], &dag_ts.to_string()).await.unwrap();
        event
    }

    async fn page_messages(events: &[Event]) -> Vec<String> {
        let mut messages = Vec::with_capacity(events.len());
        for event in events {
            let (privmsg, _) = deserialize_async_partial::<Privmsg>(event.content()).await.unwrap();
            messages.push(privmsg.msg);
        }
        messages
    }

    #[test]
    fn mobile_chat_can_scroll_backwards_across_multiple_dags() {
        run_history_test(|ex| async move {
            const HOUR_MS: u64 = 3_600_000;
            const DAG_COUNT: usize = 5;
            const EVENTS_PER_DAG: usize = 4;
            const PAGE_SIZE: usize = 3;

            let nodes = make_history_network(ex).await;
            let source = nodes[0].event_graph.clone();
            let mobile = nodes[1].event_graph.clone();

            let latest_dag = source.current_genesis.read().await.header.timestamp;
            let mut dag_timestamps = Vec::with_capacity(DAG_COUNT);
            for offset in (0..DAG_COUNT).rev() {
                dag_timestamps.push(latest_dag.saturating_sub((offset as u64) * HOUR_MS));
            }

            let mut seeded = Vec::new();
            let mut expected_scrollback = Vec::new();
            for (dag_index, dag_ts) in dag_timestamps.iter().enumerate() {
                let mut dag_events = Vec::new();
                for event_index in 0..EVENTS_PER_DAG {
                    let message = format!("dag {dag_index} history message {event_index}");
                    let event = append_chat_event(
                        &source,
                        *dag_ts,
                        dag_ts + 10_000 + event_index as u64,
                        "alice",
                        &message,
                    )
                    .await;
                    dag_events.push(event);
                }

                expected_scrollback.extend(
                    (0..EVENTS_PER_DAG).rev().map(|event_index| {
                        format!("dag {dag_index} history message {event_index}")
                    }),
                );
                seeded.push(dag_events);
            }
            expected_scrollback =
                expected_scrollback.chunks(EVENTS_PER_DAG).rev().flatten().cloned().collect();

            for dag_events in &seeded {
                for event in dag_events {
                    assert!(mobile.fetch_event_from_dags(&event.id()).await.unwrap().is_none());
                }
            }

            mobile.sync_selected_headers(1).await.unwrap();

            let mut collected = Vec::new();
            for dag_ts in dag_timestamps.iter().rev() {
                let mut cursor = RangeCursor::newest();
                loop {
                    let page = mobile
                        .dag_sync_range(*dag_ts, cursor, SyncDirection::Backward, PAGE_SIZE)
                        .await
                        .unwrap();

                    collected.extend(page_messages(&page.events).await);
                    if page.exhausted {
                        break
                    }
                    cursor = page.next_cursor;
                }
            }

            assert_eq!(collected, expected_scrollback);

            for dag_events in &seeded {
                for event in dag_events {
                    assert!(mobile.fetch_event_from_dags(&event.id()).await.unwrap().is_some());
                }
            }

            shutdown_history_network(&nodes).await;
        });
    }
}
