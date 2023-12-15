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

use std::{sync::Arc, time::UNIX_EPOCH};

use async_trait::async_trait;
use log::debug;
use smol::Executor;

use super::{
    super::{
        channel::ChannelPtr,
        hosts::HostsPtr,
        message::{AddrsMessage, GetAddrsMessage},
        message_subscriber::MessageSubscription,
        p2p::P2pPtr,
        session::OutboundSessionPtr,
        settings::SettingsPtr,
    },
    protocol_base::{ProtocolBase, ProtocolBasePtr},
};
use crate::Result;

/// Implements the seed protocol
pub struct ProtocolSeed {
    channel: ChannelPtr,
    hosts: HostsPtr,
    settings: SettingsPtr,
    addr_sub: MessageSubscription<AddrsMessage>,
    // We require this to access ping_self() method.
    session: OutboundSessionPtr,
}

const PROTO_NAME: &str = "ProtocolSeed";

impl ProtocolSeed {
    /// Create a new seed protocol.
    pub async fn init(channel: ChannelPtr, p2p: P2pPtr) -> ProtocolBasePtr {
        let hosts = p2p.hosts();
        let settings = p2p.settings();
        let session = p2p.session_outbound();

        // Create a subscription to address message
        let addr_sub =
            channel.subscribe_msg::<AddrsMessage>().await.expect("Missing addr dispatcher!");

        Arc::new(Self { channel, hosts, settings, addr_sub, session })
    }

    /// Sends own external addresses over a channel. Imports own external addresses
    /// from settings, then adds those addresses to an addrs message and sends it
    /// out over the channel.
    pub async fn send_self_address(&self) -> Result<()> {
        debug!(target: "net::protocol_seed::send_self_address()", "[START]");
        // Do nothing if external addresses are not configured
        if self.settings.external_addrs.is_empty() {
            return Ok(())
        }

        // Do nothing if advertise is set to false
        if self.settings.advertise {
            return Ok(())
        }

        let mut addrs = vec![];
        for addr in self.settings.external_addrs.clone() {
            // See if we can do a version exchange with ourself.
            if self.session.ping_node(&addr).await {
                // We're online. Update last_seen and broadcast our address.
                let last_seen = UNIX_EPOCH.elapsed().unwrap().as_secs();
                addrs.push((addr, last_seen));
            }
        }

        debug!(
            target: "net::protocol_seed::send_self_address()",
            "ext_addrs={:?}, dest={}", addrs, self.channel.address(),
        );

        let ext_addr_msg = AddrsMessage { addrs };
        self.channel.send(&ext_addr_msg).await?;
        debug!(target: "net::protocol_seed::send_self_address()", "[END]");
        Ok(())
    }
}

#[async_trait]
impl ProtocolBase for ProtocolSeed {
    /// Starts the seed protocol. Creates a subscription to the address message,
    /// then sends our address to the seed server. Sends a get-address message
    /// and receives an address messsage.
    async fn start(self: Arc<Self>, _ex: Arc<Executor<'_>>) -> Result<()> {
        debug!(target: "net::protocol_seed::start()", "START => address={}", self.channel.address());

        // TODO: only do this if "advertise" is true
        // Send own address to the seed server
        self.send_self_address().await?;

        // Send get address message
        let get_addr = GetAddrsMessage {
            max: self.settings.outbound_connections as u32,
            transports: self.settings.allowed_transports.clone(),
        };
        self.channel.send(&get_addr).await?;

        // Receive addresses
        let addrs_msg = self.addr_sub.receive().await?;
        debug!(
            target: "net::protocol_seed::start()",
            "Received {} addrs from {}", addrs_msg.addrs.len(), self.channel.address(),
        );
        self.hosts.greylist_store(&addrs_msg.addrs).await;

        debug!(target: "net::protocol_seed::start()", "END => address={}", self.channel.address());
        Ok(())
    }

    fn name(&self) -> &'static str {
        PROTO_NAME
    }
}
