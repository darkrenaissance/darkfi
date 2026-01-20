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
    sync::{atomic::Ordering, Arc},
    time::Duration,
};

use futures::{
    future::{select, Either},
    pin_mut,
};
use smol::lock::RwLock as AsyncRwLock;
use tracing::warn;
use url::Url;

use super::{
    channel::{Channel, ChannelPtr},
    hosts::HostColor,
    session::SessionWeakPtr,
    settings::Settings,
    transport::Dialer,
};
use crate::{net::hosts::HostContainer, system::CondVar, Error, Result};

/// Create outbound socket connections
pub struct Connector {
    /// P2P settings
    settings: Arc<AsyncRwLock<Settings>>,
    /// Weak pointer to the session
    pub session: SessionWeakPtr,
    /// Stop signal that aborts the connector if received.
    stop_signal: CondVar,
}

impl Connector {
    /// Create a new connector with given network settings
    pub fn new(settings: Arc<AsyncRwLock<Settings>>, session: SessionWeakPtr) -> Self {
        Self { settings, session, stop_signal: CondVar::new() }
    }

    /// Establish an outbound connection
    pub async fn connect(&self, url: &Url) -> Result<(Url, ChannelPtr)> {
        let hosts = self.session.upgrade().unwrap().p2p().hosts();
        if hosts.container.contains(HostColor::Black, url) || hosts.block_all_ports(url) {
            warn!(target: "net::connector::connect", "Peer {url} is blacklisted");
            return Err(Error::ConnectFailed(format!("[{url}]: Peer is blacklisted")));
        }

        let settings = self.settings.read().await;
        let datastore = settings.p2p_datastore.clone();
        let i2p_socks5_proxy = settings.i2p_socks5_proxy.clone();

        let (endpoint, mixed_transport) = if let Some(mixed_host) = HostContainer::mix_host(
            url,
            &settings.active_profiles,
            &settings.mixed_profiles,
            &settings.tor_socks5_proxy,
            &settings.nym_socks5_proxy,
        )
        .first()
        {
            (mixed_host.clone(), true)
        } else {
            (url.clone(), false)
        };

        let outbound_connect_timeout = settings.outbound_connect_timeout(endpoint.scheme());
        drop(settings);

        let dialer = match Dialer::new(endpoint.clone(), datastore, Some(i2p_socks5_proxy)).await {
            Ok(dialer) => dialer,
            Err(err) => return Err(Error::ConnectFailed(format!("[{endpoint}]: {err}"))),
        };
        let timeout = Duration::from_secs(outbound_connect_timeout);

        let stop_fut = async {
            self.stop_signal.wait().await;
        };
        let dial_fut = async { dialer.dial(Some(timeout)).await };

        pin_mut!(stop_fut);
        pin_mut!(dial_fut);

        match select(dial_fut, stop_fut).await {
            Either::Left((Ok(ptstream), _)) => {
                let channel = Channel::new(
                    ptstream,
                    Some(endpoint.clone()),
                    url.clone(),
                    self.session.clone(),
                    mixed_transport,
                )
                .await;
                Ok((endpoint, channel))
            }

            Either::Left((Err(e), _)) => {
                // If we get ENETUNREACH, we don't have IPv6 connectivity so note it down.
                if e.raw_os_error() == Some(libc::ENETUNREACH) {
                    self.session
                        .upgrade()
                        .unwrap()
                        .p2p()
                        .hosts()
                        .ipv6_available
                        .store(false, Ordering::SeqCst);
                }
                Err(Error::ConnectFailed(format!("[{endpoint}]: {e}")))
            }

            Either::Right((_, _)) => Err(Error::ConnectorStopped(format!("[{endpoint}]"))),
        }
    }

    pub(crate) fn stop(&self) {
        self.stop_signal.notify()
    }
}
