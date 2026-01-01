/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

use std::{
    sync::{Arc, Weak},
    time::UNIX_EPOCH,
};

use async_trait::async_trait;
use smol::Executor;
use tracing::{debug, error, trace};

use super::{
    channel::ChannelPtr,
    dnet::{self, dnetev, DnetEvent},
    hosts::HostColor,
    p2p::P2pPtr,
    protocol::ProtocolVersion,
};
use crate::{system::Subscription, util::logger::verbose, Error, Result};

pub mod inbound_session;
pub use inbound_session::{InboundSession, InboundSessionPtr};
pub mod manual_session;
pub use manual_session::{ManualSession, ManualSessionPtr};
pub mod outbound_session;
pub use outbound_session::{OutboundSession, OutboundSessionPtr};
pub mod seedsync_session;
pub use seedsync_session::{SeedSyncSession, SeedSyncSessionPtr};
pub mod refine_session;
pub use refine_session::{RefineSession, RefineSessionPtr};
pub mod direct_session;
pub use direct_session::{DirectSession, DirectSessionPtr};

/// Bitwise selectors for the `protocol_registry`
pub type SessionBitFlag = u32;
pub const SESSION_INBOUND: SessionBitFlag = 0b000001;
pub const SESSION_OUTBOUND: SessionBitFlag = 0b000010;
pub const SESSION_MANUAL: SessionBitFlag = 0b000100;
pub const SESSION_SEED: SessionBitFlag = 0b001000;
pub const SESSION_REFINE: SessionBitFlag = 0b010000;
pub const SESSION_DIRECT: SessionBitFlag = 0b100000;

pub const SESSION_DEFAULT: SessionBitFlag = 0b100111;
pub const SESSION_ALL: SessionBitFlag = 0b111111;

pub type SessionWeakPtr = Weak<dyn Session + Send + Sync + 'static>;

/// Removes channel from the list of connected channels when a stop signal
/// is received.
pub async fn remove_sub_on_stop(
    p2p: P2pPtr,
    channel: ChannelPtr,
    type_id: SessionBitFlag,
    stop_sub: Subscription<Error>,
) {
    debug!(target: "net::session::remove_sub_on_stop", "[START]");
    let hosts = p2p.hosts();
    let addr = channel.address();

    stop_sub.receive().await;

    debug!(
        target: "net::session::remove_sub_on_stop",
        "Received stop event. Removing channel {}",
        channel.display_address()
    );

    // Downgrade to greylist if this is a outbound session.
    if type_id & (SESSION_OUTBOUND | SESSION_DIRECT) != 0 {
        debug!(
            target: "net::session::remove_sub_on_stop",
            "Downgrading {}",
            channel.display_address()
        );

        // If the host we are downgrading has been moved to blacklist,
        // fetch_last_seen(addr) can return None. We simply print an
        // error in this case.
        match hosts.fetch_last_seen(addr) {
            Some(last_seen) => {
                if let Err(e) = hosts.move_host(addr, last_seen, HostColor::Grey).await {
                    error!(target: "net::session::remove_sub_on_stop",
            "Failed to move host {} to Greylist! Err={e}", channel.display_address());
                }
            }
            None => {
                error!(target: "net::session::remove_sub_on_stop",
               "Failed to fetch last seen for {}", channel.display_address());
            }
        }
    }

    // For all sessions that are not refine sessions, mark this addr as
    // Free. `unregister()` frees up this addr for any future operation. We
    // don't call this on refine sessions since the unregister() call
    // happens in the refinery directly.
    if type_id & SESSION_REFINE == 0 {
        if let Err(e) = hosts.unregister(channel.address()) {
            error!(target: "net::session::remove_sub_on_stop", "Error while unregistering addr={}, err={e}", channel.display_address());
        }
    }

    if type_id & SESSION_DIRECT != 0 {
        dnetev!(p2p.session_direct(), DirectDisconnected, {
            connect_addr: channel.info.connect_addr.clone(),
            err: "Channel stopped".to_string()
        });
        verbose!(
            target: "net::direct_session",
            "[P2P] Direct outbound disconnected [{}]",
            channel.display_address()
        );
    }

    if !p2p.is_connected() {
        hosts.disconnect_publisher.notify(Error::NetworkNotConnected).await;
    }
    debug!(target: "net::session::remove_sub_on_stop", "[END]");
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
        trace!(target: "net::session::register_channel", "[START]");

        // Protocols should all be initialized but not started.
        // We do this so that the protocols can begin receiving and buffering
        // messages while the handshake protocol is ongoing. They are currently
        // in sleep mode.
        let p2p = self.p2p();
        let protocols =
            p2p.protocol_registry().attach(self.type_id(), channel.clone(), p2p.clone()).await;

        // Perform the handshake protocol
        let protocol_version = ProtocolVersion::new(channel.clone(), p2p.settings().clone()).await;
        debug!(
            target: "net::session::register_channel",
            "Performing handshake protocols {}", channel.clone().display_address(),
        );

        let handshake_task =
            self.perform_handshake_protocols(protocol_version, channel.clone(), executor.clone());

        // Switch on the channel
        channel.clone().start(executor.clone());

        // Wait for handshake to finish.
        match handshake_task.await {
            Ok(()) => {
                debug!(target: "net::session::register_channel",
                "Handshake successful {}", channel.clone().display_address());
            }
            Err(e) => {
                debug!(target: "net::session::register_channel",
                "Handshake error {e} {}", channel.clone().display_address());

                return Err(e)
            }
        }

        // Now the channel is ready
        debug!(target: "net::session::register_channel", "Session handshake complete");
        debug!(target: "net::session::register_channel", "Activating remaining protocols");

        // Now start all the protocols. They are responsible for managing their own
        // lifetimes and correctly selfdestructing when the channel ends.
        for protocol in protocols {
            protocol.start(executor.clone()).await?;
        }

        trace!(target: "net::session::register_channel", "[END]");

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
        // Subscribe to stop events
        let stop_sub = channel.clone().subscribe_stop().await?;

        // Perform handshake
        match protocol_version.run(executor.clone()).await {
            Ok(()) => {
                // Upgrade to goldlist if this is a outbound session.
                if self.type_id() & (SESSION_OUTBOUND | SESSION_DIRECT) != 0 {
                    debug!(
                        target: "net::session::perform_handshake_protocols",
                        "Upgrading {}", channel.display_address(),
                    );

                    let last_seen = UNIX_EPOCH.elapsed().unwrap().as_secs();

                    self.p2p()
                        .hosts()
                        .move_host(channel.address(), last_seen, HostColor::Gold)
                        .await?;
                }

                // Attempt to add channel to registry
                self.p2p().hosts().register_channel(channel.clone()).await;

                // Subscribe to stop, so we can remove from registry
                executor
                    .spawn(remove_sub_on_stop(self.p2p(), channel, self.type_id(), stop_sub))
                    .detach();

                // Channel is ready for use
                Ok(())
            }
            Err(e) => return Err(e),
        }
    }

    /// Returns a pointer to the p2p network interface
    fn p2p(&self) -> P2pPtr;

    /// Return the session bit flag for the session type
    fn type_id(&self) -> SessionBitFlag;

    /// Reload settings for this session
    async fn reload(self: Arc<Self>);
}
