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

use std::sync::{Arc, Weak};

use async_trait::async_trait;
use log::debug;
use smol::Executor;

use crate::Result;

use super::{p2p::P2pPtr, protocol::ProtocolVersion, ChannelPtr};

/// Seed sync session creates a connection to the seed nodes specified in settings.
/// A new seed sync session is created every time we call p2p::start(). The seed
/// sync session loops through all the configured seeds and tries to connect to
/// them using a Connector. Seed sync either connects successfully,
/// fails with an error or times out.
///
/// If a seed node connects successfully, it runs a version exchange protocol,
/// stores the channel in the p2p list of channels, and disconnects, removing
/// the channel from the channel list.
///
/// The channel is registered using Session trait method, register_channel().
/// This invokes the Protocol Registry method attach(). Usually this returns a
/// list of protocols that we loop through and start. In this case, attach()
/// uses the bitflag selector to identify seed sessions and exclude them.
///
/// The version exchange occurs inside register_channel(). We create a handshake
/// task that runs the version exchange with the function
/// perform_handshake_protocols(). This runs the version exchange protocol,
/// stores the channel in the p2p list of channels, and subscribes to a stop
/// signal.
pub mod seedsync_session;

pub mod manual_session;

/// Inbound connections session. Manages the creation of inbound sessions. Used
/// to create an inbound session and start and stop the session.
///
/// Class consists of 3 pointers: a weak pointer to the p2p parent class, an
/// acceptor pointer, and a stoppable task pointer. Using a weak pointer to P2P
/// allows us to avoid circular dependencies.
pub mod inbound_session;

/// Outbound connections session. Manages the creation of outbound sessions.
/// Used to create an outbound session and stop and start the session.
///
/// Class consists of a weak pointer to the p2p interface and a vector
/// of outbound connection slots. Using a weak pointer to p2p allows us to avoid
/// circular dependencies. The vector of slots is wrapped in a mutex lock. This
/// is switched on everytime we instantiate a connection slot and insures that
/// no other part of the program uses the slots at the same time.
pub mod outbound_session;

// bitwise selectors for the protocol_registry
pub type SessionBitflag = u32;
pub const SESSION_INBOUND: SessionBitflag = 0b0001;
pub const SESSION_OUTBOUND: SessionBitflag = 0b0010;
pub const SESSION_MANUAL: SessionBitflag = 0b0100;
pub const SESSION_SEED: SessionBitflag = 0b1000;
pub const SESSION_ALL: SessionBitflag = 0b1111;

pub use inbound_session::InboundSession;
pub use manual_session::ManualSession;
pub use outbound_session::OutboundSession;
pub use seedsync_session::SeedSyncSession;

pub type SessionWeakPtr = Arc<Weak<dyn Session + Send + Sync + 'static>>;

/// Removes channel from the list of connected channels when a stop signal is
/// received.
async fn remove_sub_on_stop(p2p: P2pPtr, channel: ChannelPtr) {
    debug!(target: "net", "remove_sub_on_stop() [START]");
    // Subscribe to stop events
    let stop_sub = channel.clone().subscribe_stop().await;

    if stop_sub.is_ok() {
        // Wait for a stop event
        stop_sub.unwrap().receive().await;
    }

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
/// Defines methods that are used across sessions. Implements registering the
/// channel and initializing the channel by performing a network handshake.
pub trait Session: Sync {
    /// Registers a new channel with the session. Performs a network handshake
    /// and starts the channel.
    // if we need to pass Self as an Arc we can do so like this:
    // pub trait MyTrait: Send + Sync {
    //      async fn foo(&self, self_: Arc<dyn MyTrait>) {}
    // }
    async fn register_channel(
        &self,
        channel: ChannelPtr,
        executor: Arc<Executor<'_>>,
    ) -> Result<()> {
        debug!(target: "net", "Session::register_channel() [START]");

        // Protocols should all be initialized but not started
        // We do this so that the protocols can begin receiving and buffering messages
        // while the handshake protocol is ongoing.
        // They are currently in sleep mode.
        let p2p = self.p2p();
        let protocols =
            p2p.protocol_registry().attach(self.type_id(), channel.clone(), p2p.clone()).await;

        // Perform the handshake protocol
        let protocol_version =
            ProtocolVersion::new(channel.clone(), p2p.settings().clone(), p2p.hosts().clone())
                .await;
        let handshake_task =
            self.perform_handshake_protocols(protocol_version, channel.clone(), executor.clone());

        // Switch on the channel
        channel.start(executor.clone());

        // Wait for handshake to finish.
        handshake_task.await?;

        // Now the channel is ready
        debug!(target: "net", "Session handshake complete. Activating remaining protocols");

        // Now start all the protocols
        // They are responsible for managing their own lifetimes and
        // correctly self destructing when the channel ends.
        for protocol in protocols {
            // Activate protocol
            protocol.start(executor.clone()).await?;
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

    async fn get_info(&self) -> serde_json::Value;

    /// Returns a pointer to the p2p network interface.
    fn p2p(&self) -> P2pPtr;

    fn type_id(&self) -> u32;
}
