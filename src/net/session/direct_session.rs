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
use smol::lock::{Mutex as AsyncMutex, OnceCell};
use tracing::{error, warn};
use url::Url;

use super::{
    super::{
        connector::Connector,
        dnet::{self, dnetev, DnetEvent},
        hosts::{HostColor, HostState},
        message::GetAddrsMessage,
        p2p::{P2p, P2pPtr},
    },
    Session, SessionBitFlag, SESSION_DIRECT,
};
use crate::{
    net::ChannelPtr,
    system::{
        msleep, sleep, timeout::timeout, CondVar, PublisherPtr, StoppableTask, StoppableTaskPtr,
    },
    util::logger::verbose,
    Error, Result,
};

pub type DirectSessionPtr = Arc<DirectSession>;

/// Defines direct connections session.
pub struct DirectSession {
    /// Weak pointer to parent p2p object
    pub(in crate::net) p2p: Weak<P2p>,
    /// Connector to create direct connections
    connector: OnceCell<Connector>,
    /// Tasks that are trying to create a direct channel (they retry until they succeed).
    /// A task is removed once the channel is successfully created.
    retries_tasks: Arc<AsyncMutex<HashMap<Url, Arc<StoppableTask>>>>,
    /// Peer discovery task
    peer_discovery: Arc<PeerDiscovery>,
    /// Channel ID -> usage count
    channels_usage: Arc<AsyncMutex<HashMap<u32, u32>>>,
    /// Pending channel creation tasks
    tasks: Arc<AsyncMutex<HashMap<Url, Weak<ChannelTask>>>>,
}

impl DirectSession {
    /// Create a new direct session.
    pub fn new(p2p: Weak<P2p>) -> DirectSessionPtr {
        Arc::new_cyclic(|session| Self {
            p2p,
            connector: OnceCell::new(),
            retries_tasks: Arc::new(AsyncMutex::new(HashMap::new())),
            peer_discovery: PeerDiscovery::new(session.clone()),
            channels_usage: Arc::new(AsyncMutex::new(HashMap::new())),
            tasks: Arc::new(AsyncMutex::new(HashMap::new())),
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

    /// If there is an existing channel to the same address, this method will
    /// return it (even if the channel was not created by the direct session).
    /// Otherwise it will create a new channel to `addr` in the direct session.
    pub async fn get_channel(self: Arc<Self>, addr: &Url) -> Result<ChannelPtr> {
        // Check existing channels
        let channels = self.p2p().hosts().channels();
        if let Some(channel) =
            channels.iter().find(|&chan| chan.info.connect_addr == *addr).cloned()
        {
            let mut channels_usage = self.channels_usage.lock().await;
            if channel.is_stopped() {
                channel.clone().start(self.p2p().executor());
            }
            if channel.session_type_id() & SESSION_DIRECT != 0 {
                channels_usage.entry(channel.info.id).and_modify(|count| *count += 1).or_insert(1);
            }
            return Ok(channel);
        }

        let mut tasks = self.tasks.lock().await;

        // Check if task is already running for this addr
        if let Some(task) = tasks.get(addr) {
            if let Some(task) = task.upgrade() {
                drop(tasks);
                // Wait for the existing task to complete
                while task.output.lock().await.is_none() {
                    msleep(100).await;
                }
                return task.output.lock().await.clone().unwrap();
            } else {
                drop(tasks);
                // Wait for the existing task to be fully removed
                loop {
                    tasks = self.tasks.lock().await;
                    if !tasks.contains_key(addr) {
                        break
                    }
                    drop(tasks);
                    msleep(100).await;
                }
            }
        }

        // If no task running, create one
        let task = Arc::new(ChannelTask {
            session: Arc::downgrade(&self.clone()),
            addr: addr.clone(),
            output: Arc::new(AsyncMutex::new(None)),
        });
        tasks.insert(addr.clone(), Arc::downgrade(&task));
        drop(tasks);

        // Spawn a new task to create the channel
        let ex = self.p2p().executor();
        let addr_ = addr.clone();
        let self_ = self.clone();
        let task_ = task.clone();
        ex.spawn(async move {
            let res = self_.clone().new_channel(addr_.clone()).await;

            let mut output = task_.output.lock().await;
            *output = Some(res);
        })
        .detach();

        // Wait for completion
        while task.output.lock().await.is_none() {
            msleep(100).await;
        }
        let res = task.output.lock().await.as_ref().unwrap().clone();
        if let Ok(ref channel) = res {
            self.inc_channel_usage(channel, Arc::strong_count(&task).try_into().unwrap()).await;
        }
        res
    }

    /// Increment channel usage
    pub async fn inc_channel_usage(&self, channel: &ChannelPtr, n: u32) {
        if channel.session_type_id() & SESSION_DIRECT == 0 {
            // Do nothing if this is not a channel created by the direct session
            return
        }
        let mut channels_usage = self.channels_usage.lock().await;
        channels_usage.entry(channel.info.id).and_modify(|count| *count += n).or_insert(n);
    }

    /// Try to create a new channel until it succeeds, then notify `channel_pub`.
    /// If it fails to create a channel, a task will sleep
    /// `outbound_connect_timeout` seconds and try again.
    pub async fn get_channel_with_retries(
        self: Arc<Self>,
        addr: Url,
        channel_pub: PublisherPtr<ChannelPtr>,
    ) {
        let task = StoppableTask::new();
        let self_ = self.clone();
        let mut retries_tasks = self.retries_tasks.lock().await;
        retries_tasks.insert(addr.clone(), task.clone());
        drop(retries_tasks);

        task.clone().start(
            async move {
                loop {
                    let res = self_.clone().get_channel(&addr).await;
                    match res {
                        Ok(channel) => {
                            channel_pub.notify(channel).await;
                            let mut retries_tasks = self_.retries_tasks.lock().await;
                            retries_tasks.remove(&addr);
                            break
                        }
                        Err(_) => {
                            let outbound_connect_timeout = self_
                                .p2p()
                                .settings()
                                .read_arc()
                                .await
                                .outbound_connect_timeout(addr.scheme());
                            sleep(outbound_connect_timeout).await;
                        }
                    }
                }

                Ok(())
            },
            |res| async {
                match res {
                    Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                    Err(e) => {
                        error!(target: "net::direct_session::get_channel_with_retries", "{e}")
                    }
                }
            },
            Error::DetachedTaskStopped,
            self.p2p().executor(),
        );
    }

    async fn new_channel(self: Arc<Self>, addr: Url) -> Result<ChannelPtr> {
        if !self.connector.is_initialized() {
            let _ = self
                .connector
                .set(Connector::new(self.p2p().settings(), Arc::downgrade(&self.clone()).clone()))
                .await;
        }

        verbose!(
            target: "net::direct_session",
            "[P2P] Connecting to direct outbound [{addr}]",
        );

        let settings = self.p2p().settings().read_arc().await;
        let seeds = settings.seeds.clone();
        let active_profiles = settings.active_profiles.clone();
        drop(settings);

        // Do not establish a connection to a host that is also configured as a seed.
        // This indicates a user misconfiguration.
        if seeds.contains(&addr) {
            error!(
                target: "net::direct_session",
                "[P2P] Suspending direct connection to seed [{}]", addr.clone(),
            );
            return Err(Error::ConnectFailed(format!("[{addr}]: Direct connection to seed")))
        }

        // Abort if we are trying to connect to our own external address.
        let hosts = self.p2p().hosts();
        let external_addrs = hosts.external_addrs().await;
        if external_addrs.contains(&addr) {
            warn!(
                target: "net::hosts::check_addrs",
                "[P2P] Suspending direct connection to external addr [{}]", addr.clone(),
            );
            return Err(Error::ConnectFailed(format!(
                "[{addr}]: Direct connection to external addr"
            )))
        }

        // Abort if we do not support this transport.
        if !active_profiles.contains(&addr.scheme().to_string()) {
            return Err(Error::UnsupportedTransport(addr.scheme().to_string()))
        }

        // Abort if this peer is IPv6 and we do not support it.
        if !hosts.ipv6_available.load(Ordering::SeqCst) && hosts.is_ipv6(&addr) {
            return Err(Error::ConnectFailed(format!("[{addr}]: IPv6 is unavailable")))
        }

        // Set the addr to HostState::Connect
        loop {
            if let Err(e) = hosts.try_register(addr.clone(), HostState::Connect) {
                // If `try_register` failed because the addr is being refined, try again in a bit.
                if let Error::HostStateBlocked(from, _) = &e {
                    if from == "Refine" {
                        // TODO: Add a setting or have a way to wait for the refinery to complete
                        sleep(5).await;
                        continue
                    }
                }

                error!(target: "net::direct_session",
                    "[P2P] Cannot connect to direct={addr}, err={e}");
                return Err(e)
            }
            break
        }

        dnetev!(self, DirectConnecting, {
            connect_addr: addr.clone(),
        });

        // Attempt channel creation
        match self.connector.get().unwrap().connect(&addr).await {
            Ok((_, channel)) => {
                verbose!(
                    target: "net::direct_session",
                    "[P2P] Direct outbound connected [{}]",
                    channel.display_address()
                );

                dnetev!(self, DirectConnected, {
                    connect_addr: channel.info.connect_addr.clone(),
                    addr: channel.display_address().clone(),
                    channel_id: channel.info.id
                });

                // Register the new channel
                match self.register_channel(channel.clone(), self.p2p().executor()).await {
                    Ok(()) => Ok(channel),
                    Err(e) => {
                        warn!(
                            target: "net::direct_session",
                            "[P2P] Unable to connect to direct outbound [{}]: {e}",
                            channel.display_address(),
                        );

                        dnetev!(self, DirectDisconnected, {
                            connect_addr: channel.info.connect_addr.clone(),
                            err: e.to_string()
                        });

                        // Free up this addr for future operations.
                        if let Err(e) = self.p2p().hosts().unregister(channel.address()) {
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

                dnetev!(self, DirectDisconnected, {
                    connect_addr: addr.clone(),
                    err: e.to_string()
                });

                // Free up this addr for future operations.
                if let Err(e) = self.p2p().hosts().unregister(&addr) {
                    warn!(target: "net::direct_session", "[P2P] Error while unregistering addr={addr}, err={e}");
                }

                Err(e)
            }
        }
    }

    /// Close a direct channel if it's not used by anything.
    /// `AsyncDrop` would be great here (<https://doc.rust-lang.org/std/future/trait.AsyncDrop.html>)
    /// but it's still in nightly. For now you must call this method manually
    /// once you are done with a direct channel.
    /// Returns `true` if the channel is stopped.
    pub async fn cleanup_channel(self: Arc<Self>, channel: ChannelPtr) -> bool {
        if channel.session_type_id() & SESSION_DIRECT == 0 {
            // Do nothing if this is not a channel created by the direct session
            return false
        }

        let mut channels_usage = self.channels_usage.lock().await;
        let usage_count = channels_usage.get_mut(&channel.info.id);
        if usage_count.is_none() {
            let _ = self.p2p().hosts().unregister(channel.address());
            channel.stop().await;
            return true
        }
        let usage_count = usage_count.unwrap();
        if *usage_count > 0 {
            *usage_count -= 1;
        }

        if *usage_count == 0 {
            channels_usage.remove(&channel.info.id);
            let _ = self.p2p().hosts().unregister(channel.address());
            channel.stop().await;
            return true
        }

        false
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

struct ChannelTask {
    session: Weak<DirectSession>,
    addr: Url,
    output: Arc<AsyncMutex<Option<Result<ChannelPtr>>>>,
}

impl Drop for ChannelTask {
    fn drop(&mut self) {
        let session = self.session.upgrade().unwrap();
        let addr = self.addr.clone();
        session
            .p2p()
            .executor()
            .spawn(async move {
                let mut tasks = session.tasks.lock().await;
                tasks.remove(&addr);
            })
            .detach();
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
            dnetev!(self, DirectPeerDiscovery, {
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
            let active_profiles = settings.active_profiles.clone();
            let seeds = settings.seeds.clone();
            drop(settings);

            current_attempt += 1;

            if current_attempt >= 4 {
                verbose!(
                    target: "net::direct_session::peer_discovery",
                    "[P2P] [PEER DISCOVERY] Sleeping and trying again. Attempt {current_attempt}"
                );

                dnetev!(self, DirectPeerDiscovery, {
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
                dnetev!(self, DirectPeerDiscovery, {
                    attempt: current_attempt,
                    state: "newchan",
                });

                for color in [HostColor::Gold, HostColor::White, HostColor::Grey].iter() {
                    if let Some((entry, _)) = self
                        .p2p()
                        .hosts()
                        .container
                        .fetch_random_with_schemes(color.clone(), &active_profiles)
                    {
                        channel = self.p2p().session_direct().get_channel(&entry.0).await.ok();
                        break;
                    }
                }
            }

            // First 2 times try sending GetAddr to the network.
            // 3rd time do a seed sync (providing we have seeds configured).
            if self.p2p().is_connected() && current_attempt <= 2 {
                // Broadcast the GetAddrs message to all active peers.
                // If we have no active peers, we will perform a SeedSyncSession instead.
                verbose!(
                    target: "net::direct_session::peer_discovery",
                    "[P2P] [PEER DISCOVERY] Asking peers for new peers to connect to...");

                dnetev!(self, DirectPeerDiscovery, {
                    attempt: current_attempt,
                    state: "getaddr",
                });

                let get_addrs =
                    GetAddrsMessage { max: getaddrs_max.unwrap_or(1), transports: active_profiles };

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
                        verbose!(
                            target: "net::direct_session::peer_discovery",
                            "[P2P] [PEER DISCOVERY] Discovered {addrs_len} peers"
                        );
                        // Found some addrs, reset `current_attempt`
                        if addrs_len > 0 {
                            current_attempt = 0;
                        }
                    }
                    Err(_) => {
                        warn!(
                            target: "net::direct_session::peer_discovery",
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
                verbose!(
                    target: "net::direct_session::peer_discovery",
                    "[P2P] [PEER DISCOVERY] Asking seeds for new peers to connect to...");

                dnetev!(self, DirectPeerDiscovery, {
                    attempt: current_attempt,
                    state: "seed",
                });

                self.p2p().seed().await;
            }

            // Stop the channel we created for peer discovery
            if let Some(ch) = channel {
                self.p2p().session_direct().cleanup_channel(ch).await;
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
