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
use log::{debug, warn};
use smol::Executor;

use super::{
    super::{
        channel::ChannelPtr,
        hosts::{refinery::ping_node, store::HostsPtr},
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

/// Defines address and get-address messages.
/// On receiving GetAddr, nodes send an AddrMessage containing whitelisted nodes.
/// On receiving an AddrMessage, nodes enter the info into their greylists.
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

    /// Handles receiving the get-address message. Continually receives get-address
    /// messages on the get-address subscription. Then replies with an address message.
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

            // First we grab address with the requested transports
            debug!(target: "net::protocol_address::handle_receive_get_addrs()",
            "Fetching whitelist entries with schemes");
            let mut addrs = self
                .hosts
                .whitelist_fetch_n_random_with_schemes(&get_addrs_msg.transports, get_addrs_msg.max)
                .await;

            // Then we grab addresses without the requested transports
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

            debug!(
                target: "net::protocol_address::handle_receive_get_addrs()",
                "Sending {} addresses to {}", addrs.len(), self.channel.address(),
            );

            let addrs_msg = AddrsMessage { addrs };
            self.channel.send(&addrs_msg).await?;
        }
    }

    // If it's an outbound session, we have an extern_addr, and address advertising
    // is enabled, send our address.
    async fn send_my_addrs(self: Arc<Self>) -> Result<()> {
        debug!(target: "net::protocol_address::send_my_addrs()", "[START]");
        let type_id = self.channel.session_type_id();

        if type_id != SESSION_OUTBOUND {
            debug!(target: "net::protocol_address::send_my_addrs()", "Not an outbound session. Stopping");
            return Ok(())
        }

        if self.settings.external_addrs.is_empty() {
            debug!(target: "net::protocol_address::send_my_addrs()", "External addr not configured. Stopping");
            return Ok(())
        }

        // Do nothing if advertise is set to false
        if !self.settings.advertise {
            debug!(target: "net::protocol_address::send_my_addrs()", "Advertise is false. Stopping");
            return Ok(())
        }

        debug!(
            target: "net::protocol_address::send_my_addrs()",
            "[START] address={}", self.channel.address(),
        );

        let mut addrs = vec![];
        for addr in self.settings.external_addrs.clone() {
            debug!(target: "net::protocol_address::send_my_addrs()", "Attempting to ping self");

            // See if we can do a version exchange with ourself.
            if ping_node(&addr, self.p2p.clone()).await {
                // We're online. Update last_seen and broadcast our address.
                let last_seen = UNIX_EPOCH.elapsed().unwrap().as_secs();
                addrs.push((addr, last_seen));
            } else {
                debug!(target: "net::protocol_address::send_my_addrs()", "Ping self failed");
                return Ok(())
            }
        }
        debug!(target: "net::protocol_address::send_my_addrs()", "Broadcasting address");
        let ext_addr_msg = AddrsMessage { addrs };
        self.channel.send(&ext_addr_msg).await?;
        debug!(target: "net::protocol_address::send_my_addrs()", "[END]");

        Ok(())
    }
}

#[async_trait]
impl ProtocolBase for ProtocolAddress {
    /// Starts the address protocol. If it's an outbound session, has an external address
    /// is set to advertise, pings our external address and sends it if everything is fine.
    /// Runs receive address and get address protocols on the protocol task manager.
    /// Then sends get-address msg.
    async fn start(self: Arc<Self>, ex: Arc<Executor<'_>>) -> Result<()> {
        debug!(target: "net::protocol_address::start()", "START => address={}", self.channel.address());

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

        debug!(target: "net::protocol_address::start()", "END => address={}", self.channel.address());
        Ok(())
    }
    fn name(&self) -> &'static str {
        PROTO_NAME
    }
}
