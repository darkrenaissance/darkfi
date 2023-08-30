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

use std::sync::Arc;

use log::error;
use smol::Executor;
use url::Url;

use super::{
    channel::{Channel, ChannelPtr},
    session::SessionWeakPtr,
    transport::{Listener, PtListener},
};
use crate::{
    system::{StoppableTask, StoppableTaskPtr, Subscriber, SubscriberPtr, Subscription},
    Error, Result,
};

/// Atomic pointer to Acceptor
pub type AcceptorPtr = Arc<Acceptor>;

/// Create inbound socket connections
pub struct Acceptor {
    channel_subscriber: SubscriberPtr<Result<ChannelPtr>>,
    task: StoppableTaskPtr,
    session: SessionWeakPtr,
}

impl Acceptor {
    /// Create new Acceptor object.
    pub fn new(session: SessionWeakPtr) -> AcceptorPtr {
        Arc::new(Self {
            channel_subscriber: Subscriber::new(),
            task: StoppableTask::new(),
            session,
        })
    }

    /// Start accepting inbound socket connections
    pub async fn start(self: Arc<Self>, endpoint: Url, ex: Arc<Executor<'_>>) -> Result<()> {
        let listener = Listener::new(endpoint).await?.listen().await?;
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
        self.channel_subscriber.clone().subscribe().await
    }

    /// Run the accept loop in a new thread and error if a connection problem occurs
    fn accept(self: Arc<Self>, listener: Box<dyn PtListener>, ex: Arc<Executor<'_>>) {
        let self_ = self.clone();
        self.task.clone().start(
            self.run_accept_loop(listener),
            |result| self_.handle_stop(result),
            Error::NetworkServiceStopped,
            ex,
        );
    }

    /// Run the accept loop.
    async fn run_accept_loop(self: Arc<Self>, listener: Box<dyn PtListener>) -> Result<()> {
        loop {
            match listener.next().await {
                Ok((stream, url)) => {
                    let session = self.session.clone();
                    let channel = Channel::new(stream, url, session).await;
                    self.channel_subscriber.notify(Ok(channel)).await;
                }

                // As per accept(2) recommendation:
                Err(e) => {
                    if let Some(os_err) = e.raw_os_error() {
                        // TODO: Should EINTR actually break out? Check if StoppableTask does this.
                        // TODO: Investigate why libc::EWOULDBLOCK is not considered reachable
                        match os_err {
                            libc::EAGAIN | libc::ECONNABORTED | libc::EPROTO | libc::EINTR => {
                                continue
                            }
                            _ => { /* Do nothing */ }
                        }
                    }
                    error!(
                        target: "net::acceptor::run_accept_loop()",
                        "[P2P] Acceptor failed listening: {}", e,
                    );
                    return Err(e.into())
                }
            }
        }
    }

    /// Handles network errors. Panics if errors pass silently, otherwise broadcasts it
    /// to all channel subscribers.
    async fn handle_stop(self: Arc<Self>, result: Result<()>) {
        match result {
            Ok(()) => panic!("Acceptor task should never complete without error status"),
            Err(err) => self.channel_subscriber.notify(Err(err)).await,
        }
    }
}
