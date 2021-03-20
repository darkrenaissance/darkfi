use async_std::sync::Mutex;
use futures::io::{ReadHalf, WriteHalf};
use futures::AsyncReadExt;
use log::*;
use smol::{Async, Executor};

use std::net::{SocketAddr, TcpStream};

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::error;
use crate::net::error::{NetError, NetResult};
use crate::net::message_subscriber::{MessageSubscription, MessageSubsystem};
use crate::net::messages;
use crate::system::{StoppableTask, StoppableTaskPtr, Subscriber, SubscriberPtr, Subscription};

/// Atomic pointer to async channel.
pub type ChannelPtr = Arc<Channel>;

/// Async TCP channel that handles the sending of messages across the network.
pub struct Channel {
    reader: Mutex<ReadHalf<Async<TcpStream>>>,
    writer: Mutex<WriteHalf<Async<TcpStream>>>,
    address: SocketAddr,
    message_subsystem: MessageSubsystem,
    stop_subscriber: SubscriberPtr<NetError>,
    receive_task: StoppableTaskPtr,
    stopped: AtomicBool,
}

impl Channel {
    /// Create a new channel.
    pub async fn new(
        stream: Async<TcpStream>,
        address: SocketAddr,
    ) -> Arc<Self> {
        let (reader, writer) = stream.split();
        let reader = Mutex::new(reader);
        let writer = Mutex::new(writer);

        let message_subsystem = MessageSubsystem::new();
        Self::setup_dispatchers(&message_subsystem).await;

        Arc::new(Self {
            reader,
            writer,
            address,
            message_subsystem,
            stop_subscriber: Subscriber::new(),
            receive_task: StoppableTask::new(),
            stopped: AtomicBool::new(false),
        })
    }
    
    /// Start the channel.
    pub fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) {
        debug!(target: "net", "Channel::start() [START, address={}]", self.address());
        let self2 = self.clone();
        self.receive_task.clone().start(
            self.clone().main_receive_loop(),
            // Ignore stop handler
            |result| self2.handle_stop(result),
            NetError::ServiceStopped,
            executor,
        );
        debug!(target: "net", "Channel::start() [END, address={}]", self.address());
    }
    
    /// Stop the channel.
    pub async fn stop(&self) {
        debug!(target: "net", "Channel::stop() [START, address={}]", self.address());
        assert_eq!(self.stopped.load(Ordering::Relaxed), false);
        self.stopped.store(false, Ordering::Relaxed);
        self.stop_subscriber.notify(NetError::ChannelStopped).await;
        self.receive_task.stop().await;
        self.message_subsystem.trigger_error(NetError::ChannelStopped).await;
        debug!(target: "net", "Channel::stop() [END, address={}]", self.address());
    }

    /// Stop the channel and create a new sub.
    pub async fn subscribe_stop(&self) -> Subscription<NetError> {
        debug!(target: "net",
            "Channel::subscribe_stop() [START, address={}]",
            self.address()
        );
        // TODO: this should check the stopped status
        // Call to receive should return ChannelStopped on newly created sub
        let sub = self.stop_subscriber.clone().subscribe().await;
        debug!(target: "net",
            "Channel::subscribe_stop() [END, address={}]",
            self.address()
        );
        sub
    }
    
    /// Send a message across a channel. 
    pub async fn send<M: messages::Message>(&self, message: M) -> NetResult<()> {
        debug!(target: "net",
            "Channel::send() [START, command={:?}, address={}]",
            M::name(),
            self.address()
        );
        if self.stopped.load(Ordering::Relaxed) {
            return Err(NetError::ChannelStopped);
        }

        // Catch failure and stop channel, return a net error
        let result = match self.send_message(message).await {
            Ok(()) => Ok(()),
            Err(err) => {
                error!("Channel send error for [{}]: {}", self.address(), err);
                self.stop().await;
                Err(NetError::ChannelStopped)
            }
        };
        debug!(target: "net",
            "Channel::send() [END, command={:?}, address={}]",
            M::name(),
            self.address()
        );
        result
    }
    
    /// Implements send message functionality.
    async fn send_message<M: messages::Message>(&self, message: M) -> error::Result<()> {
        let mut payload = Vec::new();
        message.encode(&mut payload)?;
        let packet = messages::Packet {
            command: String::from(M::name()),
            payload,
        };

        let stream = &mut *self.writer.lock().await;
        messages::send_packet(stream, packet).await
    }

    /// Subscribe to a message type.
    pub async fn subscribe_msg<M: messages::Message>(&self) -> NetResult<MessageSubscription<M>> {
        debug!(target: "net",
            "Channel::subscribe_msg() [START, command={:?}, address={}]",
            M::name(),
            self.address()
        );
        let sub = self.message_subsystem.subscribe::<M>().await;
        debug!(target: "net",
            "Channel::subscribe_msg() [END, command={:?}, address={}]",
            M::name(),
            self.address()
        );
        sub
    }
    
    /// Return the local socket address.
    pub fn address(&self) -> SocketAddr {
        self.address
    }
    
    /// End of file error. Triggered when unexpected end of file occurs.
    fn is_eof_error(err: &error::Error) -> bool {
        match err {
            error::Error::Io(io_err) => io_err.kind() == std::io::ErrorKind::UnexpectedEof,
            _ => false,
        }
    }

    /// Perform network handshake for message subsystem dispatchers.
    async fn setup_dispatchers(message_subsystem: &MessageSubsystem) {
        message_subsystem
            .add_dispatch::<messages::VersionMessage>()
            .await;
        message_subsystem
            .add_dispatch::<messages::VerackMessage>()
            .await;
        message_subsystem
            .add_dispatch::<messages::PingMessage>()
            .await;
        message_subsystem
            .add_dispatch::<messages::PongMessage>()
            .await;
        message_subsystem
            .add_dispatch::<messages::GetAddrsMessage>()
            .await;
        message_subsystem
            .add_dispatch::<messages::AddrsMessage>()
            .await;
    }
    
    pub fn get_message_subsystem(&self) -> &MessageSubsystem {
        &self.message_subsystem
    }

    /// Run the receive loop. Start receiving messages or handle network failure.
    async fn main_receive_loop(self: Arc<Self>) -> NetResult<()> {
        debug!(target: "net",
            "Channel::receive_loop() [START, address={}]",
            self.address()
        );

        let reader = &mut *self.reader.lock().await;

        loop {
            let packet = match messages::read_packet(reader).await {
                Ok(packet) => packet,
                Err(err) => {
                    if Self::is_eof_error(&err) {
                        info!("Channel {} disconnected", self.address());
                    } else {
                        error!("Read error on channel: {}", err);
                    }
                    debug!(target: "net",
                        "Channel::receive_loop() stopping channel {}",
                        self.address()
                    );
                    self.stop().await;
                    return Err(NetError::ChannelStopped);
                }
            };

            // Send result to our subscribers
            self.message_subsystem
                .notify(&packet.command, packet.payload)
                .await;
        }
    }

    /// Handle network errors. Panic if error passes silently, otherwise broadcast the error.
    async fn handle_stop(self: Arc<Self>, result: NetResult<()>) {
        debug!(target: "net", "Channel::handle_stop() [START, address={}]", self.address());
        match result {
            Ok(()) => panic!("Channel task should never complete without error status"),
            Err(err) => {
                // Send this error to all channel subscribers
                self.message_subsystem.trigger_error(err).await;
            }
        }
        debug!(target: "net", "Channel::handle_stop() [END, address={}]", self.address());
    }
}
