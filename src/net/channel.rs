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
use futures::{
    io::{ReadHalf, WriteHalf},
    AsyncReadExt,
};
use log::{debug, error, info};
use rand::Rng;
use serde_json::json;
use smol::Executor;
use url::Url;

use super::{
    message,
    message_subscriber::{MessageSubscription, MessageSubsystem},
    transport::TransportStream,
    Session, SessionBitflag, SessionWeakPtr,
};
use crate::{
    system::{StoppableTask, StoppableTaskPtr, Subscriber, SubscriberPtr, Subscription},
    util::time::NanoTimestamp,
    Error, Result,
};

/// Atomic pointer to async channel.
pub type ChannelPtr = Arc<Channel>;

struct ChannelInfo {
    random_id: u32,
    remote_node_id: String,
    last_msg: String,
    last_status: String,
    // Message log which is cleared on querying get_info
    log: Option<Mutex<Vec<(NanoTimestamp, String, String)>>>,
}

impl ChannelInfo {
    fn new(channel_log: bool) -> Self {
        let log = match channel_log {
            true => Some(Mutex::new(Vec::new())),
            false => None,
        };

        Self {
            random_id: rand::thread_rng().gen(),
            remote_node_id: String::new(),
            last_msg: String::new(),
            last_status: String::new(),
            log,
        }
    }

    // ANCHOR: get_info
    async fn get_info(&self) -> serde_json::Value {
        let log = match &self.log {
            Some(l) => {
                let mut lock = l.lock().await;
                let ret = lock.clone();
                *lock = Vec::new();
                ret
            }
            None => vec![],
        };

        json!({
            "random_id": self.random_id,
            "remote_node_id": self.remote_node_id,
            "last_msg": self.last_msg,
            "last_status": self.last_status,
            "log": log,
        })
    }
    // ANCHOR_END: get_info
}

/// Async channel for communication between nodes.
pub struct Channel {
    reader: Mutex<ReadHalf<Box<dyn TransportStream>>>,
    writer: Mutex<WriteHalf<Box<dyn TransportStream>>>,
    address: Url,
    message_subsystem: MessageSubsystem,
    stop_subscriber: SubscriberPtr<Error>,
    receive_task: StoppableTaskPtr,
    stopped: Mutex<bool>,
    info: Mutex<ChannelInfo>,
    session: SessionWeakPtr,
}

impl Channel {
    /// Sets up a new channel. Creates a reader and writer TCP stream and
    /// summons the message subscriber subsystem. Performs a network
    /// handshake on the subsystem dispatchers.
    pub async fn new(
        stream: Box<dyn TransportStream>,
        address: Url,
        session: SessionWeakPtr,
    ) -> Arc<Self> {
        let (reader, writer) = stream.split();
        let reader = Mutex::new(reader);
        let writer = Mutex::new(writer);

        let message_subsystem = MessageSubsystem::new();
        Self::setup_dispatchers(&message_subsystem).await;

        let channel_log = session.upgrade().unwrap().p2p().settings().channel_log;

        Arc::new(Self {
            reader,
            writer,
            address,
            message_subsystem,
            stop_subscriber: Subscriber::new(),
            receive_task: StoppableTask::new(),
            stopped: Mutex::new(false),
            info: Mutex::new(ChannelInfo::new(channel_log)),
            session,
        })
    }

    pub async fn get_info(&self) -> serde_json::Value {
        self.info.lock().await.get_info().await
    }

    /// Starts the channel. Runs a receive loop to start receiving messages or
    /// handles a network failure.
    pub fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) {
        debug!(target: "net::channel::start()", "START, address={}", self.address());
        let self2 = self.clone();
        self.receive_task.clone().start(
            self.clone().main_receive_loop(),
            |result| self2.handle_stop(result),
            Error::NetworkServiceStopped,
            executor,
        );
        debug!(target: "net::channel::start()", "END, address={}", self.address());
    }

    /// Stops the channel. Steps through each component of the channel
    /// connection and sends a stop signal. Notifies all subscribers that
    /// the channel has been closed.
    pub async fn stop(&self) {
        debug!(target: "net::channel::stop()", "START, address={}", self.address());
        if !(*self.stopped.lock().await) {
            *self.stopped.lock().await = true;

            self.stop_subscriber.notify(Error::ChannelStopped).await;
            self.receive_task.stop().await;
            self.message_subsystem.trigger_error(Error::ChannelStopped).await;
            debug!(target: "net::channel::stop()", "END, address={}", self.address());
        }
    }

    /// Creates a subscription to a stopped signal.
    /// If the channel is stopped then this will return a ChannelStopped error.
    pub async fn subscribe_stop(&self) -> Result<Subscription<Error>> {
        debug!(target: "net::channel::subscribe_stop()", "START, address={}", self.address());

        {
            let stopped = *self.stopped.lock().await;
            if stopped {
                return Err(Error::ChannelStopped)
            }
        }

        let sub = self.stop_subscriber.clone().subscribe().await;
        debug!(target: "net::channel::subscribe_stop()", "END, address={}", self.address());

        Ok(sub)
    }

    /// Sends a message across a channel. Calls function 'send_message' that
    /// creates a new payload and sends it over the TCP connection as a
    /// packet. Returns an error if something goes wrong.
    pub async fn send<M: message::Message>(&self, message: M) -> Result<()> {
        debug!(
            target: "net::channel::send()",
            "START, command={:?}, address={}",
            M::name(),
            self.address()
        );

        {
            let stopped = *self.stopped.lock().await;
            if stopped {
                return Err(Error::ChannelStopped)
            }
        }

        // Catch failure and stop channel, return a net error
        let result = match self.send_message(message).await {
            Ok(()) => Ok(()),
            Err(err) => {
                error!(target: "net::channel::send()", "Channel send error for [{}]: {}", self.address(), err);
                self.stop().await;
                Err(Error::ChannelStopped)
            }
        };

        debug!(
            target: "net::channel::send()",
            "END, command={:?}, address={}",
            M::name(),
            self.address()
        );
        {
            let info = &mut *self.info.lock().await;
            info.last_msg = M::name().to_string();
            info.last_status = "sent".to_string();
        }

        result
    }

    /// Implements send message functionality. Creates a new payload and encodes
    /// it. Then creates a message packet- the base type of the network- and
    /// copies the payload into it. Then we send the packet over the TCP
    /// stream.
    async fn send_message<M: message::Message>(&self, message: M) -> Result<()> {
        let mut payload = Vec::new();
        message.encode(&mut payload)?;
        let packet = message::Packet { command: String::from(M::name()), payload };
        let time = NanoTimestamp::current_time();
        //let time = time::unix_timestamp()?;

        {
            let info = &mut *self.info.lock().await;
            if let Some(l) = &info.log {
                l.lock().await.push((time, "send".to_string(), packet.command.clone()));
            };
        }

        let stream = &mut *self.writer.lock().await;
        message::send_packet(stream, packet).await
    }

    /// Subscribe to a messages on the message subsystem.
    pub async fn subscribe_msg<M: message::Message>(&self) -> Result<MessageSubscription<M>> {
        debug!(
            target: "net::channel::subscribe_msg()",
            "START, command={:?}, address={}",
            M::name(),
            self.address()
        );
        let sub = self.message_subsystem.subscribe::<M>().await;
        debug!(
            target: "net::channel::subscribe_msg()",
            "END, command={:?}, address={}",
            M::name(),
            self.address()
        );
        sub
    }

    /// Return the local socket address.
    pub fn address(&self) -> Url {
        self.address.clone()
    }

    pub async fn remote_node_id(&self) -> String {
        self.info.lock().await.remote_node_id.clone()
    }
    pub async fn set_remote_node_id(&self, remote_node_id: String) {
        self.info.lock().await.remote_node_id = remote_node_id;
    }

    /// End of file error. Triggered when unexpected end of file occurs.
    fn is_eof_error(err: Error) -> bool {
        match err {
            Error::Io(io_err) => io_err == std::io::ErrorKind::UnexpectedEof,
            _ => false,
        }
    }

    /// Perform network handshake for message subsystem dispatchers.
    async fn setup_dispatchers(message_subsystem: &MessageSubsystem) {
        message_subsystem.add_dispatch::<message::VersionMessage>().await;
        message_subsystem.add_dispatch::<message::VerackMessage>().await;
        message_subsystem.add_dispatch::<message::PingMessage>().await;
        message_subsystem.add_dispatch::<message::PongMessage>().await;
        message_subsystem.add_dispatch::<message::GetAddrsMessage>().await;
        message_subsystem.add_dispatch::<message::AddrsMessage>().await;
        message_subsystem.add_dispatch::<message::ExtAddrsMessage>().await;
    }

    /// Convenience function that returns the Message Subsystem.
    pub fn get_message_subsystem(&self) -> &MessageSubsystem {
        &self.message_subsystem
    }

    /// Run the receive loop. Start receiving messages or handle network
    /// failure.
    async fn main_receive_loop(self: Arc<Self>) -> Result<()> {
        debug!(target: "net::channel::main_receive_loop()", "START, address={}", self.address());

        let reader = &mut *self.reader.lock().await;

        loop {
            let packet = match message::read_packet(reader).await {
                Ok(packet) => packet,
                Err(err) => {
                    if Self::is_eof_error(err.clone()) {
                        info!(
                            target: "net::channel::main_receive_loop()",
                            "Inbound connection {} disconnected",
                            self.address()
                        );
                    } else {
                        error!(
                            target: "net::channel::main_receive_loop()",
                            "Read error on channel {}: {}",
                            self.address(),
                            err
                        );
                    }
                    debug!(
                        target: "net::channel::main_receive_loop()",
                        "Channel::receive_loop() stopping channel {}",
                        self.address()
                    );
                    self.stop().await;
                    return Err(Error::ChannelStopped)
                }
            };
            {
                let info = &mut *self.info.lock().await;
                info.last_msg = packet.command.clone();
                info.last_status = "recv".to_string();
                let time = NanoTimestamp::current_time();
                //let time = time::unix_timestamp()?;
                if let Some(l) = &info.log {
                    l.lock().await.push((time, "recv".to_string(), packet.command.clone()));
                };
            }

            // Send result to our subscribers
            self.message_subsystem.notify(&packet.command, packet.payload).await;
        }
    }

    /// Handle network errors. Panic if error passes silently, otherwise
    /// broadcast the error.
    async fn handle_stop(self: Arc<Self>, result: Result<()>) {
        debug!(
            target: "net::channel::handle_stop()",
            "START, address={}",
            self.address()
        );
        match result {
            Ok(()) => panic!("Channel task should never complete without error status"),
            Err(err) => {
                // Send this error to all channel subscribers
                self.message_subsystem.trigger_error(err).await;
            }
        }
        debug!(
            target: "net::channel::handle_stop()",
            "END, address={}",
            self.address()
        );
    }

    fn session(&self) -> Arc<dyn Session> {
        self.session.upgrade().unwrap()
    }

    pub fn session_type_id(&self) -> SessionBitflag {
        let session = self.session();
        session.type_id()
    }
}
