use async_trait::async_trait;
use log::*;
use smol::Executor;
use std::sync::Arc;

use crate::net::error::NetResult;
use crate::net::p2p::P2pPtr;
use crate::net::protocols::ProtocolVersion;
use crate::net::ChannelPtr;

/// Removes channel from the list of connected channels when a stop signal is
/// received.
async fn remove_sub_on_stop(p2p: P2pPtr, channel: ChannelPtr) {
    debug!(target: "net", "remove_sub_on_stop() [START]");
    // Subscribe to stop events
    let stop_sub = channel.clone().subscribe_stop().await;
    // Wait for a stop event
    let _ = stop_sub.receive().await;
    debug!(target: "net",
        "remove_sub_on_stop(): received stop event. Removing channel {}",
        channel.address()
    );
    // Remove channel from p2p
    p2p.remove(channel).await;
    debug!(target: "net", "remove_sub_on_stop() [END]");
}

#[async_trait]
/// Session trait.
pub trait Session: Sync {
    /// Registers a new channel with the session. Performs a network handshake
    /// and starts the channel.
    async fn register_channel(
        self: Arc<Self>,
        channel: ChannelPtr,
        executor: Arc<Executor<'_>>,
    ) -> NetResult<()> {
        debug!(target: "net", "Session::register_channel() [START]");

        let protocol_version = ProtocolVersion::new(channel.clone(), self.p2p().settings()).await;
        let handshake_task =
            self.perform_handshake_protocols(protocol_version, channel.clone(), executor.clone());

        // start channel
        channel.start(executor);

        handshake_task.await?;

        debug!(target: "net", "Session::register_channel() [END]");
        Ok(())
    }

    /// Performs network handshake to initialize channel. Adds the channel to
    /// the list of connected channels, and prepares to remove the channel
    /// when a stop signal is received.
    async fn perform_handshake_protocols(
        &self,
        protocol_version: Arc<ProtocolVersion>,
        channel: ChannelPtr,
        executor: Arc<Executor<'_>>,
    ) -> NetResult<()> {
        // Perform handshake
        protocol_version.run(executor.clone()).await?;

        // Channel is now initialized

        // Add channel to p2p
        self.p2p().store(channel.clone()).await;

        // Subscribe to stop, so can remove from p2p
        executor
            .spawn(remove_sub_on_stop(self.p2p(), channel))
            .detach();

        // Channel is ready for use
        Ok(())
    }

    /// Returns a pointer to the p2p network interface.
    fn p2p(&self) -> P2pPtr;
}
