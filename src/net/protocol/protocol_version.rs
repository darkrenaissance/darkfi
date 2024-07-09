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

use std::{
    sync::Arc,
    time::{Duration, UNIX_EPOCH},
};

use futures::{
    future::{join_all, select, Either},
    pin_mut,
};
use log::{debug, error};
use smol::{lock::RwLock as AsyncRwLock, Executor, Timer};

use super::super::{
    channel::ChannelPtr,
    message::{VerackMessage, VersionMessage},
    message_publisher::MessageSubscription,
    settings::Settings,
};
use crate::{Error, Result};

/// Implements the protocol version handshake sent out by nodes at
/// the beginning of a connection.
pub struct ProtocolVersion {
    channel: ChannelPtr,
    version_sub: MessageSubscription<VersionMessage>,
    verack_sub: MessageSubscription<VerackMessage>,
    settings: Arc<AsyncRwLock<Settings>>,
}

impl ProtocolVersion {
    /// Create a new version protocol. Makes a version and version ack
    /// subscription, then adds them to a version protocol instance.
    // TODO: This function takes settings as a param, however, it is also reachable through Channel.
    //       Maybe we want to navigate towards Settings through channel->session->p2p->settings
    pub async fn new(channel: ChannelPtr, settings: Arc<AsyncRwLock<Settings>>) -> Arc<Self> {
        // Creates a version subscription
        let version_sub =
            channel.subscribe_msg::<VersionMessage>().await.expect("Missing version dispatcher!");

        // Creates a version acknowledgement subscription
        let verack_sub =
            channel.subscribe_msg::<VerackMessage>().await.expect("Missing verack dispatcher!");

        Arc::new(Self { channel, version_sub, verack_sub, settings })
    }

    /// Start version information exchange. Start the timer. Send version
    /// info and wait for version ack. Wait for version info and send
    /// version ack.
    pub async fn run(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        debug!(target: "net::protocol_version::run()", "START => address={}", self.channel.address());
        let timeout =
            Timer::after(Duration::from_secs(self.settings.read().await.channel_handshake_timeout));
        let version = self.clone().exchange_versions(executor);

        pin_mut!(timeout);
        pin_mut!(version);

        // Run timer and version exchange at the same time. Either deal
        // with the success or failure of the version exchange or
        // time out.
        match select(version, timeout).await {
            Either::Left((Ok(_), _)) => {
                debug!(target: "net::protocol_version::run()", "END => address={}",
                self.channel.address());

                Ok(())
            }
            Either::Left((Err(e), _)) => {
                error!(
                    target: "net::protocol_version::run()",
                    "[P2P] Version Exchange failed [{}]: {}",
                    self.channel.address(), e,
                );

                self.channel.stop().await;
                Err(e)
            }

            Either::Right((_, _)) => {
                error!(
                    target: "net::protocol_version::run()",
                    "[P2P] Version Exchange timed out [{}]",
                    self.channel.address(),
                );

                self.channel.stop().await;
                Err(Error::ChannelTimeout)
            }
        }
    }

    /// Send and receive version information
    async fn exchange_versions(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        debug!(
            target: "net::protocol_version::exchange_versions()",
            "START => address={}", self.channel.address(),
        );

        let send = executor.spawn(self.clone().send_version());
        let recv = executor.spawn(self.clone().recv_version());

        let rets = join_all(vec![send, recv]).await;
        if let Err(e) = &rets[0] {
            error!(
                target: "net::protocol_version::exchange_versions()",
                "send_version() failed: {}", e,
            );
            return Err(e.clone())
        }

        if let Err(e) = &rets[1] {
            error!(
                target: "net::protocol_version::exchange_versions()",
                "recv_version() failed: {}", e,
            );
            return Err(e.clone())
        }

        debug!(
            target: "net::protocol_version::exchange_versions()",
            "END => address={}", self.channel.address(),
        );
        Ok(())
    }

    /// Send version info and wait for version acknowledgement.
    /// Ensures that the app version is the same.
    async fn send_version(self: Arc<Self>) -> Result<()> {
        debug!(
            target: "net::protocol_version::send_version()",
            "START => address={}", self.channel.address(),
        );

        let settings = self.settings.read().await;
        let node_id = settings.node_id.clone();
        let app_version = settings.app_version.clone();
        let external_addrs = settings.external_addrs.clone();
        drop(settings);

        let version = VersionMessage {
            node_id,
            version: app_version.clone(),
            timestamp: UNIX_EPOCH.elapsed().unwrap().as_secs(),
            connect_recv_addr: self.channel.connect_addr().clone(),
            resolve_recv_addr: self.channel.resolve_addr().clone(),
            ext_send_addr: external_addrs,
            /* NOTE: `features` is a list of enabled features in the
            format Vec<(service, version)>. In the future, Protocols will
            add their own data to this field when they are attached.*/
            features: vec![],
        };
        self.channel.send(&version).await?;

        // Wait for verack
        let verack_msg = self.verack_sub.receive().await?;

        // Validate peer received version against our version.
        debug!(
            target: "net::protocol_version::send_version()",
            "App version: {}, Recv version: {}",
            app_version, verack_msg.app_version,
        );

        // MAJOR and MINOR should be the same.
        if app_version.major != verack_msg.app_version.major ||
            app_version.minor != verack_msg.app_version.minor
        {
            error!(
                target: "net::protocol_version::send_version()",
                "[P2P] Version mismatch from {}. Disconnecting...",
                self.channel.address(),
            );

            self.channel.stop().await;
            return Err(Error::ChannelStopped)
        }

        // Versions are compatible
        debug!(
            target: "net::protocol_version::send_version()",
            "END => address={}", self.channel.address(),
        );
        Ok(())
    }

    /// Receive version info, check the message is okay and send verack
    /// with app version attached.
    async fn recv_version(self: Arc<Self>) -> Result<()> {
        debug!(
            target: "net::protocol_version::recv_version()",
            "START => address={}", self.channel.address(),
        );

        // Receive version message
        let version = self.version_sub.receive().await?;
        self.channel.set_version(version).await;

        // Send verack
        let verack = VerackMessage { app_version: self.settings.read().await.app_version.clone() };
        self.channel.send(&verack).await?;

        debug!(
            target: "net::protocol_version::recv_version()",
            "END => address={}", self.channel.address(),
        );
        Ok(())
    }
}
