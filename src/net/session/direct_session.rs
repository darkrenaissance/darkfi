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

//! Direct connections session. Manages the creation of direct sessions.
//! Used to create a direct session and to stop and start the session.
//!
//! A direct session is a type of outbound session in which a protocol can
//! open a temporary channel (stopped after used) to a peer. Direct sessions
//! do not loop continually, once stopped the session will not try to reopen
//! a connection.
//!
//! [`ChannelBuilder`] is used to create new direct connections.
//!
//! If there is no slots in the outbound session, the direct session can
//! optionnally handle peer discovery.

use std::{
    collections::HashMap,
    sync::{atomic::Ordering, Arc, Weak},
    time::Duration,
};

use async_trait::async_trait;
use log::{debug, error, info, warn};
use smol::lock::Mutex as AsyncMutex;
use url::Url;

use super::{
    super::{
        connector::Connector,
        p2p::{P2p, P2pPtr},
    },
    Session, SessionBitFlag, SESSION_DIRECT,
};
use crate::{
    net::{
        dnet,
        dnet::{dnetev, DnetEvent},
        hosts::HostState,
        message::GetAddrsMessage,
        session::HostColor,
        ChannelPtr,
    },
    system::{sleep, timeout::timeout, CondVar, PublisherPtr, StoppableTask, StoppableTaskPtr},
    Error, Result,
};

pub type DirectSessionPtr = Arc<DirectSession>;

/// Defines direct connections session.
pub struct DirectSession {
    /// Weak pointer to parent p2p object
    pub(in crate::net) p2p: Weak<P2p>,
    /// Service to create direct channels
    channel_builder: Arc<AsyncMutex<ChannelBuilder>>,
    /// Tasks that are trying to create a direct channel (they retry until they succeed).
    /// A task is removed once the channel is successfully created.
    retries_tasks: Arc<AsyncMutex<HashMap<Url, Arc<StoppableTask>>>>,
    /// Peer discovery task
    peer_discovery: Arc<PeerDiscovery>,
}

impl DirectSession {
    /// Create a new direct session.
    pub fn new(p2p: Weak<P2p>) -> DirectSessionPtr {
        Arc::new_cyclic(|session| Self {
            p2p,
            channel_builder: Arc::new(AsyncMutex::new(ChannelBuilder::new(session.clone()))),
            retries_tasks: Arc::new(AsyncMutex::new(HashMap::new())),
            peer_discovery: PeerDiscovery::new(session.clone()),
        })
    }

    /// Start the direct session.
    pub(crate) async fn start(self: Arc<Self>) {
        self.peer_discovery.clone().start().await;
    }

    /// Stops the direct session.
    pub async fn stop(&self) {
        self.peer_discovery.clone().stop().await;

        for (_, task) in self.retries_tasks.lock().await.iter() {
            task.stop().await;
        }
    }

    /// Notify the peer discovery task to start it.
    /// The direct session's peer discovery process will not start until this
    /// method is called.
    /// If there are outbound slots, peer discovery does not start even if this
    /// method is called, we let the outbound session take care of it.
    pub fn start_peer_discovery(&self) {
        self.peer_discovery.notify();
    }

    /// Create a new channel to `addr` in the direct session.
    pub async fn create_channel(&self, addr: &Url) -> Result<ChannelPtr> {
        self.channel_builder.lock().await.new_channel(addr).await
    }

    /// Try to create a new channel until it succeeds, then notify `channel_pub`.
    /// If it fails to create a channel, a task will sleep
    /// `outbound_connect_timeout` seconds and try again.
    pub async fn create_channel_with_retries(
        &self,
        addr: Url,
        channel_pub: PublisherPtr<ChannelPtr>,
    ) {
        let channel_builder = self.channel_builder.clone();
        let task = StoppableTask::new();
        let retries_tasks_lock = self.retries_tasks.clone();
        let mut retries_tasks = self.retries_tasks.lock().await;
        let p2p = self.p2p().clone();
        retries_tasks.insert(addr.clone(), task.clone());
        drop(retries_tasks);

        task.clone().start(
            async move {
                loop {
                    let mut builder = channel_builder.lock().await;
                    let res = builder.new_channel(&addr).await;
                    match res {
                        Ok(channel) => {
                            channel_pub.notify(channel).await;
                            let mut retries_tasks = retries_tasks_lock.lock().await;
                            retries_tasks.remove(&addr);
                            break
                        }
                        Err(Error::HostDoesNotExist) => break,
                        Err(_) => {
                            drop(builder);
                            let settings = p2p.settings().read_arc().await;
                            sleep(settings.outbound_connect_timeout).await;
                        }
                    }
                }

                Ok(())
            },
            |res| async {
                match res {
                    Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                    Err(e) => {
                        error!(target: "net::direct_session::create_channel_with_retries()", "{e}")
                    }
                }
            },
            Error::DetachedTaskStopped,
            self.p2p().executor(),
        );
    }
}

#[async_trait]
impl Session for DirectSession {
    fn p2p(&self) -> P2pPtr {
        self.p2p.upgrade().unwrap()
    }

    fn type_id(&self) -> SessionBitFlag {
        SESSION_DIRECT
    }
}

pub struct ChannelBuilder {
    /// Weak pointer to parent object
    session: Weak<DirectSession>,
    connector: Option<Arc<Connector>>,
}

impl ChannelBuilder {
    pub fn new(session: Weak<DirectSession>) -> Self {
        Self { session: session.clone(), connector: None }
    }

    fn session(&self) -> DirectSessionPtr {
        self.session.upgrade().unwrap()
    }

    fn connector(&mut self) -> Arc<Connector> {
        match &self.connector {
            Some(c) => c.clone(),
            None => {
                self.connector = Some(Arc::new(Connector::new(
                    self.session().p2p().settings(),
                    self.session.clone(),
                )));
                self.connector.clone().unwrap()
            }
        }
    }

    /// Create a new channel to `addr` in the direct session.
    /// The transport is verified before the connection is started.
    pub async fn new_channel(&mut self, addr: &Url) -> Result<ChannelPtr> {
        info!(
            target: "net::direct_session",
            "[P2P] Connecting to direct outbound [{addr}]",
        );

        let settings = self.session().p2p().settings().read_arc().await;
        let seeds = settings.seeds.clone();
        let allowed_transports = settings.allowed_transports.clone();
        drop(settings);

        // Do not establish a connection to a host that is also configured as a seed.
        // This indicates a user misconfiguration.
        if seeds.contains(addr) {
            error!(
                target: "net::direct_session",
                "[P2P] Suspending direct connection to seed [{}]", addr.clone(),
            );
            return Err(Error::ConnectFailed(format!("[{addr}]: Direct connection to seed")))
        }

        // Abort if we are trying to connect to our own external address.
        let hosts = self.session().p2p().hosts();
        let external_addrs = hosts.external_addrs().await;
        if external_addrs.contains(addr) {
            warn!(
                target: "net::hosts::check_addrs",
                "[P2P] Suspending direct connection to external addr [{}]", addr.clone(),
            );
            return Err(Error::ConnectFailed(format!(
                "[{addr}]: Direct connection to external addr"
            )))
        }

        // Abort if we do not support this transport.
        if !allowed_transports.contains(&addr.scheme().to_string()) {
            return Err(Error::UnsupportedTransport(addr.scheme().to_string()))
        }

        // Abort if this peer is IPv6 and we do not support it.
        if !hosts.ipv6_available.load(Ordering::SeqCst) && hosts.is_ipv6(addr) {
            return Err(Error::ConnectFailed(format!("[{addr}]: IPv6 is unavailable")))
        }

        if let Err(e) = hosts.try_register(addr.clone(), HostState::Connect) {
            debug!(target: "net::direct_session",
                "[P2P] Cannot connect to direct={addr}, err={e}");
            return Err(e)
        }

        match self.connector().connect(addr).await {
            Ok((_, channel)) => {
                info!(
                    target: "net::direct_session",
                    "[P2P] Direct outbound connected [{}]",
                    channel.display_address()
                );

                // Register the new channel
                match self
                    .session()
                    .register_channel(channel.clone(), self.session().p2p().executor())
                    .await
                {
                    Ok(()) => Ok(channel),
                    Err(e) => {
                        warn!(
                            target: "net::direct_session",
                            "[P2P] Unable to connect to direct outbound [{}]: {e}",
                            channel.display_address(),
                        );

                        // Free up this addr for future operations.
                        if let Err(e) = self.session().p2p().hosts().unregister(channel.address()) {
                            warn!(target: "net::direct_session", "[P2P] Error while unregistering addr={}, err={e}", channel.display_address());
                        }

                        Err(e)
                    }
                }
            }
            Err(e) => {
                warn!(
                    target: "net::direct_session",
                    "[P2P] Unable to connect to direct outbound: {e}",
                );

                // Free up this addr for future operations.
                if let Err(e) = self.session().p2p().hosts().unregister(addr) {
                    warn!(target: "net::direct_session", "[P2P] Error while unregistering addr={addr}, err={e}");
                }

                Err(e)
            }
        }
    }
}

/// PeerDiscovery process for that sends `GetAddrs` messages to a random
/// whitelist or greylist host (creating a channel in the direct session).
/// If it's unsuccessful after two attempts, connect to our seed nodes and
/// perform `SeedSyncSession`.
struct PeerDiscovery {
    process: StoppableTaskPtr,
    init: CondVar,
    session: Weak<DirectSession>,
}

impl PeerDiscovery {
    fn new(session: Weak<DirectSession>) -> Arc<Self> {
        Arc::new(Self { process: StoppableTask::new(), init: CondVar::new(), session })
    }
}

impl PeerDiscovery {
    async fn start(self: Arc<Self>) {
        let ex = self.p2p().executor();
        self.process.clone().start(
            async move {
                self.run().await;
                Ok(())
            },
            // Ignore stop handler
            |_| async {},
            Error::NetworkServiceStopped,
            ex,
        );
    }
    async fn stop(self: Arc<Self>) {
        self.process.stop().await;
    }

    /// Peer discovery's main process. For the first two attempts, this will
    /// broadcast a `GetAddrs` message to request more peers. If we are not
    /// connected to any peer, we try to create a channel in the direct session
    /// to a random whitelist or greylist host.
    /// Other parts of the P2P stack will then handle the incoming addresses
    /// and place them in the hosts list.
    ///
    /// On the third attempt, and if we still haven't made any connections,
    /// this function will then call `p2p.seed()` which triggers a
    /// `SeedSyncSession` that will connect to configured seeds and request
    /// peers from them.
    ///
    /// This function will also sleep `outbound_peer_discovery_attempt_time`
    /// seconds after broadcasting in order to let the P2P stack receive and
    /// work through the addresses it is expecting.
    ///
    /// Peer discovery will only start once `notify()` is called.
    async fn run(self: Arc<Self>) {
        // DirectSession can handle peer discovery only if there is no outbound
        // slot. Otherwise we let the outbound session take care of it.
        let settings = self.p2p().settings().read_arc().await;
        if settings.outbound_connections > 0 {
            return
        }

        // Wait for the peer discovery to be notified
        self.init.wait().await;

        let mut current_attempt = 0;
        loop {
            dnetev!(self, OutboundPeerDiscovery, {
                attempt: current_attempt,
                state: "wait",
            });

            // Read the current P2P settings
            let settings = self.p2p().settings().read_arc().await;
            let outbound_peer_discovery_cooloff_time =
                settings.outbound_peer_discovery_cooloff_time;
            let outbound_peer_discovery_attempt_time =
                settings.outbound_peer_discovery_attempt_time;
            let getaddrs_max = settings.getaddrs_max;
            let allowed_transports = settings.allowed_transports.clone();
            let seeds = settings.seeds.clone();
            drop(settings);

            current_attempt += 1;

            if current_attempt >= 4 {
                info!(
                    target: "net::direct_session::peer_discovery()",
                    "[P2P] [PEER DISCOVERY] Sleeping and trying again. Attempt {current_attempt}"
                );

                dnetev!(self, OutboundPeerDiscovery, {
                    attempt: current_attempt,
                    state: "sleep",
                });

                sleep(outbound_peer_discovery_cooloff_time).await;
                current_attempt = 1;
            }

            // If we are not connected to any peer, try to create a channel
            // (using the direct session) to a random host from the goldlist,
            // whitelist, or greylist.
            let mut channel = None;
            if !self.p2p().is_connected() {
                for color in [HostColor::Gold, HostColor::White, HostColor::Grey].iter() {
                    if let Some((entry, _)) = self
                        .p2p()
                        .hosts()
                        .container
                        .fetch_random_with_schemes(color.clone(), &allowed_transports)
                    {
                        channel = self.p2p().session_direct().create_channel(&entry.0).await.ok();
                        break;
                    }
                }
            }

            // First 2 times try sending GetAddr to the network.
            // 3rd time do a seed sync (providing we have seeds configured).
            if self.p2p().is_connected() && current_attempt <= 2 {
                // Broadcast the GetAddrs message to all active peers.
                // If we have no active peers, we will perform a SeedSyncSession instead.
                info!(
                    target: "net::direct_session::peer_discovery()",
                    "[P2P] [PEER DISCOVERY] Asking peers for new peers to connect to...");

                dnetev!(self, OutboundPeerDiscovery, {
                    attempt: current_attempt,
                    state: "getaddr",
                });

                let get_addrs = GetAddrsMessage {
                    max: getaddrs_max.unwrap_or(1),
                    transports: allowed_transports,
                };

                self.p2p().broadcast(&get_addrs).await;

                // Wait for a hosts store update event
                let store_sub = self.p2p().hosts().subscribe_store().await;

                let result = timeout(
                    Duration::from_secs(outbound_peer_discovery_attempt_time),
                    store_sub.receive(),
                )
                .await;

                match result {
                    Ok(addrs_len) => {
                        info!(
                            target: "net::direct_session::peer_discovery()",
                            "[P2P] [PEER DISCOVERY] Discovered {addrs_len} peers"
                        );
                        // Found some addrs, reset `current_attempt`
                        if addrs_len > 0 {
                            current_attempt = 0;
                        }
                    }
                    Err(_) => {
                        warn!(
                            target: "net::direct_session::peer_discovery()",
                            "[P2P] [PEER DISCOVERY] Waiting for addrs timed out."
                        );
                        // Just do seed next time
                        current_attempt = 3;
                    }
                }

                // NOTE: not every call to subscribe() in net/ has a
                // corresponding unsubscribe(). To do this we need async
                // Drop. For now it's sufficient for publishers to be
                // de-allocated when the Session completes.
                store_sub.unsubscribe().await;
            } else if !seeds.is_empty() {
                info!(
                    target: "net::direct_session::peer_discovery()",
                    "[P2P] [PEER DISCOVERY] Asking seeds for new peers to connect to...");

                dnetev!(self, OutboundPeerDiscovery, {
                    attempt: current_attempt,
                    state: "seed",
                });

                self.p2p().seed().await;
            }

            // Stop the channel we created for peer discovery
            if let Some(ch) = channel {
                ch.stop().await;
            }

            // Give some time for new connections to be established
            sleep(outbound_peer_discovery_attempt_time).await;
        }
    }

    /// Init peer discovery by sending a notification to `init`.
    /// Uses the underlying `CondVar` method `notify()`.
    pub fn notify(&self) {
        self.init.notify()
    }

    fn session(&self) -> DirectSessionPtr {
        self.session.upgrade().unwrap()
    }

    fn p2p(&self) -> P2pPtr {
        self.session().p2p()
    }
}
