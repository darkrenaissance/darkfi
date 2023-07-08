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

use async_std::sync::{Arc, Mutex};
use darkfi_serial::serialize;
use futures::{
    io::{ReadHalf, WriteHalf},
    AsyncReadExt,
};
use log::{debug, error, info};
use rand::{rngs::OsRng, Rng};
use smol::Executor;
use url::Url;

use super::{
    message,
    message::Packet,
    message_subscriber::{MessageSubscription, MessageSubsystem},
    p2p::{dnet, P2pPtr},
    session::{Session, SessionBitFlag, SessionWeakPtr},
    transport::PtStream,
};
use crate::{
    system::{StoppableTask, StoppableTaskPtr, Subscriber, SubscriberPtr, Subscription},
    util::{ringbuffer::RingBuffer, time::NanoTimestamp},
    Error, Result,
};

/// Atomic pointer to async channel
pub type ChannelPtr = Arc<Channel>;

/// Channel debug info
#[derive(Clone)]
pub struct ChannelInfo {
    pub addr: Url,
    pub random_id: u32,
    pub remote_node_id: String,
    pub log: RingBuffer<(NanoTimestamp, String, String), 512>,
}

impl ChannelInfo {
    fn new(addr: Url) -> Self {
        Self { addr, random_id: OsRng.gen(), remote_node_id: String::new(), log: RingBuffer::new() }
    }

    /// Get available debug info, resets the ringbuffer when called.
    fn dnet_info(&mut self) -> Self {
        let info = self.clone();
        self.log = RingBuffer::new();
        info
    }
}

/// Async channel for communication between nodes.
pub struct Channel {
    /// The reading half of the transport stream
    reader: Mutex<ReadHalf<Box<dyn PtStream>>>,
    /// The writing half of the transport stream
    writer: Mutex<WriteHalf<Box<dyn PtStream>>>,
    /// Socket address
    address: Url,
    /// The message subsystem instance for this channel
    message_subsystem: MessageSubsystem,
    /// Subscriber listening for stop signal for closing this channel
    stop_subscriber: SubscriberPtr<Error>,
    /// Task that is listening for the stop signal
    receive_task: StoppableTaskPtr,
    /// A boolean marking if this channel is stopped
    stopped: Mutex<bool>,
    /// Weak pointer to respective session
    session: SessionWeakPtr,
    /// Channel debug info
    info: Mutex<Option<ChannelInfo>>,
}

impl std::fmt::Debug for Channel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.address)
    }
}

impl Channel {
    /// Sets up a new channel. Creates a reader and writer [`PtStream`] and
    /// summons the message subscriber subsystem. Performs a network handshake
    /// on the subsystem dispatchers.
    pub async fn new(
        stream: Box<dyn PtStream>,
        address: Url,
        session: SessionWeakPtr,
    ) -> Arc<Self> {
        let (reader, writer) = stream.split();
        let reader = Mutex::new(reader);
        let writer = Mutex::new(writer);

        let message_subsystem = MessageSubsystem::new();
        Self::setup_dispatchers(&message_subsystem).await;

        let info = if *session.upgrade().unwrap().p2p().dnet_enabled.lock().await {
            Mutex::new(Some(ChannelInfo::new(address.clone())))
        } else {
            Mutex::new(None)
        };

        Arc::new(Self {
            reader,
            writer,
            address,
            message_subsystem,
            stop_subscriber: Subscriber::new(),
            receive_task: StoppableTask::new(),
            stopped: Mutex::new(false),
            session,
            info,
        })
    }

    /// Perform network handshake for message subsystem dispatchers.
    async fn setup_dispatchers(subsystem: &MessageSubsystem) {
        subsystem.add_dispatch::<message::VersionMessage>().await;
        subsystem.add_dispatch::<message::VerackMessage>().await;
        subsystem.add_dispatch::<message::PingMessage>().await;
        subsystem.add_dispatch::<message::PongMessage>().await;
        subsystem.add_dispatch::<message::GetAddrsMessage>().await;
        subsystem.add_dispatch::<message::AddrsMessage>().await;
    }

    /// Fetch dnet info for the channel, if enabled.
    /// Returns the [`Channel::address`] and [`ChannelInfo`].
    pub(crate) async fn dnet_info(&self) -> ChannelInfo {
        // We're unwrapping here because if we get None it means
        // there's a bug somehwere where we initialized dnet but
        // ChannelInfo was not created.
        self.info.lock().await.as_mut().unwrap().dnet_info()
    }

    pub(crate) async fn dnet_enable(&self) {
        let mut info = self.info.lock().await;
        if info.is_none() {
            *info = Some(ChannelInfo::new(self.address.clone()));
        }
    }

    pub(crate) async fn dnet_disable(&self) {
        *self.info.lock().await = None;
    }

    /// Starts the channel. Runs a receive loop to start receiving messages
    /// or handles a network failure.
    pub fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) {
        debug!(target: "net::channel::start()", "START => address={}", self.address());

        let self_ = self.clone();
        self.receive_task.clone().start(
            self.clone().main_receive_loop(),
            |result| self_.handle_stop(result),
            Error::NetworkServiceStopped,
            executor,
        );

        debug!(target: "net::channel::start()", "END => address={}", self.address());
    }

    /// Stops the channel. Steps through each component of the channel connection
    /// and sends a stop signal. Notifies all subscribers that the channel has
    /// been closed.
    pub async fn stop(&self) {
        debug!(target: "net::channel::stop()", "START => address={}", self.address());

        if !*self.stopped.lock().await {
            *self.stopped.lock().await = true;

            self.stop_subscriber.notify(Error::ChannelStopped).await;
            self.receive_task.stop().await;
            self.message_subsystem.trigger_error(Error::ChannelStopped).await;
        }

        debug!(target: "net::channel::stop()", "END => address={}", self.address());
    }

    /// Creates a subscription to a stopped signal.
    /// If the channel is stopped then this will return a ChannelStopped error.
    pub async fn subscribe_stop(&self) -> Result<Subscription<Error>> {
        debug!(target: "net::channel::subscribe_stop()", "START => address={}", self.address());

        if *self.stopped.lock().await {
            return Err(Error::ChannelStopped)
        }

        let sub = self.stop_subscriber.clone().subscribe().await;

        debug!(target: "net::channel::subscribe_stop()", "END => address={}", self.address());

        Ok(sub)
    }

    /// Sends a message across a channel. Calls `send_message` that creates
    /// a new payload and sends it over the network transport as a packet.
    /// Returns an error if something goes wrong.
    pub async fn send<M: message::Message>(&self, message: &M) -> Result<()> {
        debug!(
             target: "net::channel::send()", "[START] command={} => address={}",
             M::NAME, self.address(),
        );

        if *self.stopped.lock().await {
            return Err(Error::ChannelStopped)
        }

        // Catch failure and stop channel, return a net error
        if let Err(e) = self.send_message(message).await {
            error!(
                target: "net::channel::send()", "[P2P]Channel send error for [{}]: {}",
                self.address(), e
            );
            self.stop().await;
            return Err(Error::ChannelStopped)
        }

        debug!(
            target: "net::channel::send()", "[END] command={} => address={}",
            M::NAME,self.address(),
        );

        Ok(())
    }

    /// Implements send message functionality. Creates a new payload and
    /// encodes it. Then creates a message packet (the base type of the
    /// network) and copies the payload into it. Then we send the packet
    /// over the network stream.
    async fn send_message<M: message::Message>(&self, message: &M) -> Result<()> {
        let packet = Packet { command: M::NAME.to_string(), payload: serialize(message) };

        dnet!(self,
            let time = NanoTimestamp::current_time();
            match self.info.lock().await.as_mut() {
                Some(info) => info.log.push((time, "send".into(), packet.command.clone())),
                None => unreachable!(),
            }
        );

        let stream = &mut *self.writer.lock().await;
        let _written = message::send_packet(stream, packet).await?;

        Ok(())
    }

    /// Subscribe to a message on the message subsystem.
    pub async fn subscribe_msg<M: message::Message>(&self) -> Result<MessageSubscription<M>> {
        debug!(
            target: "net::channel::subscribe_msg()", "[START] command={} => address={}",
            M::NAME, self.address(),
        );

        let sub = self.message_subsystem.subscribe::<M>().await;

        debug!(
            target: "net::channel::subscribe_msg()", "[END] command={} => address={}",
            M::NAME, self.address(),
        );

        sub
    }

    /// Handle network errors. Panic if error passes silently, otherwise
    /// broadcast the error.
    async fn handle_stop(self: Arc<Self>, result: Result<()>) {
        debug!(target: "net::channel::handle_stop()", "[START] address={}", self.address());

        match result {
            Ok(()) => panic!("Channel task should never complete without error status"),
            // Send this error to all channel subscribers
            Err(e) => self.message_subsystem.trigger_error(e).await,
        }

        debug!(target: "net::channel::handle_stop()", "[END] address={}", self.address());
    }

    /// Run the receive loop. Start receiving messages or handle network failure.
    async fn main_receive_loop(self: Arc<Self>) -> Result<()> {
        debug!(target: "net::channel::main_receive_loop()", "[START] address={}", self.address());

        // Acquire reader lock
        let reader = &mut *self.reader.lock().await;

        // Run loop
        loop {
            let packet = match message::read_packet(reader).await {
                Ok(packet) => packet,
                Err(err) => {
                    if Self::is_eof_error(&err) {
                        info!(
                            target: "net::channel::main_receive_loop()",
                            "[net] Channel inbound connection {} disconnected",
                            self.address(),
                        );
                    } else {
                        error!(
                            target: "net::channel::main_receive_loop()",
                            "Read error on channel {}: {}",
                            self.address(), err,
                        );
                    }

                    debug!(
                        target: "net::channel::main_receive_loop()",
                        "Stopping channel {}", self.address(),
                    );
                    self.stop().await;
                    return Err(Error::ChannelStopped)
                }
            };

            // Send result to our subscribers
            self.message_subsystem.notify(&packet.command, &packet.payload).await;
        }
    }

    /// Returns the local socket address
    pub fn address(&self) -> &Url {
        &self.address
    }

    /// Returns the inner [`MessageSubsystem`] reference
    pub fn message_subsystem(&self) -> &MessageSubsystem {
        &self.message_subsystem
    }

    fn session(&self) -> Arc<dyn Session> {
        self.session.upgrade().unwrap()
    }

    pub fn session_type_id(&self) -> SessionBitFlag {
        let session = self.session();
        session.type_id()
    }

    fn p2p(&self) -> P2pPtr {
        self.session().p2p()
    }

    fn is_eof_error(err: &Error) -> bool {
        match err {
            Error::Io(ioerr) => ioerr == &std::io::ErrorKind::UnexpectedEof,
            _ => false,
        }
    }
}
