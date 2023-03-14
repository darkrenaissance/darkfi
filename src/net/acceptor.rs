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

use std::{env, fs};

use async_std::sync::{Arc, Mutex};
use log::{error, info};
use smol::Executor;
use url::Url;

use super::{
    transport::{TcpTransport, TorTransport, Transport, TransportListener, TransportName},
    Channel, ChannelPtr, SessionWeakPtr,
};
use crate::{
    net::transport::NymTransport,
    system::{StoppableTask, StoppableTaskPtr, Subscriber, SubscriberPtr, Subscription},
    Error, Result,
};

/// Atomic pointer to Acceptor class.
pub type AcceptorPtr = Arc<Acceptor>;

/// Create inbound socket connections.
pub struct Acceptor {
    channel_subscriber: SubscriberPtr<Result<ChannelPtr>>,
    task: StoppableTaskPtr,
    pub session: Mutex<Option<SessionWeakPtr>>,
}

impl Acceptor {
    /// Create new Acceptor object.
    pub fn new(session: Mutex<Option<SessionWeakPtr>>) -> Arc<Self> {
        Arc::new(Self {
            channel_subscriber: Subscriber::new(),
            task: StoppableTask::new(),
            session,
        })
    }
    /// Start accepting inbound socket connections. Creates a listener to start
    /// listening on a local socket address. Then runs an accept loop in a new
    /// thread, erroring if a connection problem occurs.
    pub async fn start(
        self: Arc<Self>,
        accept_url: Url,
        executor: Arc<Executor<'_>>,
    ) -> Result<()> {
        let transport_name = TransportName::try_from(accept_url.clone())?;

        macro_rules! accept {
            ($listener:expr, $transport:expr, $upgrade:expr) => {{
                if let Err(err) = $listener {
                    error!(target: "net::acceptor", "Setup for {} failed: {}", accept_url, err);
                    return Err(Error::BindFailed(accept_url.as_str().into()))
                }

                let listener = $listener?.await;

                if let Err(err) = listener {
                    error!(target: "net::acceptor", "Bind listener to {} failed: {}", accept_url, err);
                    return Err(Error::BindFailed(accept_url.as_str().into()))
                }

                let listener = listener?;

                match $upgrade {
                    None => {
                        self.accept(Box::new(listener), executor);
                    }
                    Some(u) if u == "tls" => {
                        let tls_listener = $transport.upgrade_listener(listener)?.await?;
                        self.accept(Box::new(tls_listener), executor);
                    }
                    Some(u) => return Err(Error::UnsupportedTransportUpgrade(u)),
                }
            }};
        }

        match transport_name {
            TransportName::Tcp(upgrade) => {
                let transport = TcpTransport::new(None, 1024);
                let listener = transport.listen_on(accept_url.clone());
                accept!(listener, transport, upgrade);
            }
            TransportName::Tor(upgrade) => {
                let socks5_url = Url::parse(
                    &env::var("DARKFI_TOR_SOCKS5_URL")
                        .unwrap_or_else(|_| "socks5://127.0.0.1:9050".to_string()),
                )?;

                let torc_url = Url::parse(
                    &env::var("DARKFI_TOR_CONTROL_URL")
                        .unwrap_or_else(|_| "tcp://127.0.0.1:9051".to_string()),
                )?;

                let auth_cookie = env::var("DARKFI_TOR_COOKIE");

                if auth_cookie.is_err() {
                    return Err(Error::TorError(
                            "Please set the env var DARKFI_TOR_COOKIE to the configured tor cookie file. \
                    For example: \
                    \'export DARKFI_TOR_COOKIE=\"/var/lib/tor/control_auth_cookie\"\'".to_string(),
                    ));
                }

                let auth_cookie = auth_cookie.unwrap();
                let auth_cookie = hex::encode(fs::read(auth_cookie).unwrap());
                let transport = TorTransport::new(socks5_url, Some((torc_url, auth_cookie)))?;

                // generate EHS pointing to local address
                let hurl = transport.create_ehs(accept_url.clone())?;

                info!(target: "net::acceptor", "EHS TOR: {}", hurl.to_string());

                let listener = transport.clone().listen_on(accept_url.clone());

                accept!(listener, transport, upgrade);
            }
            TransportName::Nym(upgrade) => {
                let transport = NymTransport::new()?;

                let listener = transport.clone().listen_on(accept_url.clone());

                accept!(listener, transport, upgrade);
            }
            _ => unimplemented!(),
        }
        Ok(())
    }

    /// Stop accepting inbound socket connections.
    pub async fn stop(&self) {
        // Send stop signal
        self.task.stop().await;
    }

    /// Start receiving network messages.
    pub async fn subscribe(self: Arc<Self>) -> Subscription<Result<ChannelPtr>> {
        self.channel_subscriber.clone().subscribe().await
    }

    /// Run the accept loop in a new thread and error if a connection problem
    /// occurs.
    fn accept(self: Arc<Self>, listener: Box<dyn TransportListener>, executor: Arc<Executor<'_>>) {
        let self2 = self.clone();
        self.task.clone().start(
            self.clone().run_accept_loop(listener),
            |result| self2.handle_stop(result),
            Error::NetworkServiceStopped,
            executor,
        );
    }

    /// Run the accept loop.
    async fn run_accept_loop(self: Arc<Self>, listener: Box<dyn TransportListener>) -> Result<()> {
        loop {
            match listener.next().await {
                Ok((stream, url)) => {
                    let channel =
                        Channel::new(stream, url, self.session.lock().await.clone().unwrap()).await;
                    self.channel_subscriber.notify(Ok(channel)).await;
                }
                Err(e) => {
                    error!(target: "net::acceptor", "Error listening for new connection: {}", e);
                }
            }
        }
    }

    /// Handles network errors. Panics if error passes silently, otherwise
    /// broadcasts the error.
    async fn handle_stop(self: Arc<Self>, result: Result<()>) {
        match result {
            Ok(()) => panic!("Acceptor task should never complete without error status"),
            Err(err) => {
                // Send this error to all channel subscribers
                let result = Err(err);
                self.channel_subscriber.notify(result).await;
            }
        }
    }
}
