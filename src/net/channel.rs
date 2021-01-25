use async_std::sync::Mutex;
use futures::io::{ReadHalf, WriteHalf};
use futures::AsyncReadExt;
use futures::FutureExt;
use log::*;
use smol::{Async, Executor};

use std::net::{SocketAddr, TcpStream};

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::net::error::{NetError, NetResult};
use crate::net::message_subscriber::{
    MessageSubscriber, MessageSubscriberPtr, MessageSubscription,
};
use crate::net::messages;
use crate::net::settings::SettingsPtr;
use crate::system::{Subscriber, SubscriberPtr, Subscription};

pub type ChannelPtr = Arc<Channel>;

pub struct Channel {
    reader: Mutex<ReadHalf<Async<TcpStream>>>,
    writer: Mutex<WriteHalf<Async<TcpStream>>>,
    address: SocketAddr,
    message_subscriber: MessageSubscriberPtr,
    stop_subscriber: SubscriberPtr<NetError>,
    stopped: AtomicBool,
    settings: SettingsPtr,
}

impl Channel {
    pub fn new(stream: Async<TcpStream>, address: SocketAddr, settings: SettingsPtr) -> Arc<Self> {
        let (reader, writer) = stream.split();
        let reader = Mutex::new(reader);
        let writer = Mutex::new(writer);
        Arc::new(Self {
            reader,
            writer,
            address,
            message_subscriber: MessageSubscriber::new(),
            stop_subscriber: Subscriber::new(),
            stopped: AtomicBool::new(false),
            settings,
        })
    }

    pub fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) {
        executor.spawn(self.receive_loop()).detach();
    }

    pub async fn send(self: Arc<Self>, message: messages::Message) -> NetResult<()> {
        if self.stopped.load(Ordering::Relaxed) {
            return Err(NetError::ChannelStopped);
        }

        // Catch failure and stop channel, return a net error
        match messages::send_message(&mut *self.writer.lock().await, message).await {
            Ok(()) => Ok(()),
            Err(err) => {
                error!("Channel error {}, closing {}", err, self.address());
                self.stop().await;
                Err(NetError::ChannelStopped)
            }
        }
    }

    pub fn address(&self) -> SocketAddr {
        self.address
    }

    pub async fn subscribe_msg(
        self: Arc<Self>,
        packet_type: messages::PacketType,
    ) -> MessageSubscription {
        self.message_subscriber.clone().subscribe(packet_type).await
    }

    pub async fn subscribe_stop(self: Arc<Self>) -> Subscription<NetError> {
        self.stop_subscriber.clone().subscribe().await
    }

    pub async fn stop(&self) {
        self.stopped.store(false, Ordering::Relaxed);
        let stop_err = Arc::new(NetError::ChannelStopped);
        self.stop_subscriber.notify(stop_err).await;
    }

    async fn receive_loop(self: Arc<Self>) -> NetResult<()> {
        let stop_sub = self.clone().subscribe_stop().await;
        let reader = &mut *self.reader.lock().await;

        loop {
            let message_result = futures::select! {
                message_result = messages::receive_message(reader).fuse() => {
                    match message_result {
                        Ok(message) => Ok(Arc::new(message)),
                        Err(err) => {
                            error!("Read error on channel {}", err);
                            self.stop().await;
                            Err(NetError::ChannelStopped)
                        }
                    }
                }
                stop_err = stop_sub.receive().fuse() => {
                    Err(*stop_err)
                }
            };

            // Save status before using the message
            let stopped = message_result.is_err();

            // Send result to our subscribers
            self.message_subscriber.notify(message_result).await;

            // If channel is stopped, timed out or any other error then terminate loop.
            if stopped {
                break;
            }
        }
        Ok(())
    }
}
