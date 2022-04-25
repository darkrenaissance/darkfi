use async_std::future::timeout;
use std::{sync::Arc, time::Duration};

use log::*;
use smol::Executor;

use crate::{Error, Result};

use super::super::{message, message_subscriber::MessageSubscription, ChannelPtr, SettingsPtr};

/// Implements the protocol version handshake sent out by nodes at the beginning
/// of a connection.
pub struct ProtocolVersion {
    channel: ChannelPtr,
    version_sub: MessageSubscription<message::VersionMessage>,
    verack_sub: MessageSubscription<message::VerackMessage>,
    settings: SettingsPtr,
}

impl ProtocolVersion {
    /// Create a new version protocol. Makes a version and version
    /// acknowledgement subscription, then adds them to a version protocol
    /// instance.
    pub async fn new(channel: ChannelPtr, settings: SettingsPtr) -> Arc<Self> {
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

        Arc::new(Self { channel, version_sub, verack_sub, settings })
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
        let result = match timeout(
            Duration::from_secs(self.settings.channel_handshake_seconds.into()),
            self.clone().exchange_versions(executor),
        )
        .await
        {
            Ok(t) => t,
            Err(_) => Err(Error::ChannelTimeout),
        };
        debug!(target: "net", "ProtocolVersion::run() [END]");
        result
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
    /// Send version info and wait for version acknowledgement.
    async fn send_version(self: Arc<Self>) -> Result<()> {
        debug!(target: "net", "ProtocolVersion::send_version() [START]");
        let version = message::VersionMessage {};
        self.channel.clone().send(version).await?;

        // Wait for version acknowledgement
        let _verack_msg = self.verack_sub.receive().await?;

        debug!(target: "net", "ProtocolVersion::send_version() [END]");
        Ok(())
    }
    /// Recieve version info, check the message is okay and send version
    /// acknowledgement.
    async fn recv_version(self: Arc<Self>) -> Result<()> {
        debug!(target: "net", "ProtocolVersion::recv_version() [START]");
        // Rec
        let _version_msg = self.version_sub.receive().await?;

        // Check the message is OK

        // Send version acknowledgement
        let verack = message::VerackMessage {};
        self.channel.clone().send(verack).await?;

        debug!(target: "net", "ProtocolVersion::recv_version() [END]");
        Ok(())
    }
}
