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

use std::{
    sync::{atomic::Ordering, Arc},
    time::Duration,
};

use futures::{
    future::{select, Either},
    pin_mut,
};
use log::warn;
use smol::lock::RwLock as AsyncRwLock;
use url::Url;

use super::{
    channel::{Channel, ChannelPtr},
    hosts::HostColor,
    session::SessionWeakPtr,
    settings::Settings,
    transport::Dialer,
};
use crate::{system::CondVar, Error, Result};

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
        if hosts.container.contains(HostColor::Black as usize, url) ||
            hosts.block_all_ports(url.host_str().unwrap().to_string())
        {
            warn!(target: "net::connector::connect", "Peer {} is blacklisted", url);
            return Err(Error::ConnectFailed)
        }

        let settings = self.settings.read().await;
        let transports = settings.allowed_transports.clone();
        let transport_mixing = settings.transport_mixing;
        let datastore = settings.datastore.clone();
        let outbound_connect_timeout = settings.outbound_connect_timeout;
        drop(settings);

        let mut endpoint = url.clone();
        let scheme = endpoint.scheme();

        if !transports.contains(&scheme.to_string()) && transport_mixing {
            if transports.contains(&"tor".to_string()) && scheme == "tcp" {
                endpoint.set_scheme("tor")?;
            } else if transports.contains(&"tor+tls".to_string()) && scheme == "tcp+tls" {
                endpoint.set_scheme("tor+tls")?;
            } else if transports.contains(&"nym".to_string()) && scheme == "tcp" {
                endpoint.set_scheme("nym")?;
            } else if transports.contains(&"nym+tls".to_string()) && scheme == "tcp+tls" {
                endpoint.set_scheme("nym+tls")?;
            }
        }

        let dialer = Dialer::new(endpoint.clone(), datastore).await?;
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
                Err(e.into())
            }

            Either::Right((_, _)) => Err(Error::ConnectorStopped),
        }
    }

    pub(crate) fn stop(&self) {
        self.stop_signal.notify()
    }
}
