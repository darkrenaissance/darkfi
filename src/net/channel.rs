use async_std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use log::*;
use futures::FutureExt;
use futures::io::{ReadHalf, WriteHalf};
use futures::AsyncReadExt;
use smol::{Async, Executor};
use std::future::Future;
use std::net::{SocketAddr, TcpStream};
use std::pin::Pin;
use std::sync::Arc;

use crate::error::{Error, Result};
use crate::net::messages;
use crate::net::settings::SettingsPtr;
use crate::net::message_subscriber::{MessageSubscriberPtr, MessageSubscription, MessageSubscriber};
use crate::net::utility::clone_net_error;
use crate::system::{SubscriberPtr, Subscription, Subscriber};

pub type ChannelPtr = Arc<Channel>;

pub struct Channel {
    reader: Mutex<ReadHalf<Async<TcpStream>>>,
    writer: Mutex<WriteHalf<Async<TcpStream>>>,
    address: SocketAddr,
    message_subscriber: MessageSubscriberPtr,
    stop_subscriber: SubscriberPtr<Error>,
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

    pub async fn send(self: Arc<Self>, message: messages::Message) -> Result<()> {
        if self.stopped.load(Ordering::Relaxed) {
            return Err(Error::ChannelStopped);
        }

        // Catch failure and stop channel, return a net error
        match messages::send_message(&mut *self.writer.lock().await, message).await {
            Ok(()) => Ok(()),
            Err(err) => {
                error!("Channel error {}, closing {}", err, self.address());
                self.stop().await;
                Err(Error::ChannelStopped)
            }
        }
    }

    pub fn address(&self) -> SocketAddr {
        self.address
    }

    pub async fn subscribe_msg(self: Arc<Self>, packet_type: messages::PacketType) -> MessageSubscription {
        self.message_subscriber.clone().subscribe(packet_type).await
    }

    pub async fn subscribe_stop(self: Arc<Self>) -> Subscription<Error> {
        self.stop_subscriber.clone().subscribe().await
    }

    pub async fn stop(&self) {
        self.stopped.store(false, Ordering::Relaxed);
        let stop_err = Arc::new(Error::ChannelStopped);
        self.stop_subscriber.notify(stop_err).await;
    }

    async fn receive_loop(self: Arc<Self>) -> Result<()> {
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
                            Err(Error::ChannelStopped)
                        }
                    }
                }
                stop_err = stop_sub.receive().fuse() => {
                    Err(clone_net_error(&*stop_err))
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
