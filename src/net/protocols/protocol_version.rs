use futures::FutureExt;
use log::*;
use smol::Executor;
use std::sync::Arc;

use crate::net::error::{NetError, NetResult};
use crate::net::message_subscriber::MessageSubscription;
use crate::net::messages;
use crate::net::utility::sleep;
use crate::net::{ChannelPtr, SettingsPtr};

/// Protocol for version information handshake between nodes at the start of a
/// connection. Implements the process for exchanging version information
/// between nodes. This is the first step when establishing a p2p connection.
///
/// The version protocol starts of by instantiating the protocol and creating a
/// new subscription to version and version acknowledgement messages. Then we
/// run the protocol. Nodes send a version message and wait for a version
/// acknowledgement, while asynchronously waiting for version info from the
/// other node and sending the version acknowledgement.
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

        Arc::new(Self {
            channel,
            version_sub,
            verack_sub,
            settings,
        })
    }
    /// Start version information exchange. Start the timer. Send version info
    /// and wait for version acknowledgement. Wait for version info and send
    /// version acknowledgement.
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
    /// Send and recieve version information.
    async fn exchange_versions(self: Arc<Self>, executor: Arc<Executor<'_>>) -> NetResult<()> {
        debug!(target: "net", "ProtocolVersion::exchange_versions() [START]");

        let send = executor.spawn(self.clone().send_version());
        let recv = executor.spawn(self.recv_version());

        send.await.and(recv.await)?;
        debug!(target: "net", "ProtocolVersion::exchange_versions() [END]");
        Ok(())
    }
    /// Send version info and wait for version acknowledgement.
    async fn send_version(self: Arc<Self>) -> NetResult<()> {
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
    async fn recv_version(self: Arc<Self>) -> NetResult<()> {
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
