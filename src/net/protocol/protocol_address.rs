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
        session::SESSION_OUTBOUND,
        settings::Settings,
    },
    protocol_base::{ProtocolBase, ProtocolBasePtr},
    protocol_jobs_manager::{ProtocolJobsManager, ProtocolJobsManagerPtr},
};
use crate::{Error, Result};

/// Defines address and get-address messages. On receiving GetAddr, nodes
/// reply an AddrMessage containing nodes from their hostlist.  On receiving
/// an AddrMessage, nodes enter the info into their greylists.
///
/// The node selection logic for creating an AddrMessage is as follows:
///
/// 1. First select nodes matching the requested transports from the
///    anchorlist. These nodes have the highest guarantee of being reachable,
///    so we prioritize them first.
///
/// 2. Then select nodes matching the requested transports from the
///    whitelist.
///
/// 3. Next select whitelist nodes that don't match our transports. We do
///    this so that nodes share and propagate nodes of different transports,
///    even if they can't connect to them themselves.
///
/// 4. Finally, if there's still space available, fill the remaining vector
///    space with darklist entries. This is necessary to propagate transports
///    that neither this node nor the receiving node support.
pub struct ProtocolAddress {
    channel: ChannelPtr,
    addrs_sub: MessageSubscription<AddrsMessage>,
    get_addrs_sub: MessageSubscription<GetAddrsMessage>,
    hosts: HostsPtr,
    settings: Arc<AsyncRwLock<Settings>>,
    jobsman: ProtocolJobsManagerPtr,
}

const PROTO_NAME: &str = "ProtocolAddress";

/// A vector of all currently accepted transports and valid transport
/// combinations.  Should be updated if and when new transports are
/// added. Creates a upper bound on the number of transports a given peer
/// can request.
const TRANSPORT_COMBOS: [&str; 7] = ["tor", "tls", "tcp", "nym", "tor+tls", "nym+tls", "tcp+tls"];

impl ProtocolAddress {
    /// Creates a new address protocol. Makes an address, an external address
    /// and a get-address subscription and adds them to the address protocol
    /// instance.
    pub async fn init(channel: ChannelPtr, p2p: P2pPtr) -> ProtocolBasePtr {
        // Creates a subscription to address message
        let addrs_sub =
            channel.subscribe_msg::<AddrsMessage>().await.expect("Missing addrs dispatcher!");

        // Creates a subscription to get-address message
        let get_addrs_sub =
            channel.subscribe_msg::<GetAddrsMessage>().await.expect("Missing getaddrs dispatcher!");

        Arc::new(Self {
            channel: channel.clone(),
            addrs_sub,
            get_addrs_sub,
            hosts: p2p.hosts(),
            jobsman: ProtocolJobsManager::new(PROTO_NAME, channel),
            settings: p2p.settings(),
        })
    }

    /// Handles receiving the address message. Loops to continually receive
    /// address messages on the address subscription. Validates and adds the
    /// received addresses to the greylist.
    async fn handle_receive_addrs(self: Arc<Self>) -> Result<()> {
        debug!(
            target: "net::protocol_address::handle_receive_addrs()",
            "[START] address={}", self.channel.address(),
        );

        loop {
            let addrs_msg = self.addrs_sub.receive().await?;
            debug!(
                target: "net::protocol_address::handle_receive_addrs()",
                "Received {} addrs from {}", addrs_msg.addrs.len(), self.channel.address(),
            );

            debug!(
                target: "net::protocol_address::handle_receive_addrs()",
                "Appending to greylist...",
            );

            self.hosts.insert(HostColor::Grey, &addrs_msg.addrs).await;
        }
    }

    /// Handles receiving the get-address message. Continually receives
    /// get-address messages on the get-address subscription. Then replies
    /// with an address message.
    async fn handle_receive_get_addrs(self: Arc<Self>) -> Result<()> {
        debug!(
            target: "net::protocol_address::handle_receive_get_addrs()",
            "[START] address={}", self.channel.address(),
        );

        loop {
            let get_addrs_msg = self.get_addrs_sub.receive().await?;

            debug!(
                target: "net::protocol_address::handle_receive_get_addrs()",
                "Received GetAddrs({}) message from {}", get_addrs_msg.max, self.channel.address(),
            );

            // Check that this peer isn't requesting more transports than we support
            // (the max number of all transports, plus mixing).
            if get_addrs_msg.transports.len() > TRANSPORT_COMBOS.len() {
                return Err(Error::InvalidTransportRequest);
            }

            // First we grab address with the requested transports from the gold list
            debug!(target: "net::protocol_address::handle_receive_get_addrs()",
            "Fetching gold entries with schemes");
            let mut addrs = self.hosts.container.fetch_n_random_with_schemes(
                HostColor::Gold,
                &get_addrs_msg.transports,
                get_addrs_msg.max,
            );

            // Then we grab address with the requested transports from the whitelist
            debug!(target: "net::protocol_address::handle_receive_get_addrs()",
            "Fetching whitelist entries with schemes");
            addrs.append(&mut self.hosts.container.fetch_n_random_with_schemes(
                HostColor::White,
                &get_addrs_msg.transports,
                get_addrs_msg.max,
            ));

            // Next we grab addresses without the requested transports
            // to fill a 2 * max length vector.

            // Then we grab address without the requested transports from the gold list
            debug!(target: "net::protocol_address::handle_receive_get_addrs()",
            "Fetching gold entries without schemes");
            let remain = 2 * get_addrs_msg.max - addrs.len() as u32;
            addrs.append(&mut self.hosts.container.fetch_n_random_excluding_schemes(
                HostColor::Gold,
                &get_addrs_msg.transports,
                remain,
            ));

            // Then we grab address without the requested transports from the white list
            debug!(target: "net::protocol_address::handle_receive_get_addrs()",
            "Fetching white entries without schemes");
            let remain = 2 * get_addrs_msg.max - addrs.len() as u32;
            addrs.append(&mut self.hosts.container.fetch_n_random_excluding_schemes(
                HostColor::White,
                &get_addrs_msg.transports,
                remain,
            ));

            // If there's still space available, take from the Dark list.

            /* NOTE: We share peers from our Dark list because to ensure
            that non-compatiable transports are shared with other nodes
            so that they propagate on the network even if they're not
            popular transports. */

            debug!(target: "net::protocol_address::handle_receive_get_addrs()",
            "Fetching dark entries");
            let remain = 2 * get_addrs_msg.max - addrs.len() as u32;
            addrs.append(&mut self.hosts.container.fetch_n_random(HostColor::Dark, remain));

            debug!(
                target: "net::protocol_address::handle_receive_get_addrs()",
                "Sending {} addresses to {}", addrs.len(), self.channel.address(),
            );

            let addrs_msg = AddrsMessage { addrs };
            self.channel.send(&addrs_msg).await?;
        }
    }

    /// Send our own external addresses over a channel. Set the
    /// last_seen field to now.
    async fn send_my_addrs(self: Arc<Self>) -> Result<()> {
        debug!(
            target: "net::protocol_address::send_my_addrs",
            "[START] channel address={}", self.channel.address(),
        );

        let type_id = self.channel.session_type_id();
        if type_id != SESSION_OUTBOUND {
            debug!(
                target: "net::protocol_address::send_my_addrs",
                "Not an outbound session. Stopping",
            );
            return Ok(())
        }

        let external_addrs = self.settings.read().await.external_addrs.clone();

        if external_addrs.is_empty() {
            debug!(
                target: "net::protocol_address::send_my_addrs",
                "External addr not configured. Stopping",
            );
            return Ok(())
        }

        let mut addrs = vec![];

        for addr in external_addrs {
            let last_seen = UNIX_EPOCH.elapsed().unwrap().as_secs();
            addrs.push((addr, last_seen));
        }

        debug!(
            target: "net::protocol_address::send_my_addrs",
            "Broadcasting {} addresses", addrs.len(),
        );

        let ext_addr_msg = AddrsMessage { addrs };
        self.channel.send(&ext_addr_msg).await?;

        debug!(
            target: "net::protocol_address::send_my_addrs",
            "[END] channel address={}", self.channel.address(),
        );

        Ok(())
    }
}

#[async_trait]
impl ProtocolBase for ProtocolAddress {
    /// Start the address protocol. If it's an outbound session and has an
    /// external address, send our external address. Run receive address
    /// and get address protocols on the protocol task manager. Then send
    /// get-address msg.
    async fn start(self: Arc<Self>, ex: Arc<Executor<'_>>) -> Result<()> {
        debug!(
            target: "net::protocol_address::start()",
            "START => address={}", self.channel.address(),
        );

        let settings = self.settings.read().await;
        let outbound_connections = settings.outbound_connections;
        let allowed_transports = settings.allowed_transports.clone();
        drop(settings);

        self.jobsman.clone().start(ex.clone());

        self.jobsman.clone().spawn(self.clone().send_my_addrs(), ex.clone()).await;

        self.jobsman.clone().spawn(self.clone().handle_receive_addrs(), ex.clone()).await;

        self.jobsman.spawn(self.clone().handle_receive_get_addrs(), ex).await;

        // Send get_address message.
        let get_addrs =
            GetAddrsMessage { max: outbound_connections as u32, transports: allowed_transports };
        self.channel.send(&get_addrs).await?;

        debug!(
            target: "net::protocol_address::start()",
            "END => address={}", self.channel.address(),
        );

        Ok(())
    }
    fn name(&self) -> &'static str {
        PROTO_NAME
    }
}
