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

use std::time::Duration;

use async_std::{future::timeout, sync::Arc};
use futures::future::join_all;
use log::{debug, error};
use smol::Executor;

use super::super::{
    channel::ChannelPtr,
    hosts::HostsPtr,
    message::{VerackMessage, VersionMessage},
    message_subscriber::MessageSubscription,
    settings::SettingsPtr,
};
use crate::{Error, Result};

/// Implements the protocol version handshake sent out by nodes at
/// the beginning of a connection.
pub struct ProtocolVersion {
    channel: ChannelPtr,
    version_sub: MessageSubscription<VersionMessage>,
    verack_sub: MessageSubscription<VerackMessage>,
    settings: SettingsPtr,
    hosts: HostsPtr,
}

impl ProtocolVersion {
    /// Create a new version protocol. Makes a version and version ack
    /// subscription, then adds them to a version protocol instance.
    pub async fn new(channel: ChannelPtr, settings: SettingsPtr, hosts: HostsPtr) -> Arc<Self> {
        // Creates a versi5on subscription
        let version_sub =
            channel.subscribe_msg::<VersionMessage>().await.expect("Missing version dispatcher!");

        // Creates a version acknowledgement subscription
        let verack_sub =
            channel.subscribe_msg::<VerackMessage>().await.expect("Missing verack dispatcher!");

        Arc::new(Self { channel, version_sub, verack_sub, settings, hosts })
    }

    /// Start version information exchange. Start the timer. Send version
    /// info and wait for version ack. Wait for version info and send
    /// version ack.
    pub async fn run(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        debug!(target: "net::protocol_version::run()", "START => address={}", self.channel.address());
        // Start timer
        // Send version, wait for verack
        // Wait for version, send verack
        // Fin.
        let result = timeout(
            Duration::from_secs(self.settings.channel_handshake_timeout),
            self.clone().exchange_versions(executor),
        )
        .await;

        if let Err(e) = result {
            error!(
                target: "net::protocol_version::run()",
                "[P2P] Version Exchange failed [{}]: {}",
                self.channel.address(), e,
            );

            // Remove from hosts
            self.hosts.remove(self.channel.address()).await;
            self.channel.stop().await;
            return Err(Error::ChannelTimeout)
        }

        debug!(target: "net::protocol_version::run()", "END => address={}", self.channel.address());
        Ok(())
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

        let version = VersionMessage { node_id: self.settings.node_id.clone() };
        self.channel.send(&version).await?;

        // Wait for verack
        let verack_msg = self.verack_sub.receive().await?;

        // Validate peer received version against our version.
        debug!(
            target: "net::protocol_version::send_version()",
            "App version: {}, Recv version: {}",
            self.settings.app_version, verack_msg.app_version,
        );

        // MAJOR and MINOR should be the same.
        if self.settings.app_version.major != verack_msg.app_version.major ||
            self.settings.app_version.minor != verack_msg.app_version.minor
        {
            error!(
                target: "net::protocol_version::send_version()",
                "[P2P] Version mismatch from {}. Disconnecting...",
                self.channel.address(),
            );

            self.hosts.remove(self.channel.address()).await;
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
        let _version = self.version_sub.receive().await?;
        // TODO: self.channel.set_remote_node_id(version.node_id.clone()).await;

        // Send verack
        let verack = VerackMessage { app_version: self.settings.app_version.clone() };
        self.channel.send(&verack).await?;

        debug!(
            target: "net::protocol_version::recv_version()",
            "END => address={}", self.channel.address(),
        );
        Ok(())
    }
}
