use async_std::{net::TcpStream, sync::Mutex};
use std::{
    net::SocketAddr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use futures::{
    io::{ReadHalf, WriteHalf},
    AsyncRead, AsyncReadExt, AsyncWrite,
};
use futures_rustls::TlsStream;
use log::{debug, error, info};
use rand::Rng;
use serde_json::json;
use smol::Executor;

use crate::{
    system::{StoppableTask, StoppableTaskPtr, Subscriber, SubscriberPtr, Subscription},
    Error, Result,
};

use super::{
    message,
    message_subscriber::{MessageSubscription, MessageSubsystem},
};

/// Atomic pointer to async channel.
pub type ChannelPtr = Arc<Channel>;

struct ChannelInfo {
    random_id: u32,
    last_msg: String,
    last_status: String,
    // Message log which is cleared on querying get_info
    log: Mutex<Vec<(String, String)>>,
}

impl ChannelInfo {
    fn new() -> Self {
        Self {
            random_id: rand::thread_rng().gen(),
            last_msg: String::new(),
            last_status: String::new(),
            log: Mutex::new(Vec::new()),
        }
    }

    async fn get_info(&self) -> serde_json::Value {
        let result = json!({
            "random_id": self.random_id,
            "last_msg": self.last_msg,
            "last_status": self.last_status,
            "log": self.log.lock().await.clone(),
        });
        self.log.lock().await.clear();
        result
    }
}

pub trait Stream: AsyncWrite + AsyncRead + Unpin + Send + Sync {}

impl Stream for TcpStream {}
impl<T: Stream> Stream for TlsStream<T> {}

/// Async channel for communication between nodes.
pub struct Channel {
    reader: Mutex<ReadHalf<Box<dyn Stream>>>,
    writer: Mutex<WriteHalf<Box<dyn Stream>>>,
    address: SocketAddr,
    message_subsystem: MessageSubsystem,
    stop_subscriber: SubscriberPtr<Error>,
    receive_task: StoppableTaskPtr,
    stopped: AtomicBool,
    info: Mutex<ChannelInfo>,
}

impl Channel {
    /// Sets up a new channel. Creates a reader and writer TCP stream and
    /// summons the message subscriber subsystem. Performs a network
    /// handshake on the subsystem dispatchers.
    pub async fn new(stream: Box<dyn Stream>, address: SocketAddr) -> Arc<Self> {
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
            info: Mutex::new(ChannelInfo::new()),
        })
    }

    pub async fn get_info(&self) -> serde_json::Value {
        self.info.lock().await.get_info().await
    }

    /// Starts the channel. Runs a receive loop to start receiving messages or
    /// handles a network failure.
    pub fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) {
        debug!(target: "net", "Channel::start() [START, address={}]", self.address());
        let self2 = self.clone();
        self.receive_task.clone().start(
            self.clone().main_receive_loop(),
            // Ignore stop handler
            |result| self2.handle_stop(result),
            Error::ServiceStopped,
            executor,
        );
        debug!(target: "net", "Channel::start() [END, address={}]", self.address());
    }

    /// Stops the channel. Steps through each component of the channel
    /// connection and sends a stop signal. Notifies all subscribers that
    /// the channel has been closed.
    pub async fn stop(&self) {
        debug!(target: "net", "Channel::stop() [START, address={}]", self.address());
        assert!(!self.stopped.load(Ordering::Relaxed));
        // Changes memory ordering to relaxed. We don't need strict thread locking here.
        self.stopped.store(false, Ordering::Relaxed);
        self.stop_subscriber.notify(Error::ChannelStopped).await;
        self.receive_task.stop().await;
        self.message_subsystem.trigger_error(Error::ChannelStopped).await;
        debug!(target: "net", "Channel::stop() [END, address={}]", self.address());
    }

    /// Creates a subscription to a stopped signal.
    pub async fn subscribe_stop(&self) -> Subscription<Error> {
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

    /// Sends a message across a channel. Calls function 'send_message' that
    /// creates a new payload and sends it over the TCP connection as a
    /// packet. Returns an error if something goes wrong.
    pub async fn send<M: message::Message>(&self, message: M) -> Result<()> {
        debug!(target: "net",
         "Channel::send() [START, command={:?}, address={}]",
         M::name(),
         self.address()
        );
        if self.stopped.load(Ordering::Relaxed) {
            return Err(Error::ChannelStopped)
        }

        // Catch failure and stop channel, return a net error
        let result = match self.send_message(message).await {
            Ok(()) => Ok(()),
            Err(err) => {
                error!("Channel send error for [{}]: {}", self.address(), err);
                self.stop().await;
                Err(Error::ChannelStopped)
            }
        };

        debug!(target: "net",
         "Channel::send() [END, command={:?}, address={}]",
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

        {
            let info = &mut *self.info.lock().await;
            info.log.lock().await.push(("send".to_string(), packet.command.clone()));
        }

        let stream = &mut *self.writer.lock().await;
        message::send_packet(stream, packet).await
    }

    /// Subscribe to a messages on the message subsystem.
    pub async fn subscribe_msg<M: message::Message>(&self) -> Result<MessageSubscription<M>> {
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
    }

    /// Convenience function that returns the Message Subsystem.
    pub fn get_message_subsystem(&self) -> &MessageSubsystem {
        &self.message_subsystem
    }

    /// Run the receive loop. Start receiving messages or handle network
    /// failure.
    async fn main_receive_loop(self: Arc<Self>) -> Result<()> {
        debug!(target: "net",
         "Channel::receive_loop() [START, address={}]",
         self.address()
        );

        let reader = &mut *self.reader.lock().await;

        loop {
            let packet = match message::read_packet(reader).await {
                Ok(packet) => packet,
                Err(err) => {
                    if Self::is_eof_error(err.clone()) {
                        info!("Channel {:?} disconnected", self.address());
                    } else {
                        error!("Read error on channel: {}", err);
                    }
                    debug!(target: "net",
                     "Channel::receive_loop() stopping channel {:?}",
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
                info.log.lock().await.push(("recv".to_string(), packet.command.clone()));
            }

            // Send result to our subscribers
            self.message_subsystem.notify(&packet.command, packet.payload).await;
        }
    }

    /// Handle network errors. Panic if error passes silently, otherwise
    /// broadcast the error.
    async fn handle_stop(self: Arc<Self>, result: Result<()>) {
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
