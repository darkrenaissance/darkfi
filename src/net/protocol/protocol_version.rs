/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2022 Dyne.org foundation
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

use async_std::future::timeout;
use std::{sync::Arc, time::Duration};

use log::*;
use smol::Executor;

use crate::{Error, Result};

use super::super::{
    message, message_subscriber::MessageSubscription, ChannelPtr, HostsPtr, SettingsPtr,
};

/// Implements the protocol version handshake sent out by nodes at the beginning
/// of a connection.
pub struct ProtocolVersion {
    channel: ChannelPtr,
    version_sub: MessageSubscription<message::VersionMessage>,
    verack_sub: MessageSubscription<message::VerackMessage>,
    settings: SettingsPtr,
    hosts: HostsPtr,
}

impl ProtocolVersion {
    /// Create a new version protocol. Makes a version and version
    /// acknowledgement subscription, then adds them to a version protocol
    /// instance.
    pub async fn new(channel: ChannelPtr, settings: SettingsPtr, hosts: HostsPtr) -> Arc<Self> {
        // Creates a version subscription.
        let version_sub = channel
            .clone()
            .subscribe_msg::<message::VersionMessage>()
            .await
            .expect("Missing version dispatcher!");

        // Creates a version acknowledgement subscription.
        let verack_sub = channel
            .clone()
            .subscribe_msg::<message::VerackMessage>()
            .await
            .expect("Missing verack dispatcher!");

        Arc::new(Self { channel, version_sub, verack_sub, settings, hosts })
    }

    /// Start version information exchange. Start the timer. Send version info
    /// and wait for version acknowledgement. Wait for version info and send
    /// version acknowledgement.
    pub async fn run(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        debug!(target: "net", "ProtocolVersion::run() [START]");
        // Start timer
        // Send version, wait for verack
        // Wait for version, send verack
        // Fin.
        let result = timeout(
            Duration::from_secs(self.settings.channel_handshake_seconds.into()),
            self.clone().exchange_versions(executor),
        )
        .await;

        if let Err(_e) = result {
            return Err(Error::ChannelTimeout)
        }

        debug!(target: "net", "ProtocolVersion::run() [END]");
        Ok(())
    }

    /// Send and recieve version information.
    async fn exchange_versions(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        debug!(target: "net", "ProtocolVersion::exchange_versions() [START]");

        let send = executor.spawn(self.clone().send_version());
        let recv = executor.spawn(self.recv_version());

        send.await?;
        recv.await?;

        debug!(target: "net", "ProtocolVersion::exchange_versions() [END]");
        Ok(())
    }

    /// Send version info and wait for version acknowledgement
    /// and ensures the app version is the same, if configured.
    async fn send_version(self: Arc<Self>) -> Result<()> {
        debug!(target: "net", "ProtocolVersion::send_version() [START]");

        let version = message::VersionMessage { node_id: self.settings.node_id.clone() };

        self.channel.clone().send(version).await?;

        // Wait for version acknowledgement
        let verack_msg = self.verack_sub.receive().await?;

        // Validate peer received version against our version, if configured.
        // Seeds version gets ignored.
        if !self.settings.seeds.contains(&self.channel.address()) {
            match &self.settings.app_version {
                Some(app_version) => {
                    debug!(target: "net", "ProtocolVersion::send_version() [App version: {}]", app_version);
                    debug!(target: "net", "ProtocolVersion::send_version() [Recieved version: {}]", verack_msg.app);
                    // Version format: MAJOR.MINOR.PATCH
                    let app_versions: Vec<&str> = app_version.split('.').collect();
                    let verack_msg_versions: Vec<&str> = verack_msg.app.split('.').collect();
                    // Check for malformed versions
                    if app_versions.len() != 3 || verack_msg_versions.len() != 3 {
                        error!("ProtocolVersion::send_version() [Malformed version detected. Disconnecting from channel.]");
                        self.hosts.remove(&self.channel.address()).await;
                        self.channel.stop().await;
                        return Err(Error::ChannelStopped)
                    }
                    // Ignore PATCH version
                    if app_versions[0] != verack_msg_versions[0] ||
                        app_versions[1] != verack_msg_versions[1]
                    {
                        error!(
                            "ProtocolVersion::send_version() [Wrong app version from ({}). Disconnecting from channel.]",
                            self.channel.address()
                        );
                        self.hosts.remove(&self.channel.address()).await;
                        self.channel.stop().await;
                        return Err(Error::ChannelStopped)
                    }
                }
                None => {
                    debug!(target: "net", "ProtocolVersion::send_version() [App version not set, ignorring received]")
                }
            }
        }

        debug!(target: "net", "ProtocolVersion::send_version() [END]");
        Ok(())
    }

    /// Recieve version info, check the message is okay and send version
    /// acknowledgement with app version attached.
    async fn recv_version(self: Arc<Self>) -> Result<()> {
        debug!(target: "net", "ProtocolVersion::recv_version() [START]");
        // Receive version message
        let version = self.version_sub.receive().await?;
        self.channel.set_remote_node_id(version.node_id.clone()).await;

        // Send version acknowledgement
        let verack = message::VerackMessage {
            app: self.settings.app_version.clone().unwrap_or_default(),
        };
        self.channel.clone().send(verack).await?;

        debug!(target: "net", "ProtocolVersion::recv_version() [END]");
        Ok(())
    }
}
