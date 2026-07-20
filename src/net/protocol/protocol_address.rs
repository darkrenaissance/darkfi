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

use async_trait::async_trait;
use smol::{lock::RwLock as AsyncRwLock, Executor};
use std::{sync::Arc, time::UNIX_EPOCH};
use tracing::debug;
use url::Url;

use super::{
    super::{
        channel::ChannelPtr,
        hosts::{HostColor, HostContainer, HostsPtr, SHAREABLE_SCHEMES},
        message::{AddrsMessage, GetAddrsMessage},
        message_publisher::MessageSubscription,
        p2p::P2pPtr,
        session::SESSION_OUTBOUND,
        settings::Settings,
    },
    protocol_base::{ProtocolBase, ProtocolBasePtr},
    protocol_jobs_manager::{ProtocolJobsManager, ProtocolJobsManagerPtr},
};
use crate::Result;

/// Defines address and get-address messages.
///
/// On receiving GetAddr, nodes reply an AddrMessage containing nodes from
/// their hostlist.  On receiving an AddrMessage, nodes enter the info into
/// their greylists.
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

/// Strip query parameters from a URL before broadcasting.
///
/// This prevents leaking internal tracking identifiers (e.g., UPnP cookies)
/// that could be used for fingerprinting nodes on the P2P network.
fn strip_query_params(url: &Url) -> Url {
    let mut stripped = url.clone();
    stripped.set_query(None);
    stripped
}

fn select_addrs(container: &HostContainer, request: &GetAddrsMessage) -> Vec<(Url, u64)> {
    // Ignore private or unknown endpoint schemes and collapse duplicate preferences.
    let mut requested_transports = vec![];
    for transport in &request.transports {
        if SHAREABLE_SCHEMES.contains(&transport.as_str()) &&
            !requested_transports.contains(transport)
        {
            requested_transports.push(transport.clone());
        }
    }

    let max = request.max as usize;
    let response_max = max.saturating_mul(2);

    // Prefer proven and recently refined peers matching the request.
    let mut addrs =
        container.fetch_n_random_with_schemes(HostColor::Gold, &requested_transports, max);
    addrs.append(&mut container.fetch_n_random_with_schemes(
        HostColor::White,
        &requested_transports,
        max,
    ));

    // Fill the second half with other public peers so uncommon transports
    // continue to propagate across the network.
    let remain = response_max.saturating_sub(addrs.len());
    addrs.append(&mut container.fetch_n_random_excluding_schemes(
        HostColor::Gold,
        &requested_transports,
        remain,
    ));

    let remain = response_max.saturating_sub(addrs.len());
    addrs.append(&mut container.fetch_n_random_excluding_schemes(
        HostColor::White,
        &requested_transports,
        remain,
    ));

    let remain = response_max.saturating_sub(addrs.len());
    addrs.append(&mut container.fetch_n_random(HostColor::Dark, remain));

    // Dark entries are untrusted and can contain private endpoint schemes.
    addrs.retain(|addr| SHAREABLE_SCHEMES.contains(&addr.0.scheme()));
    addrs
}

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
            target: "net::protocol_address::handle_receive_addrs",
            "[START] address={}", self.channel.display_address(),
        );

        loop {
            let addrs_msg = self.addrs_sub.receive().await?;
            debug!(
                target: "net::protocol_address::handle_receive_addrs",
                "Received {} addrs from {}", addrs_msg.addrs.len(), self.channel.display_address(),
            );

            debug!(
                target: "net::protocol_address::handle_receive_addrs",
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
            target: "net::protocol_address::handle_receive_get_addrs",
            "[START] address={}", self.channel.display_address(),
        );

        loop {
            let get_addrs_msg = self.get_addrs_sub.receive().await?;

            debug!(
                target: "net::protocol_address::handle_receive_get_addrs",
                "Received GetAddrs({}) message from {}", get_addrs_msg.max, self.channel.display_address(),
            );

            let addrs = select_addrs(&self.hosts.container, &get_addrs_msg);

            debug!(
                target: "net::protocol_address::handle_receive_get_addrs",
                "Sending {} addresses to {}", addrs.len(), self.channel.display_address(),
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
            "[START] channel address={}", self.channel.display_address(),
        );

        if self.channel.session_type_id() != SESSION_OUTBOUND {
            debug!(
                target: "net::protocol_address::send_my_addrs",
                "Not an outbound session. Stopping",
            );
            return Ok(())
        }

        let external_addrs = self.channel.hosts().external_addrs().await;

        if external_addrs.is_empty() {
            debug!(
                target: "net::protocol_address::send_my_addrs",
                "External addr not configured. Stopping",
            );
            return Ok(())
        }

        let mut addrs = vec![];

        for addr in external_addrs {
            let stripped_addr = strip_query_params(&addr);
            let last_seen = UNIX_EPOCH.elapsed().unwrap().as_secs();
            addrs.push((stripped_addr, last_seen));
        }

        debug!(
            target: "net::protocol_address::send_my_addrs",
            "Broadcasting {} addresses", addrs.len(),
        );

        let ext_addr_msg = AddrsMessage { addrs };
        self.channel.send(&ext_addr_msg).await?;

        debug!(
            target: "net::protocol_address::send_my_addrs",
            "[END] channel address={}", self.channel.display_address(),
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
            target: "net::protocol_address::start",
            "START => address={}", self.channel.display_address(),
        );

        let settings = self.settings.read().await;
        let outbound_connections = settings.outbound_connections;
        let transports = HostContainer::shareable_schemes(
            &settings.active_profiles,
            &settings.mixed_profiles,
            &settings.tor_socks5_proxy,
            &settings.nym_socks5_proxy,
        );
        let getaddrs_max = settings.getaddrs_max;
        drop(settings);

        self.jobsman.clone().start(ex.clone()).await?;

        self.jobsman.clone().spawn(self.clone().send_my_addrs(), ex.clone()).await;

        self.jobsman.clone().spawn(self.clone().handle_receive_addrs(), ex.clone()).await;

        self.jobsman.spawn(self.clone().handle_receive_get_addrs(), ex).await;

        // Send get_address message.
        // We ask for a maximum of u8::MAX addresses from a single node
        let get_addrs = GetAddrsMessage {
            max: getaddrs_max.unwrap_or(outbound_connections.min(u32::MAX as usize) as u32),
            transports,
        };
        self.channel.send(&get_addrs).await?;

        debug!(
            target: "net::protocol_address::start",
            "END => address={}", self.channel.display_address(),
        );

        Ok(())
    }
    fn name(&self) -> &'static str {
        PROTO_NAME
    }
}

#[cfg(test)]
mod tests {
    use darkfi_serial::serialize;
    use smol::lock::RwLock as AsyncRwLock;
    use std::sync::Arc;
    use url::Url;

    use crate::net::{
        hosts::{HostColor, HostContainer, Hosts, SHAREABLE_SCHEMES},
        message::GET_ADDRS_MAX_BYTES,
        Settings,
    };

    use super::{select_addrs, GetAddrsMessage};

    // Helps to check if the MAX_BYTES for GetAddrs message is valid as new transports are added
    #[test]
    fn test_get_addrs_msg_size() {
        let message = GetAddrsMessage {
            max: u8::MAX as u32,
            transports: SHAREABLE_SCHEMES.iter().map(|x| x.to_string()).collect(),
        };

        assert_eq!(serialize(&message).len() as u64, GET_ADDRS_MAX_BYTES);
    }

    #[test]
    fn test_get_addrs_prefers_transport_mixing_source() {
        let hosts = Hosts::new(Arc::new(AsyncRwLock::new(Settings::default())));
        let container = &hosts.container;
        let mixed = Url::parse("tcp+tls://mixed.example:28880").unwrap();
        let fallback = Url::parse("tcp://fallback.example:28880").unwrap();
        container.store(HostColor::Gold, mixed.clone(), 2);
        container.store(HostColor::Gold, fallback.clone(), 1);

        let transports = HostContainer::shareable_schemes(
            &["tor+tls".to_string()],
            &["tcp+tls".to_string()],
            &None,
            &None,
        );
        let response = select_addrs(container, &GetAddrsMessage { max: 1, transports });

        assert_eq!(response, [(mixed, 2), (fallback, 1)]);
    }
}
