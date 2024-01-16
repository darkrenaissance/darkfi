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
use log::debug;
use smol::Executor;

use super::{
    super::{
        channel::ChannelPtr,
        hosts::HostsPtr,
        message::{AddrsMessage, GetAddrsMessage},
        message_subscriber::MessageSubscription,
        p2p::P2pPtr,
        session::SESSION_OUTBOUND,
        settings::SettingsPtr,
    },
    protocol_base::{ProtocolBase, ProtocolBasePtr},
    protocol_jobs_manager::{ProtocolJobsManager, ProtocolJobsManagerPtr},
};
use crate::{system::sleep, Result};

/// Defines address and get-address messages
pub struct ProtocolAddress {
    channel: ChannelPtr,
    addrs_sub: MessageSubscription<AddrsMessage>,
    get_addrs_sub: MessageSubscription<GetAddrsMessage>,
    hosts: HostsPtr,
    settings: SettingsPtr,
    jobsman: ProtocolJobsManagerPtr,
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
        })
    }

    /// Handles receiving the address message. Loops to continually receive
    /// address messages on the address subscription. Validates and adds the
    /// received addresses to the hosts set.
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

            // TODO: We might want to close the channel here if we're getting
            // corrupted addresses.
            self.hosts.store(&addrs_msg.addrs).await;
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
                // TODO: Should this error out, effectively ending the connection?
                let addrs_msg = AddrsMessage { addrs: vec![] };
                self.channel.send(&addrs_msg).await?;
                continue
            }

            // First we grab address with the requested transports
            let mut addrs = self
                .hosts
                .fetch_n_random_with_schemes(&get_addrs_msg.transports, get_addrs_msg.max)
                .await;

            // Then we grab addresses without the requested transports
            // to fill a 2 * max length vector.
            let remain = 2 * get_addrs_msg.max - addrs.len() as u32;
            addrs.append(
                &mut self
                    .hosts
                    .fetch_n_random_excluding_schemes(&get_addrs_msg.transports, remain)
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

    /// Periodically send our external addresses through the channel.
    async fn send_my_addrs(self: Arc<Self>) -> Result<()> {
        debug!(
            target: "net::protocol_address::send_my_addrs()",
            "[START] address={}", self.channel.address(),
        );

        // FIXME: Revisit this. Why do we keep sending it?
        loop {
            let ext_addr_msg = AddrsMessage { addrs: self.settings.external_addrs.clone() };
            self.channel.send(&ext_addr_msg).await?;
            sleep(900).await;
        }
    }
}

#[async_trait]
impl ProtocolBase for ProtocolAddress {
    /// Starts the address protocol. Runs receive address and get address
    /// protocols on the protocol task manager. Then sends get-address msg.
    async fn start(self: Arc<Self>, ex: Arc<Executor<'_>>) -> Result<()> {
        debug!(target: "net::protocol_address::start()", "START => address={}", self.channel.address());

        let type_id = self.channel.session_type_id();

        self.jobsman.clone().start(ex.clone());

        // If it's an outbound session + has an extern_addr, send our address.
        if type_id == SESSION_OUTBOUND && !self.settings.external_addrs.is_empty() {
            self.jobsman.clone().spawn(self.clone().send_my_addrs(), ex.clone()).await;
        }

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
