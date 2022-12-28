/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
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

use std::fmt;

use async_std::sync::{Arc, Mutex, Weak};
use async_trait::async_trait;
use log::{debug, error, info, warn};
use rand::seq::SliceRandom;
use serde_json::{json, Value};
use smol::Executor;
use url::Url;

use crate::{
    net::{message, transport::TransportName},
    system::{StoppableTask, StoppableTaskPtr, Subscriber, SubscriberPtr, Subscription},
    util::async_util,
    Error, Result,
};

use super::{
    super::{ChannelPtr, Connector, P2p},
    Session, SessionBitflag, SESSION_OUTBOUND,
};

#[derive(Clone)]
enum OutboundState {
    Open,
    Pending,
    Connected,
}

impl fmt::Display for OutboundState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Open => "open",
                Self::Pending => "pending",
                Self::Connected => "connected",
            }
        )
    }
}

#[derive(Clone)]
struct OutboundInfo {
    addr: Option<Url>,
    channel: Option<ChannelPtr>,
    state: OutboundState,
}

impl OutboundInfo {
    async fn get_info(&self) -> serde_json::Value {
        let addr = match self.addr.as_ref() {
            Some(addr) => serde_json::Value::String(addr.to_string()),
            None => serde_json::Value::Null,
        };

        let channel = match &self.channel {
            Some(channel) => channel.get_info().await,
            None => serde_json::Value::Null,
        };

        json!({
            "addr": addr,
            "state": self.state.to_string(),
            "channel": channel,
        })
    }
}

impl Default for OutboundInfo {
    fn default() -> Self {
        Self { addr: None, channel: None, state: OutboundState::Open }
    }
}

/// Defines outbound connections session.
pub struct OutboundSession {
    p2p: Weak<P2p>,
    connect_slots: Mutex<Vec<StoppableTaskPtr>>,
    slot_info: Mutex<Vec<OutboundInfo>>,
    /// Subscriber used to signal channels processing
    channel_subscriber: SubscriberPtr<Result<ChannelPtr>>,
    /// Flag to toggle channel_subscriber notifications
    notify: Mutex<bool>,
}

impl OutboundSession {
    /// Create a new outbound session.
    pub fn new(p2p: Weak<P2p>) -> Arc<Self> {
        Arc::new(Self {
            p2p,
            connect_slots: Mutex::new(Vec::new()),
            slot_info: Mutex::new(Vec::new()),
            channel_subscriber: Subscriber::new(),
            notify: Mutex::new(false),
        })
    }

    /// Start the outbound session. Runs the channel connect loop.
    pub async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        let slots_count = self.p2p().settings().outbound_connections;
        info!(target: "net", "Starting {} outbound connection slots.", slots_count);
        // Activate mutex lock on connection slots.
        let mut connect_slots = self.connect_slots.lock().await;

        self.slot_info.lock().await.resize(slots_count as usize, Default::default());

        for i in 0..slots_count {
            let task = StoppableTask::new();

            task.clone().start(
                self.clone().channel_connect_loop(i, executor.clone()),
                // Ignore stop handler
                |_| async {},
                Error::NetworkServiceStopped,
                executor.clone(),
            );

            connect_slots.push(task);
        }

        Ok(())
    }

    /// Stop the outbound session.
    pub async fn stop(&self) {
        let connect_slots = &*self.connect_slots.lock().await;

        for slot in connect_slots {
            slot.stop().await;
        }
    }

    /// Creates a connector object and tries to connect using it.
    pub async fn channel_connect_loop(
        self: Arc<Self>,
        slot_number: u32,
        executor: Arc<Executor<'_>>,
    ) -> Result<()> {
        let parent = Arc::downgrade(&self);

        let connector = Connector::new(self.p2p().settings(), Arc::new(parent));

        // Retrieve preferent outbound transports
        let outbound_transports = &self.p2p().settings().outbound_transports;

        loop {
            match self
                .try_connect(slot_number, executor.clone(), &connector, outbound_transports)
                .await
            {
                Ok(_) => info!(target: "net", "#{} slot disconnected", slot_number),
                Err(err) => {
                    error!(target: "net", "#{} slot connection failed: {}", slot_number, err)
                }
            }

            async_util::sleep(self.p2p().settings().outbound_retry_seconds).await;
        }
    }

    /// Start making an outbound connection, using provided connector.
    /// Loads a valid address then tries to connect. Once connected,
    /// registers the channel, removes it from the list of pending channels,
    /// and starts sending messages across the channel, otherwise returns a network error.
    async fn try_connect(
        &self,
        slot_number: u32,
        executor: Arc<Executor<'_>>,
        connector: &Connector,
        outbound_transports: &Vec<TransportName>,
    ) -> Result<()> {
        let addr = self.load_address(slot_number).await?;
        info!(target: "net", "#{} processing outbound [{}]", slot_number, addr);
        {
            let info = &mut self.slot_info.lock().await[slot_number as usize];
            info.addr = Some(addr.clone());
            info.state = OutboundState::Pending;
        }

        // Check that addr transport is in configured outbound transport
        let addr_transport = TransportName::try_from(addr.clone())?;
        let transports = if outbound_transports.contains(&addr_transport) {
            vec![addr_transport]
        } else {
            warn!(target: "net", "#{} address {} transport is not in accepted outbound transports, will try with: {:?}", slot_number, addr, outbound_transports);
            outbound_transports.clone()
        };

        for transport in transports {
            // Replace addr transport
            let mut transport_addr = addr.clone();
            transport_addr.set_scheme(&transport.to_scheme())?;
            info!(target: "net", "#{} connecting to outbound [{}]", slot_number, transport_addr);
            match connector.connect(transport_addr.clone()).await {
                Ok(channel) => {
                    // Blacklist goes here
                    info!(target: "net", "#{} connected to outbound [{}]", slot_number, transport_addr);

                    let stop_sub = channel.subscribe_stop().await;
                    if stop_sub.is_err() {
                        continue
                    }

                    self.register_channel(channel.clone(), executor.clone()).await?;

                    // Channel is now connected but not yet setup

                    // Remove pending lock since register_channel will add the channel to p2p
                    self.p2p().remove_pending(&addr).await;
                    {
                        let info = &mut self.slot_info.lock().await[slot_number as usize];
                        info.channel = Some(channel.clone());
                        info.state = OutboundState::Connected;
                    }

                    // Notify that channel processing has been finished
                    if *self.notify.lock().await {
                        self.channel_subscriber.notify(Ok(channel)).await;
                    }

                    // Wait for channel to close
                    stop_sub.unwrap().receive().await;

                    return Ok(())
                }
                Err(err) => {
                    error!(target: "net", "Unable to connect to outbound [{}]: {}", &transport_addr, err);
                }
            }
        }

        // Remove url from hosts
        self.p2p().hosts().remove(&addr).await;

        {
            let info = &mut self.slot_info.lock().await[slot_number as usize];
            info.addr = None;
            info.channel = None;
            info.state = OutboundState::Open;
        }

        // Notify that channel processing has been finished (failed)
        if *self.notify.lock().await {
            self.channel_subscriber.notify(Err(Error::ConnectFailed)).await;
        }

        Err(Error::ConnectFailed)
    }

    /// Loops through host addresses to find a outbound address that we can
    /// connect to. Checks whether address is valid by making sure it isn't
    /// our own inbound address, then checks whether it is already connected
    /// (exists) or connecting (pending). If no address was found, we try to
    /// to discover new peers. Keeps looping until address is found that passes all checks.
    async fn load_address(&self, slot_number: u32) -> Result<Url> {
        loop {
            let p2p = self.p2p();
            let self_inbound_addr = p2p.settings().external_addr.clone();

            let mut addrs;

            {
                let hosts = p2p.hosts().load_all().await;
                addrs = hosts;
            }

            addrs.shuffle(&mut rand::thread_rng());

            for addr in addrs {
                if p2p.exists(&addr).await? {
                    continue
                }

                // Check if address is in peers list
                if p2p.settings().peers.contains(&addr) {
                    continue
                }

                // Obtain a lock on this address to prevent duplicate connections
                if !p2p.add_pending(addr.clone()).await {
                    continue
                }

                if self_inbound_addr.contains(&addr) {
                    continue
                }

                return Ok(addr)
            }

            // Peer discovery
            if p2p.settings().peer_discovery {
                debug!(target: "net", "#{} No available address found, entering peer discovery mode.", slot_number);
                self.peer_discovery(slot_number).await?;
                debug!(target: "net", "#{} Discovery mode ended.", slot_number);
            }

            // Sleep and then retry
            debug!(target: "net", "Retrying connect slot #{}", slot_number);
            async_util::sleep(p2p.settings().outbound_retry_seconds).await;
        }
    }

    /// Try to find new peers to update available hosts.
    async fn peer_discovery(&self, slot_number: u32) -> Result<()> {
        // Check that another slot(thread) already tries to update hosts
        let p2p = self.p2p();
        if !p2p.clone().start_discovery().await {
            debug!(target: "net", "#{} P2P already on discovery mode.", slot_number);
            return Ok(())
        }

        debug!(target: "net", "#{} Discovery mode started.", slot_number);

        // Getting a random connected channel to ask for peers
        let channel = match p2p.clone().random_channel().await {
            Some(c) => c,
            None => {
                debug!(target: "net", "#{} No peers found.", slot_number);
                p2p.clone().stop_discovery().await;
                return Ok(())
            }
        };

        // Ask peer
        debug!(target: "net", "#{} Asking peer: {}", slot_number, channel.address());
        let get_addr_msg = message::GetAddrsMessage {};
        channel.send(get_addr_msg).await?;

        p2p.stop_discovery().await;

        Ok(())
    }

    /// Subscribe to a channel.
    pub async fn subscribe_channel(&self) -> Subscription<Result<ChannelPtr>> {
        self.channel_subscriber.clone().subscribe().await
    }

    /// Enable channel_subscriber notifications.
    pub async fn enable_notify(self: Arc<Self>) {
        *self.notify.lock().await = true;
    }

    /// Disable channel_subscriber notifications.
    pub async fn disable_notify(self: Arc<Self>) {
        *self.notify.lock().await = false;
    }
}

#[async_trait]
impl Session for OutboundSession {
    async fn get_info(&self) -> serde_json::Value {
        let mut slots = Vec::new();
        for info in &*self.slot_info.lock().await {
            slots.push(info.get_info().await);
        }

        let hosts = self.p2p().hosts().load_all().await;
        let addrs: Vec<Value> =
            hosts.iter().map(|addr| serde_json::Value::String(addr.to_string())).collect();

        json!({
            "slots": slots,
            "hosts": serde_json::Value::Array(addrs),
        })
    }

    fn p2p(&self) -> Arc<P2p> {
        self.p2p.upgrade().unwrap()
    }

    fn type_id(&self) -> SessionBitflag {
        SESSION_OUTBOUND
    }
}
