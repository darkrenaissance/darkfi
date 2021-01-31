use futures::FutureExt;
use log::*;
use smol::Executor;
use std::sync::Arc;

use crate::net::error::{NetError, NetResult};
use crate::net::messages;
use crate::net::utility::sleep;
use crate::net::{ChannelPtr, SettingsPtr};

pub struct ProtocolVersion {
    channel: ChannelPtr,
    settings: SettingsPtr,
}

impl ProtocolVersion {
    pub fn new(channel: ChannelPtr, settings: SettingsPtr) -> Arc<Self> {
        Arc::new(Self { channel, settings })
    }

    pub async fn run(self: Arc<Self>, executor: Arc<Executor<'_>>) -> NetResult<()> {
        debug!(target: "net", "ProtocolVersion::run() [START]");
        // Start timer
        // Send version, wait for verack
        // Wait for version, send verack
        // Fin.
        let result = futures::select! {
            _ = self.clone().exchange_versions(executor).fuse() => Ok(()),
            _ = sleep(self.settings.channel_handshake_seconds).fuse() => Err(NetError::ChannelTimeout)
        };
        debug!(target: "net", "ProtocolVersion::run() [END]");
        result
    }

    async fn exchange_versions(self: Arc<Self>, executor: Arc<Executor<'_>>) -> NetResult<()> {
        debug!(target: "net", "ProtocolVersion::exchange_versions() [START]");
        let send = executor.spawn(self.clone().send_version());
        let recv = executor.spawn(self.recv_version());

        send.await.and(recv.await)?;
        debug!(target: "net", "ProtocolVersion::exchange_versions() [END]");
        Ok(())
    }

    async fn send_version(self: Arc<Self>) -> NetResult<()> {
        debug!(target: "net", "ProtocolVersion::send_version() [START]");
        let verack_sub = self
            .channel
            .clone()
            .subscribe_msg(messages::PacketType::Verack)
            .await;

        let version = messages::Message::Version(messages::VersionMessage {});
        self.channel.clone().send(version).await?;

        // Wait for version acknowledgement
        let _verack_msg = verack_sub.receive().await?;

        debug!(target: "net", "ProtocolVersion::send_version() [END]");
        Ok(())
    }

    async fn recv_version(self: Arc<Self>) -> NetResult<()> {
        debug!(target: "net", "ProtocolVersion::recv_version() [START]");
        let version_sub = self
            .channel
            .clone()
            .subscribe_msg(messages::PacketType::Version)
            .await;

        let _version_msg = version_sub.receive().await?;

        // Check the message is OK

        // Send version acknowledgement
        let verack = messages::Message::Verack(messages::VerackMessage {});
        self.channel.clone().send(verack).await?;

        debug!(target: "net", "ProtocolVersion::recv_version() [END]");
        Ok(())
    }
}
