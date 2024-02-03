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

use std::sync::Arc;

use async_trait::async_trait;
use log::{debug, warn};
use smol::Executor;

use super::{
    super::{
        channel::ChannelPtr,
        hosts::store::HostsPtr,
        message::{AddrsMessage, GetAddrsMessage},
        message_subscriber::MessageSubscription,
        p2p::P2pPtr,
        session::SESSION_OUTBOUND,
        settings::SettingsPtr,
    },
    protocol_base::{ProtocolBase, ProtocolBasePtr},
    protocol_jobs_manager::{ProtocolJobsManager, ProtocolJobsManagerPtr},
};
use crate::Result;

/// Defines address and get-address messages. On receiving GetAddr, nodes
/// reply an AddrMessage containing nodes from their hostlist.  On receiving
/// an AddrMessage, nodes enter the info into their greylists.
///
/// The node selection logic for creating an AddrMessage is as follows:
///
/// 1. First select nodes matching the requested transports from the
/// anchorlist. These nodes have the highest guarantee of being reachable, so we
/// prioritize them first.
///
/// 2. Then select nodes matching the requested transports from the
/// whitelist. 
///
/// 3. Next select whitelist nodes that don't match our transports. We do
/// this so that nodes share and propagate nodes of different transports,
/// even if they can't connect to them themselves.
///
/// 4. Finally, if there's still space available, fill the remaining vector
/// space with greylist entries. This is necessary in case this node does
/// not support the transports of the requesting node (non-supported
/// transports are stored on the greylist).
pub struct ProtocolAddress {
    channel: ChannelPtr,
    addrs_sub: MessageSubscription<AddrsMessage>,
    get_addrs_sub: MessageSubscription<GetAddrsMessage>,
    hosts: HostsPtr,
    settings: SettingsPtr,
    jobsman: ProtocolJobsManagerPtr,
    p2p: P2pPtr,
}

const PROTO_NAME: &str = "ProtocolAddress";

impl ProtocolAddress {
    /// Creates a new address protocol. Makes an address, an external address
    /// and a get-address subscription and adds them to the address protocol
    /// instance.
    pub async fn init(channel: ChannelPtr, p2p: P2pPtr) -> ProtocolBasePtr {
        let settings = p2p.settings();
        let hosts = p2p.hosts();

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
            hosts,
            jobsman: ProtocolJobsManager::new(PROTO_NAME, channel),
            settings,
            p2p,
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

            self.hosts.greylist_store_or_update(&addrs_msg.addrs).await;
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

            // Validate transports length
            // TODO: Verify this limit. It should be the max number of all our allowed transports,
            //       plus their mixing.
            if get_addrs_msg.transports.len() > 20 {
                warn!(target: "net::protocol_address::handle_receive_get_addrs()",
                "Sending empty Addrs message");

                // TODO: Should this error out, effectively ending the connection?
                let addrs_msg = AddrsMessage { addrs: vec![] };
                self.channel.send(&addrs_msg).await?;
                continue
            }

            // First we grab address with the requested transports from the anchorlist
            debug!(target: "net::protocol_address::handle_receive_get_addrs()",
            "Fetching anchorlist entries with schemes");
            let mut addrs = self
                .hosts
                .anchorlist_fetch_n_random_with_schemes(
                    &get_addrs_msg.transports,
                    get_addrs_msg.max,
                )
                .await;

            // Then we grab address with the requested transports from the whitelist
            debug!(target: "net::protocol_address::handle_receive_get_addrs()",
            "Fetching whitelist entries with schemes");
            addrs.append(
                &mut self.hosts
                    .whitelist_fetch_n_random_with_schemes(
                        &get_addrs_msg.transports,
                        get_addrs_msg.max,
                    )
                    .await,
            );

            // Next we grab addresses without the requested transports
            // to fill a 2 * max length vector.
            debug!(target: "net::protocol_address::handle_receive_get_addrs()",
            "Fetching whitelist entries without schemes");
            let remain = 2 * get_addrs_msg.max - addrs.len() as u32;
            addrs.append(
                &mut self
                    .hosts
                    .whitelist_fetch_n_random_excluding_schemes(&get_addrs_msg.transports, remain)
                    .await,
            );

            // If there's still space available, take from the greylist.
            // Schemes are not taken into account.
            debug!(target: "net::protocol_address::handle_receive_get_addrs()",
            "Fetching greylist entries");
            let remain = 2 * get_addrs_msg.max - addrs.len() as u32;
            addrs.append(&mut self.hosts.greylist_fetch_n_random(remain).await);

            debug!(
                target: "net::protocol_address::handle_receive_get_addrs()",
                "Sending {} addresses to {}", addrs.len(), self.channel.address(),
            );

            let addrs_msg = AddrsMessage { addrs };
            self.channel.send(&addrs_msg).await?;
        }
    }

    /// Send our own external addresses over a channel. Get the latest
    /// last_seen field from InboundSession, and send it along with our
    /// external address.
    ///
    /// If our external address is misconfigured, send an empty vector.
    /// If we have reached our inbound connection limit, send our external
    /// address with a `last_seen` field that corresponds to the last time
    /// we could receive inbound connections.
    async fn send_my_addrs(self: Arc<Self>) -> Result<()> {
        debug!(
            target: "net::protocol_address::send_my_addrs()",
            "[START] channel address={}", self.channel.address(),
        );

        let type_id = self.channel.session_type_id();
        if type_id != SESSION_OUTBOUND {
            debug!(target: "net::protocol_address::send_my_addrs()",
            "Not an outbound session. Stopping");
            return Ok(())
        }

        if self.settings.external_addrs.is_empty() {
            debug!(target: "net::protocol_address::send_my_addrs()",
            "External addr not configured. Stopping");
            return Ok(())
        }

        let mut addrs = vec![];
        let inbound = self.p2p.session_inbound();
        for (addr, last_seen) in inbound.ping_self.addrs.lock().await.iter() {
            addrs.push((addr.clone(), last_seen.clone()));
        }

        debug!(target: "net::protocol_address::send_my_addrs()",
        "Broadcasting {} addresses", addrs.len());
        let ext_addr_msg = AddrsMessage { addrs };
        self.channel.send(&ext_addr_msg).await?;
        debug!(target: "net::protocol_address::send_my_addrs()",
        "[END] channel address={}", self.channel.address());

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
        debug!(target: "net::protocol_address::start()",
        "START => address={}", self.channel.address());

        self.jobsman.clone().start(ex.clone());

        self.jobsman.clone().spawn(self.clone().send_my_addrs(), ex.clone()).await;

        self.jobsman.clone().spawn(self.clone().handle_receive_addrs(), ex.clone()).await;

        self.jobsman.spawn(self.clone().handle_receive_get_addrs(), ex).await;

        // Send get_address message.
        let get_addrs = GetAddrsMessage {
            max: self.settings.outbound_connections as u32,
            transports: self.settings.allowed_transports.clone(),
        };
        self.channel.send(&get_addrs).await?;

        debug!(target: "net::protocol_address::start()",
        "END => address={}", self.channel.address());

        Ok(())
    }
    fn name(&self) -> &'static str {
        PROTO_NAME
    }
}
