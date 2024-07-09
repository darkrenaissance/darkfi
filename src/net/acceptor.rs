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
    io::ErrorKind,
    sync::{
        atomic::{AtomicUsize, Ordering::SeqCst},
        Arc,
    },
};

use log::{error, warn};
use smol::Executor;
use url::Url;

use super::{
    channel::{Channel, ChannelPtr},
    hosts::HostColor,
    session::SessionWeakPtr,
    transport::{Listener, PtListener},
};
use crate::{
    system::{CondVar, Publisher, PublisherPtr, StoppableTask, StoppableTaskPtr, Subscription},
    Error, Result,
};

/// Atomic pointer to Acceptor
pub type AcceptorPtr = Arc<Acceptor>;

/// Create inbound socket connections
pub struct Acceptor {
    channel_publisher: PublisherPtr<Result<ChannelPtr>>,
    task: StoppableTaskPtr,
    session: SessionWeakPtr,
    conn_count: AtomicUsize,
}

impl Acceptor {
    /// Create new Acceptor object.
    pub fn new(session: SessionWeakPtr) -> AcceptorPtr {
        Arc::new(Self {
            channel_publisher: Publisher::new(),
            task: StoppableTask::new(),
            session,
            conn_count: AtomicUsize::new(0),
        })
    }

    /// Start accepting inbound socket connections
    pub async fn start(self: Arc<Self>, endpoint: Url, ex: Arc<Executor<'_>>) -> Result<()> {
        let datastore =
            self.session.upgrade().unwrap().p2p().settings().read().await.datastore.clone();

        let listener = Listener::new(endpoint, datastore).await?.listen().await?;

        self.accept(listener, ex);
        Ok(())
    }

    /// Stop accepting inbound socket connections
    pub async fn stop(&self) {
        // Send stop signal
        self.task.stop().await;
    }

    /// Start receiving network messages.
    pub async fn subscribe(self: Arc<Self>) -> Subscription<Result<ChannelPtr>> {
        self.channel_publisher.clone().subscribe().await
    }

    /// Run the accept loop in a new thread and error if a connection problem occurs
    fn accept(self: Arc<Self>, listener: Box<dyn PtListener>, ex: Arc<Executor<'_>>) {
        let self_ = self.clone();
        self.task.clone().start(
            self.run_accept_loop(listener, ex.clone()),
            |result| self_.handle_stop(result),
            Error::NetworkServiceStopped,
            ex,
        );
    }

    /// Run the accept loop.
    async fn run_accept_loop(
        self: Arc<Self>,
        listener: Box<dyn PtListener>,
        ex: Arc<Executor<'_>>,
    ) -> Result<()> {
        // CondVar used to notify the loop to recheck if new connections can
        // be accepted by the listener.
        let cv = Arc::new(CondVar::new());
        let hosts = self.session.upgrade().unwrap().p2p().hosts();

        loop {
            // Refuse new connections if we're up to the connection limit
            let limit =
                self.session.upgrade().unwrap().p2p().settings().read().await.inbound_connections;

            if self.clone().conn_count.load(SeqCst) >= limit {
                // This will get notified every time an inbound channel is stopped.
                // These channels are the channels spawned below on listener.next().is_ok().
                // After the notification, we reset the condvar and retry this loop to see
                // if we can accept more connections, and if not - we'll be back here.
                warn!(target: "net::acceptor::run_accept_loop()", "Reached incoming conn limit, waiting...");
                cv.wait().await;
                cv.reset();
                continue
            }

            // Now we wait for a new connection.
            match listener.next().await {
                Ok((stream, url)) => {
                    // Check if we reject this peer
                    if hosts.container.contains(HostColor::Black as usize, &url) ||
                        hosts.block_all_ports(url.host_str().unwrap().to_string())
                    {
                        warn!(target: "net::acceptor::run_accept_loop()", "Peer {} is blacklisted", url);
                        continue
                    }

                    // Create the new Channel.
                    let session = self.session.clone();
                    let channel = Channel::new(stream, None, url, session).await;

                    // Increment the connection counter
                    self.conn_count.fetch_add(1, SeqCst);

                    // This task will subscribe on the new channel and decrement
                    // the connection counter. Along with that, it will notify
                    // the CondVar that might be waiting to allow new connections.
                    let self_ = self.clone();
                    let channel_ = channel.clone();
                    let cv_ = cv.clone();
                    ex.spawn(async move {
                        let stop_sub = channel_.subscribe_stop().await?;
                        stop_sub.receive().await;
                        self_.conn_count.fetch_sub(1, SeqCst);
                        cv_.notify();
                        Ok::<(), crate::Error>(())
                    })
                    .detach();

                    // Finally, notify any publishers about the new channel.
                    self.channel_publisher.notify(Ok(channel)).await;
                }

                // As per accept(2) recommendation:
                Err(e) if e.raw_os_error().is_some() => match e.raw_os_error().unwrap() {
                    libc::EAGAIN | libc::ECONNABORTED | libc::EPROTO | libc::EINTR => continue,
                    libc::ECONNRESET => {
                        warn!(
                            target: "net::acceptor::run_accept_loop()",
                            "[P2P] Connection reset by peer in accept_loop"
                        );
                        continue
                    }
                    libc::ETIMEDOUT => {
                        warn!(
                            target: "net::acceptor::run_accept_loop()",
                            "[P2P] Connection timed out in accept_loop"
                        );
                        continue
                    }
                    x => {
                        error!(
                            target: "net::acceptor::run_accept_loop()",
                            "[P2P] Acceptor failed listening: {} ({})", e, x,
                        );
                        error!(
                            target: "net::acceptor::run_accept_loop()",
                            "[P2P] Closing listener loop"
                        );
                        return Err(e.into())
                    }
                },

                // In case a TLS handshake fails, we'll get this:
                Err(e) if e.kind() == ErrorKind::UnexpectedEof => continue,

                // Handle ErrorKind::Other
                Err(e) if e.kind() == ErrorKind::Other => {
                    if let Some(inner) = std::error::Error::source(&e) {
                        if let Some(inner) = inner.downcast_ref::<futures_rustls::rustls::Error>() {
                            error!(
                                target: "net::acceptor::run_accept_loop()",
                                "[P2P] rustls listener error: {:?}", inner,
                            );
                            continue
                        }
                    }

                    error!(
                        target: "net::acceptor::run_accept_loop()",
                        "[P2P] Unhandled ErrorKind::Other error: {:?}", e,
                    );
                    return Err(e.into())
                }

                // Errors we didn't handle above:
                Err(e) => {
                    error!(
                        target: "net::acceptor::run_accept_loop()",
                        "[P2P] Unhandled listener.next() error: {}", e,
                    );
                    /*
                    error!(
                        target: "net::acceptor::run_accept_loop()",
                        "[P2P] Closing listener loop"
                    );
                    return Err(e.into())
                    */
                    continue
                }
            }
        }
    }

    /// Handles network errors. Panics if errors pass silently, otherwise broadcasts it
    /// to all channel publishers.
    async fn handle_stop(self: Arc<Self>, result: Result<()>) {
        match result {
            Ok(()) => panic!("Acceptor task should never complete without error status"),
            Err(err) => self.channel_publisher.notify(Err(err)).await,
        }
    }
}
