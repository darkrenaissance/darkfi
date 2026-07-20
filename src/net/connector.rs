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
    future::Future,
    io,
    sync::{atomic::Ordering, Arc},
    time::Duration,
};

use futures::{
    future::{select, Either},
    pin_mut,
};
use smol::lock::RwLock as AsyncRwLock;
use url::Url;

use super::{
    channel::{Channel, ChannelPtr},
    session::SessionWeakPtr,
    settings::Settings,
    transport::Dialer,
};
use crate::{net::hosts::HostContainer, system::CondVar, util::logger::verbose, Error, Result};

type DialRoute = (Url, bool, Duration);
type DialFailures = Vec<(Url, io::Error)>;

#[derive(Debug)]
enum DialRoutesError {
    Stopped(Url),
    Failed(DialFailures),
}

fn partition_blacklisted_endpoints<F>(
    endpoints: Vec<(Url, bool)>,
    mut is_blacklisted: F,
) -> (Vec<(Url, bool)>, Vec<Url>)
where
    F: FnMut(&Url) -> bool,
{
    let mut allowed = vec![];
    let mut blocked = vec![];

    for (endpoint, mixed_transport) in endpoints {
        if is_blacklisted(&endpoint) {
            blocked.push(endpoint);
        } else {
            allowed.push((endpoint, mixed_transport));
        }
    }

    (allowed, blocked)
}

fn build_dial_routes(endpoints: Vec<(Url, bool)>, settings: &Settings) -> Vec<DialRoute> {
    endpoints
        .into_iter()
        .map(|(endpoint, mixed_transport)| {
            let timeout = Duration::from_secs(settings.outbound_connect_timeout(endpoint.scheme()));
            (endpoint, mixed_transport, timeout)
        })
        .collect()
}

async fn try_dial_routes<T, F, Fut>(
    routes: Vec<DialRoute>,
    stop_signal: &CondVar,
    mut dial: F,
) -> std::result::Result<(Url, bool, T), DialRoutesError>
where
    F: FnMut(Url, Duration) -> Fut,
    Fut: Future<Output = io::Result<T>>,
{
    let mut failures = vec![];

    for (endpoint, mixed_transport, timeout) in routes {
        let stop_fut = stop_signal.wait();
        let dial_fut = dial(endpoint.clone(), timeout);

        pin_mut!(stop_fut);
        pin_mut!(dial_fut);

        match select(dial_fut, stop_fut).await {
            Either::Left((Ok(stream), _)) => return Ok((endpoint, mixed_transport, stream)),
            Either::Left((Err(err), _)) => failures.push((endpoint, err)),
            Either::Right((_, _)) => return Err(DialRoutesError::Stopped(endpoint)),
        }
    }

    Err(DialRoutesError::Failed(failures))
}

fn sanitized_url(url: &Url) -> String {
    let mut sanitized = url.clone();
    let _ = sanitized.set_password(None);
    let _ = sanitized.set_username("");
    sanitized.set_query(None);
    sanitized.set_fragment(None);
    sanitized.to_string()
}

fn summarize_failures(failures: &DialFailures) -> String {
    failures
        .iter()
        .map(|(endpoint, err)| format!("{} ({:?})", sanitized_url(endpoint), err.kind()))
        .collect::<Vec<_>>()
        .join(", ")
}

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
        // A canonical blacklist match blocks the peer regardless of route.
        if hosts.is_blacklisted(url) {
            let url = sanitized_url(url);
            verbose!(target: "net::connector::connect", "Peer {url} is blacklisted");
            return Err(Error::ConnectFailed(format!("[{url}]: Peer is blacklisted")));
        }

        let settings = self.settings.read().await;
        let datastore = settings.p2p_datastore.clone();
        let i2p_socks5_proxy = settings.i2p_socks5_proxy.clone();

        let endpoints = HostContainer::resolve_dial_endpoints(
            url,
            &settings.active_profiles,
            &settings.mixed_profiles,
            &settings.tor_socks5_proxy,
            &settings.nym_socks5_proxy,
        );
        if endpoints.is_empty() {
            return Err(Error::UnsupportedTransport(url.scheme().to_string()))
        }

        // A derived endpoint match blocks only that route, allowing a safe
        // alternative transport to be tried when one is available.
        let (endpoints, blocked) =
            partition_blacklisted_endpoints(endpoints, |endpoint| hosts.is_blacklisted(endpoint));
        for endpoint in blocked {
            verbose!(
                target: "net::connector::connect",
                "Skipping blacklisted connection route {}",
                sanitized_url(&endpoint),
            );
        }
        if endpoints.is_empty() {
            return Err(Error::ConnectFailed(format!(
                "[{}]: All connection routes are blacklisted",
                sanitized_url(url)
            )))
        }

        let routes = build_dial_routes(endpoints, &settings);
        drop(settings);

        let result = try_dial_routes(routes, &self.stop_signal, |endpoint, timeout| {
            let datastore = datastore.clone();
            let i2p_socks5_proxy = i2p_socks5_proxy.clone();
            async move {
                let dialer = Dialer::new(endpoint, datastore, Some(i2p_socks5_proxy), true).await?;
                dialer.dial(Some(timeout)).await
            }
        })
        .await;

        match result {
            Ok((endpoint, mixed_transport, ptstream)) => {
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

            Err(DialRoutesError::Failed(failures)) => {
                // If we get ENETUNREACH, we don't have IPv6 connectivity so note it down.
                if failures.iter().any(|(_, err)| err.raw_os_error() == Some(libc::ENETUNREACH)) {
                    hosts.ipv6_available.store(false, Ordering::SeqCst);
                }
                Err(Error::ConnectFailed(format!(
                    "All connection routes failed: {}",
                    summarize_failures(&failures)
                )))
            }

            Err(DialRoutesError::Stopped(endpoint)) => {
                Err(Error::ConnectorStopped(format!("[{}]", sanitized_url(&endpoint))))
            }
        }
    }

    pub(crate) fn stop(&self) {
        self.stop_signal.notify()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use crate::net::settings::NetworkProfile;

    use super::*;

    fn route(url: &str, timeout: u64) -> DialRoute {
        (Url::parse(url).unwrap(), true, Duration::from_secs(timeout))
    }

    #[test]
    fn test_mixed_routes_skip_blacklisted_endpoint() {
        let endpoints = vec![
            (Url::parse("tor+tls://peer.example:28880").unwrap(), true),
            (Url::parse("nym+tls://peer.example:28880").unwrap(), true),
        ];

        let (allowed, blocked) =
            partition_blacklisted_endpoints(endpoints, |url| url.scheme() == "tor+tls");

        assert_eq!(allowed.len(), 1);
        assert_eq!(allowed[0].0.scheme(), "nym+tls");
        assert_eq!(blocked.len(), 1);
        assert_eq!(blocked[0].scheme(), "tor+tls");
    }

    #[test]
    fn test_mixed_routes_reject_all_blacklisted_endpoints() {
        let endpoints = vec![
            (Url::parse("tor://peer.example:28880").unwrap(), true),
            (Url::parse("nym://peer.example:28880").unwrap(), true),
        ];

        let (allowed, blocked) = partition_blacklisted_endpoints(endpoints, |_| true);

        assert!(allowed.is_empty());
        assert_eq!(blocked.len(), 2);
    }

    #[test]
    fn test_dial_routes_use_endpoint_profile_timeouts() {
        let mut settings = Settings::default();
        settings.profiles.insert(
            "tor".to_string(),
            NetworkProfile { outbound_connect_timeout: 3, ..Default::default() },
        );
        settings.profiles.insert(
            "nym".to_string(),
            NetworkProfile { outbound_connect_timeout: 7, ..Default::default() },
        );
        let endpoints = vec![
            (Url::parse("tor://peer.example:28880").unwrap(), true),
            (Url::parse("nym://peer.example:28880").unwrap(), true),
        ];

        let routes = build_dial_routes(endpoints, &settings);

        assert_eq!(routes[0].2, Duration::from_secs(3));
        assert_eq!(routes[1].2, Duration::from_secs(7));
    }

    #[test]
    fn test_dial_routes_falls_back_after_failure() {
        smol::block_on(async {
            let attempts = Arc::new(Mutex::new(vec![]));
            let recorded = attempts.clone();
            let routes = vec![
                route("socks5://proxy-one.example:9050/peer.example:28880", 3),
                route("socks5://proxy-two.example:9050/peer.example:28880", 7),
            ];

            let result = try_dial_routes(routes, &CondVar::new(), move |endpoint, timeout| {
                let recorded = recorded.clone();
                async move {
                    recorded.lock().unwrap().push((endpoint.clone(), timeout));
                    if endpoint.host_str() == Some("proxy-one.example") {
                        return Err(io::Error::from(io::ErrorKind::ConnectionRefused))
                    }
                    Ok(42)
                }
            })
            .await
            .unwrap();

            assert_eq!(result.0.host_str(), Some("proxy-two.example"));
            assert_eq!(result.2, 42);
            assert_eq!(
                attempts
                    .lock()
                    .unwrap()
                    .iter()
                    .map(|(endpoint, timeout)| (endpoint.host_str().unwrap().to_string(), *timeout))
                    .collect::<Vec<_>>(),
                [
                    ("proxy-one.example".to_string(), Duration::from_secs(3)),
                    ("proxy-two.example".to_string(), Duration::from_secs(7)),
                ]
            );
        });
    }

    #[test]
    fn test_dial_routes_reports_all_failures_without_credentials() {
        smol::block_on(async {
            let routes = vec![
                route("socks5://alice:secret@proxy-one.example:9050/peer.example:28880", 3),
                route("socks5://bob:hidden@proxy-two.example:9050/peer.example:28880", 7),
            ];

            let Err(DialRoutesError::Failed(failures)) =
                try_dial_routes(routes, &CondVar::new(), |_, _| async {
                    Err::<(), _>(io::Error::from(io::ErrorKind::ConnectionRefused))
                })
                .await
            else {
                panic!("all routes should fail")
            };
            let summary = summarize_failures(&failures);

            assert!(summary.contains("proxy-one.example"));
            assert!(summary.contains("proxy-two.example"));
            assert!(!summary.contains("alice"));
            assert!(!summary.contains("secret"));
            assert!(!summary.contains("bob"));
            assert!(!summary.contains("hidden"));
        });
    }

    #[test]
    fn test_dial_routes_stops_without_trying_fallback() {
        smol::block_on(async {
            let stop_signal = CondVar::new();
            stop_signal.notify();
            let attempts = Arc::new(Mutex::new(vec![]));
            let recorded = attempts.clone();
            let routes = vec![
                route("socks5://proxy-one.example:9050/peer.example:28880", 3),
                route("socks5://proxy-two.example:9050/peer.example:28880", 7),
            ];

            let result = try_dial_routes(routes, &stop_signal, move |endpoint, _| {
                let recorded = recorded.clone();
                async move {
                    recorded.lock().unwrap().push(endpoint);
                    futures::future::pending::<io::Result<()>>().await
                }
            })
            .await;

            assert!(matches!(result, Err(DialRoutesError::Stopped(_))));
            assert_eq!(attempts.lock().unwrap().len(), 1);
        });
    }
}
