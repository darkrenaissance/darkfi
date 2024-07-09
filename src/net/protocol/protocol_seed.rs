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

use std::{sync::Arc, time::UNIX_EPOCH};

use async_trait::async_trait;
use log::debug;
use smol::{lock::RwLock as AsyncRwLock, Executor};

use super::{
    super::{
        channel::ChannelPtr,
        hosts::{HostColor, HostsPtr},
        message::{AddrsMessage, GetAddrsMessage},
        message_publisher::MessageSubscription,
        p2p::P2pPtr,
        settings::Settings,
    },
    protocol_base::{ProtocolBase, ProtocolBasePtr},
};
use crate::Result;

/// Implements the seed protocol
pub struct ProtocolSeed {
    channel: ChannelPtr,
    hosts: HostsPtr,
    settings: Arc<AsyncRwLock<Settings>>,
    addr_sub: MessageSubscription<AddrsMessage>,
}

const PROTO_NAME: &str = "ProtocolSeed";

impl ProtocolSeed {
    /// Create a new seed protocol.
    pub async fn init(channel: ChannelPtr, p2p: P2pPtr) -> ProtocolBasePtr {
        // Create a subscription to address message
        let addr_sub =
            channel.subscribe_msg::<AddrsMessage>().await.expect("Missing addr dispatcher!");

        Arc::new(Self { channel, hosts: p2p.hosts(), settings: p2p.settings(), addr_sub })
    }

    /// Send our own external addresses over a channel. Set the
    /// last_seen field to now.
    pub async fn send_my_addrs(&self) -> Result<()> {
        debug!(
            target: "net::protocol_seed::send_my_addrs",
            "[START] channel address={}", self.channel.address(),
        );

        let external_addrs = self.settings.read().await.external_addrs.clone();

        if external_addrs.is_empty() {
            debug!(
                target: "net::protocol_seed::send_my_addrs",
                "External address is not configured. Stopping",
            );
            return Ok(())
        }

        let mut addrs = vec![];

        for addr in external_addrs {
            let last_seen = UNIX_EPOCH.elapsed().unwrap().as_secs();
            addrs.push((addr, last_seen));
        }

        debug!(
            target: "net::protocol_seed::send_my_addrs",
            "Broadcasting {} addresses", addrs.len(),
        );

        let ext_addr_msg = AddrsMessage { addrs };
        self.channel.send(&ext_addr_msg).await?;

        debug!(
            target: "net::protocol_seed::send_my_addrs",
            "[END] channel address={}", self.channel.address(),
        );

        Ok(())
    }
}

#[async_trait]
impl ProtocolBase for ProtocolSeed {
    /// Starts the seed protocol. Creates a subscription to the address
    /// message.  If our external address is enabled, then send our address
    /// to the seed server.  Sends a get-address message and receives an
    /// address messsage.
    async fn start(self: Arc<Self>, _ex: Arc<Executor<'_>>) -> Result<()> {
        debug!(target: "net::protocol_seed::start()", "START => address={}", self.channel.address());

        // Send own address to the seed server
        self.send_my_addrs().await?;

        let settings = self.settings.read().await;
        let outbound_connections = settings.outbound_connections;
        let allowed_transports = settings.allowed_transports.clone();
        drop(settings);

        // Send get address message
        let get_addr =
            GetAddrsMessage { max: outbound_connections as u32, transports: allowed_transports };
        self.channel.send(&get_addr).await?;

        // Receive addresses
        let addrs_msg = self.addr_sub.receive().await?;
        debug!(
            target: "net::protocol_seed::start()",
            "Received {} addrs from {}", addrs_msg.addrs.len(), self.channel.address(),
        );

        if !addrs_msg.addrs.is_empty() {
            debug!(
                target: "net::protocol_seed::start()",
                "Appending to greylist...",
            );
            self.hosts.insert(HostColor::Grey, &addrs_msg.addrs).await;
        }

        debug!(target: "net::protocol_seed::start()", "END => address={}", self.channel.address());
        Ok(())
    }

    fn name(&self) -> &'static str {
        PROTO_NAME
    }
}
