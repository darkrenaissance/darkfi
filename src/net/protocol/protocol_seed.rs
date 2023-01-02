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

use std::sync::Arc;

use async_trait::async_trait;
use log::debug;
use smol::Executor;

use crate::Result;

use super::{
    super::{
        message, message_subscriber::MessageSubscription, ChannelPtr, HostsPtr, P2pPtr, SettingsPtr,
    },
    ProtocolBase, ProtocolBasePtr,
};

/// Implements the seed protocol.
pub struct ProtocolSeed {
    channel: ChannelPtr,
    hosts: HostsPtr,
    settings: SettingsPtr,
    addr_sub: MessageSubscription<message::AddrsMessage>,
}

impl ProtocolSeed {
    /// Create a new seed protocol.
    pub async fn init(channel: ChannelPtr, p2p: P2pPtr) -> ProtocolBasePtr {
        let hosts = p2p.hosts();
        let settings = p2p.settings();

        //// Create a subscription to address message.
        let addr_sub = channel
            .clone()
            .subscribe_msg::<message::AddrsMessage>()
            .await
            .expect("Missing addr dispatcher!");

        Arc::new(Self { channel, hosts, settings, addr_sub })
    }

    /// Sends own external addresses over a channel. Imports own external addresses
    /// from settings, then adds that addresses to an address message and
    /// sends it out over the channel.
    pub async fn send_self_address(&self) -> Result<()> {
        // Do nothing if external addresses are not configured
        if self.settings.external_addr.is_empty() {
            return Ok(())
        }

        let ext_addrs = self.settings.external_addr.clone();
        debug!(target: "net", "ProtocolSeed::send_self_address() ext_addrs={:?}", ext_addrs);
        let ext_addr_msg = message::ExtAddrsMessage { ext_addrs };
        self.channel.clone().send(ext_addr_msg).await
    }
}

#[async_trait]
impl ProtocolBase for ProtocolSeed {
    /// Starts the seed protocol. Creates a subscription to the address message,
    /// then sends our address to the seed server. Sends a get-address
    /// message and receives an address message.
    async fn start(self: Arc<Self>, _executor: Arc<Executor<'_>>) -> Result<()> {
        debug!(target: "net::protocol_seed::send_self_address()", "ProtocolSeed::start() [START]");

        // Send own address to the seed server.
        self.send_self_address().await?;

        // Send get address message.
        let get_addr = message::GetAddrsMessage {};
        self.channel.clone().send(get_addr).await?;

        // Receive addresses.
        let addrs_msg = self.addr_sub.receive().await?;
        debug!(target: "net::protocol_seed::send_self_address()", "ProtocolSeed::start() received {} addrs", addrs_msg.addrs.len());
        self.hosts.store(addrs_msg.addrs.clone()).await;

        debug!(target: "net::protocol_seed::send_self_address()", "ProtocolSeed::start() [END]");
        Ok(())
    }

    fn name(&self) -> &'static str {
        "ProtocolSeed"
    }
}
