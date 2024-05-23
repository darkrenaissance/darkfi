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
    io::{self, ErrorKind},
    time::Duration,
};

use arti_client::{config::BoolOrAuto, DataStream, StreamPrefs, TorClient};
use futures::{
    future::{select, Either},
    pin_mut,
};
use log::{debug, warn};
use smol::{lock::OnceCell, Timer};
use tor_error::ErrorReport;
use tor_rtcompat::PreferredRuntime;

/// A static for `TorClient` reusability
static TOR_CLIENT: OnceCell<TorClient<PreferredRuntime>> = OnceCell::new();

/// Tor Dialer implementation
#[derive(Debug, Clone)]
pub struct TorDialer;

impl TorDialer {
    /// Instantiate a new [`TorDialer`] object
    pub(crate) async fn new() -> io::Result<Self> {
        Ok(Self {})
    }

    /// Internal dial function
    pub(crate) async fn do_dial(
        &self,
        host: &str,
        port: u16,
        conn_timeout: Option<Duration>,
    ) -> io::Result<DataStream> {
        debug!(target: "net::tor::do_dial", "Dialing {}:{} with Tor...", host, port);

        // Initialize or fetch the static TOR_CLIENT that should be reused in
        // the Tor dialer
        let client = match TOR_CLIENT
            .get_or_try_init(|| async {
                debug!(target: "net::tor::do_dial", "Bootstrapping...");
                TorClient::builder().create_bootstrapped().await
            })
            .await
        {
            Ok(client) => client,
            Err(e) => {
                warn!("{}", e.report());
                return Err(io::Error::new(
                    ErrorKind::Other,
                    "Internal Tor error, see logged warning",
                ))
            }
        };

        let mut stream_prefs = StreamPrefs::new();
        stream_prefs.connect_to_onion_services(BoolOrAuto::Explicit(true));

        // If a timeout is configured, run both the connect and timeout futures
        // and return whatever finishes first. Otherwise, wait on the connect future.
        let connect = client.connect_with_prefs((host, port), &stream_prefs);

        match conn_timeout {
            Some(t) => {
                let timeout = Timer::after(t);
                pin_mut!(timeout);
                pin_mut!(connect);

                match select(connect, timeout).await {
                    Either::Left((Ok(stream), _)) => Ok(stream),

                    Either::Left((Err(e), _)) => {
                        warn!("{}", e.report());
                        Err(io::Error::new(
                            ErrorKind::Other,
                            "Internal Tor error, see logged warning",
                        ))
                    }

                    Either::Right((_, _)) => Err(io::ErrorKind::TimedOut.into()),
                }
            }

            None => {
                match connect.await {
                    Ok(stream) => Ok(stream),
                    Err(e) => {
                        // Extract error reports (i.e. very detailed debugging)
                        // from arti-client in order to help debug Tor connections.
                        // https://docs.rs/arti-client/latest/arti_client/#reporting-arti-errors
                        // https://gitlab.torproject.org/tpo/core/arti/-/issues/1086
                        warn!("{}", e.report());
                        Err(io::Error::new(
                            ErrorKind::Other,
                            "Internal Tor error, see logged warning",
                        ))
                    }
                }
            }
        }
    }
}
