use async_trait::async_trait;
use log::*;
use smol::Executor;
use std::sync::Arc;

use crate::{
    error::Result,
    net::{p2p::P2pPtr, protocol::ProtocolVersion, ChannelPtr},
};

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
    ) -> Result<()> {
        debug!(target: "net", "Session::register_channel() [START]");

        // Protocols should all be initialized but not started
        // We do this so that the protocols can begin receiving and buffering messages
        // while the handshake protocol is ongoing.
        // They are currently in sleep mode.
        let p2p = self.p2p();
        let protocols = p2p.protocol_registry().attach(channel.clone(), p2p.clone()).await;

        // Perform the handshake protocol
        let protocol_version = ProtocolVersion::new(channel.clone(), self.p2p().settings()).await;
        let handshake_task =
            self.perform_handshake_protocols(protocol_version, channel.clone(), executor.clone());

        // Switch on the channel
        channel.start(executor.clone());

        // Wait for handshake to finish.
        handshake_task.await?;

        // Now the channel is ready

        // Now start all the protocols
        // They are responsible for managing their own lifetimes and
        // correctly self destructing when the channel ends.
        for protocol in protocols {
            // Activate protocol
            protocol.start(executor.clone()).await;
        }

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
    ) -> Result<()> {
        // Perform handshake
        protocol_version.run(executor.clone()).await?;

        // Channel is now initialized

        // Add channel to p2p
        self.p2p().store(channel.clone()).await;

        // Subscribe to stop, so can remove from p2p
        executor.spawn(remove_sub_on_stop(self.p2p(), channel)).detach();

        // Channel is ready for use
        Ok(())
    }

    /// Returns a pointer to the p2p network interface.
    fn p2p(&self) -> P2pPtr;
}
