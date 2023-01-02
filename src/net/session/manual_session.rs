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

use async_std::sync::{Arc, Mutex, Weak};

use async_trait::async_trait;
use log::{info, warn};
use serde_json::json;
use smol::Executor;
use url::Url;

use crate::{
    net::transport::TransportName,
    system::{StoppableTask, StoppableTaskPtr, Subscriber, SubscriberPtr, Subscription},
    util::async_util::sleep,
    Error, Result,
};

use super::{
    super::{ChannelPtr, Connector, P2p},
    Session, SessionBitflag, SESSION_MANUAL,
};

pub struct ManualSession {
    p2p: Weak<P2p>,
    connect_slots: Mutex<Vec<StoppableTaskPtr>>,
    /// Subscriber used to signal channels processing
    channel_subscriber: SubscriberPtr<Result<ChannelPtr>>,
    /// Flag to toggle channel_subscriber notifications
    notify: Mutex<bool>,
}

impl ManualSession {
    /// Create a new inbound session.
    pub fn new(p2p: Weak<P2p>) -> Arc<Self> {
        Arc::new(Self {
            p2p,
            connect_slots: Mutex::new(Vec::new()),
            channel_subscriber: Subscriber::new(),
            notify: Mutex::new(false),
        })
    }

    /// Stop the outbound session.
    pub async fn stop(&self) {
        let connect_slots = &*self.connect_slots.lock().await;

        for slot in connect_slots {
            slot.stop().await;
        }
    }

    pub async fn connect(self: Arc<Self>, addr: &Url, executor: Arc<Executor<'_>>) {
        let task = StoppableTask::new();

        task.clone().start(
            self.clone().channel_connect_loop(addr.clone(), executor.clone()),
            // Ignore stop handler
            |_| async {},
            Error::NetworkServiceStopped,
            executor.clone(),
        );

        self.connect_slots.lock().await.push(task);
    }

    pub async fn channel_connect_loop(
        self: Arc<Self>,
        addr: Url,
        executor: Arc<Executor<'_>>,
    ) -> Result<()> {
        let parent = Arc::downgrade(&self);

        let settings = self.p2p().settings();

        let connector = Connector::new(settings.clone(), Arc::new(parent));

        let attempts = settings.manual_attempt_limit;
        let mut remaining = attempts;

        // Retrieve preferent outbound transports
        let outbound_transports = &settings.outbound_transports;

        // Check that addr transport is in configured outbound transport
        let addr_transport = TransportName::try_from(addr.clone())?;
        let transports = if outbound_transports.contains(&addr_transport) {
            vec![addr_transport]
        } else {
            warn!(target: "net::manual_session", "Manual outbound address {} transport is not in accepted outbound transports, will try with: {:?}", addr, outbound_transports);
            outbound_transports.clone()
        };

        loop {
            // Loop forever if attempts is 0
            // Otherwise loop attempts number of times
            remaining = if attempts == 0 { 1 } else { remaining - 1 };
            if remaining == 0 {
                break
            }

            self.p2p().add_pending(addr.clone()).await;

            for transport in &transports {
                // Replace addr transport
                let mut transport_addr = addr.clone();
                transport_addr.set_scheme(&transport.to_scheme())?;
                info!(target: "net::manual_session", "Connecting to manual outbound [{}]", transport_addr);
                match connector.connect(transport_addr.clone()).await {
                    Ok(channel) => {
                        // Blacklist goes here
                        info!(target: "net::manual_session", "Connected to manual outbound [{}]", transport_addr);

                        let stop_sub = channel.subscribe_stop().await;
                        if stop_sub.is_err() {
                            continue
                        }

                        self.clone().register_channel(channel.clone(), executor.clone()).await?;

                        // Channel is now connected but not yet setup

                        // Remove pending lock since register_channel will add the channel to p2p
                        self.p2p().remove_pending(&addr).await;

                        //self.clone().attach_protocols(channel, executor.clone()).await?;

                        // Notify that channel processing has been finished
                        if *self.notify.lock().await {
                            self.channel_subscriber.notify(Ok(channel)).await;
                        }

                        // Wait for channel to close
                        stop_sub.unwrap().receive().await;
                    }
                    Err(err) => {
                        info!(target: "net::manual_session", "Unable to connect to manual outbound [{}]: {}", addr, err);
                    }
                }
            }

            // Notify that channel processing has been finished (failed)
            if *self.notify.lock().await {
                self.channel_subscriber.notify(Err(Error::ConnectFailed)).await;
            }

            sleep(settings.connect_timeout_seconds.into()).await;
        }

        warn!(
        target: "net::manual_session",
        "Suspending manual connection to [{}] after {} failed attempts.",
        &addr,
        attempts
        );

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

    // Starts sending keep-alive and address messages across the channels.
    /*async fn attach_protocols(
    self: Arc<Self>,
    channel: ChannelPtr,
    executor: Arc<Executor<'_>>,
    ) -> Result<()> {
    let hosts = self.p2p().hosts();

    let protocol_ping = ProtocolPing::new(channel.clone(), self.p2p());
    let protocol_addr = ProtocolAddress::new(channel, hosts).await;

    protocol_ping.start(executor.clone()).await;
    protocol_addr.start(executor).await;

    Ok(())
    }*/
}

#[async_trait]
impl Session for ManualSession {
    async fn get_info(&self) -> serde_json::Value {
        json!({
            "key": 110
        })
    }

    fn p2p(&self) -> Arc<P2p> {
        self.p2p.upgrade().unwrap()
    }

    fn type_id(&self) -> SessionBitflag {
        SESSION_MANUAL
    }
}
