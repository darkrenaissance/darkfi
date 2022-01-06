use futures::FutureExt;
use log::*;
use smol::Executor;
use std::sync::Arc;

use crate::{
    error::{Error, Result},
    net::{message_subscriber::MessageSubscription, messages, ChannelPtr, SettingsPtr},
    util::sleep,
};

/// Implements the protocol version handshake sent out by nodes at the beginning
/// of a connection.
pub struct ProtocolVersion {
    channel: ChannelPtr,
    version_sub: MessageSubscription<messages::VersionMessage>,
    verack_sub: MessageSubscription<messages::VerackMessage>,
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
            .subscribe_msg::<messages::VersionMessage>()
            .await
            .expect("Missing version dispatcher!");

        // Creates a version acknowledgement subscription.
        let verack_sub = channel
            .clone()
            .subscribe_msg::<messages::VerackMessage>()
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
        let result = futures::select! {
            _ = self.clone().exchange_versions(executor).fuse() => Ok(()),
            _ = sleep(self.settings.channel_handshake_seconds).fuse() => Err(Error::ChannelTimeout)
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
        let version = messages::VersionMessage {};
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
        let verack = messages::VerackMessage {};
        self.channel.clone().send(verack).await?;

        debug!(target: "net", "ProtocolVersion::recv_version() [END]");
        Ok(())
    }
}
