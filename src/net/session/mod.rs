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

use std::sync::{Arc, Weak};

use async_trait::async_trait;
use log::debug;
use smol::Executor;

use super::{channel::ChannelPtr, p2p::P2pPtr, protocol::ProtocolVersion};
use crate::Result;

pub mod inbound_session;
pub use inbound_session::{InboundSession, InboundSessionPtr};
pub mod manual_session;
pub use manual_session::{ManualSession, ManualSessionPtr};
pub mod outbound_session;
pub use outbound_session::{OutboundSession, OutboundSessionPtr};
pub mod seedsync_session;
pub use seedsync_session::{SeedSyncSession, SeedSyncSessionPtr};

/// Bitwise selectors for the `protocol_registry`
pub type SessionBitFlag = u32;
pub const SESSION_INBOUND: SessionBitFlag = 0b0001;
pub const SESSION_OUTBOUND: SessionBitFlag = 0b0010;
pub const SESSION_MANUAL: SessionBitFlag = 0b0100;
pub const SESSION_SEED: SessionBitFlag = 0b1000;
pub const SESSION_ALL: SessionBitFlag = 0b1111;

pub type SessionWeakPtr = Weak<dyn Session + Send + Sync + 'static>;

/// Removes channel from the list of connected channels when a stop signal
/// is received.
pub async fn remove_sub_on_stop(p2p: P2pPtr, channel: ChannelPtr) {
    debug!(target: "net::session::remove_sub_on_stop()", "[START]");
    // Subscribe to stop events
    let stop_sub = channel.clone().subscribe_stop().await;

    if let Ok(stop_sub) = stop_sub {
        // Wait for a stop event
        stop_sub.receive().await;
    }

    debug!(
        target: "net::session::remove_sub_on_stop()",
        "Received stop event. Removing channel {}", channel.address(),
    );

    // Remove channel from p2p
    p2p.remove(channel).await;
    debug!(target: "net::session::remove_sub_on_stop()", "[END]");
}

/// Session trait. Defines methods that are used across sessions.
/// Implements registering the channel and initializing the channel by
/// performing a network handshake.
#[async_trait]
pub trait Session: Sync {
    /// Registers a new channel with the session.
    /// Performs a network handshake and starts the channel.
    /// If we need to pass `Self` as an `Arc` we can do so like this:
    /// ```
    /// pub trait MyTrait: Send + Sync {
    ///     async fn foo(&self, self_: Arc<dyn MyTrait>) {}
    /// }
    /// ```
    async fn register_channel(
        &self,
        channel: ChannelPtr,
        executor: Arc<Executor<'_>>,
    ) -> Result<()> {
        debug!(target: "net::session::register_channel()", "[START]");

        // Protocols should all be initialized but not started.
        // We do this so that the protocols can begin receiving and buffering
        // messages while the handshake protocol is ongoing. They are currently
        // in sleep mode.
        let p2p = self.p2p();
        let protocols =
            p2p.protocol_registry().attach(self.type_id(), channel.clone(), p2p.clone()).await;

        // Perform the handshake protocol
        let protocol_version = ProtocolVersion::new(channel.clone(), p2p.settings().clone()).await;
        debug!(target: "net::session::register_channel()", "register_channel {}", channel.clone().address());
        let handshake_task =
            self.perform_handshake_protocols(protocol_version, channel.clone(), executor.clone());

        // Switch on the channel
        channel.start(executor.clone());

        // Wait for handshake to finish.
        handshake_task.await?;

        // Now the channel is ready
        debug!(target: "net::session::register_channel()", "Session handshake complete");
        debug!(target: "net::session::register_channel()", "Activating remaining protocols");

        // Now start all the protocols. They are responsible for managing their own
        // lifetimes and correctly selfdestructing when the channel ends.
        for protocol in protocols {
            protocol.start(executor.clone()).await?;
        }

        debug!(target: "net::session::register_channel()", "[END]");

        Ok(())
    }

    /// Performs network handshake to initialize channel. Adds the channel to
    /// the list of connected channels, and prepares to remove the channel when
    /// a stop signal is received.
    async fn perform_handshake_protocols(
        &self,
        protocol_version: Arc<ProtocolVersion>,
        channel: ChannelPtr,
        executor: Arc<Executor<'_>>,
    ) -> Result<()> {
        // Perform handshake
        protocol_version.run(executor.clone()).await?;

        // Add channel to p2p
        self.p2p().store(channel.clone()).await;

        // Subscribe to stop, so we can remove from p2p
        executor.spawn(remove_sub_on_stop(self.p2p(), channel)).detach();

        // Channel is ready for use
        Ok(())
    }

    /// Returns a pointer to the p2p network interface
    fn p2p(&self) -> P2pPtr;

    /// Return the session bit flag for the session type
    fn type_id(&self) -> SessionBitFlag;
}
