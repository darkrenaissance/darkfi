use futures::FutureExt;
use log::*;
use smol::{Async, Executor};
use std::net::{SocketAddr, TcpListener};
use std::sync::Arc;

use crate::error::{Error, Result};
use crate::net::{Channel, ChannelPtr, SettingsPtr};
use crate::system::{StoppableTask, StoppableTaskPtr, Subscriber, SubscriberPtr, Subscription};

pub type AcceptorPtr = Arc<Acceptor>;

pub struct Acceptor {
    channel_subscriber: SubscriberPtr<Result<ChannelPtr>>,
    task: StoppableTaskPtr,
    settings: SettingsPtr,
}

impl Acceptor {
    pub fn new(settings: SettingsPtr) -> Arc<Self> {
        Arc::new(Self {
            channel_subscriber: Subscriber::new(),
            task: StoppableTask::new(),
            settings,
        })
    }

    pub fn accept(
        self: Arc<Self>,
        accept_addr: SocketAddr,
        executor: Arc<Executor<'_>>,
    ) -> Result<()> {
        let listener = Async::<TcpListener>::bind(accept_addr)?;
        info!("Listening on {}", listener.get_ref().local_addr()?);

        // Start detached task and return instantly
        self.accept_or_stop(listener, executor);

        Ok(())
    }

    pub async fn stop(&self) {
        // Send stop signal
        self.task.stop().await;
    }

    fn accept_or_stop(self: Arc<Self>, listener: Async<TcpListener>, executor: Arc<Executor<'_>>) {
        self.task.clone().start(
            self.clone().run_accept(listener),
            |result| self.handle_stop(result),
            executor,
        );
    }

    async fn run_accept(self: Arc<Self>, listener: Async<TcpListener>) -> Result<()> {
        loop {
            match self.tick_accept(&listener).await {
                Ok(channel) => {
                    let channel_result = Arc::new(Ok(channel));
                    self.channel_subscriber.notify(channel_result).await;
                }
                Err(err) => {
                    error!("Error listening for connections: {}", err);
                    return Err(Error::ServiceStopped);
                }
            }
        }
    }

    async fn handle_stop(self: Arc<Self>, result: Result<()>) {
        match result {
            Ok(()) => panic!("Acceptor task should never complete without error status"),
            Err(err) => {
                // Send this error to all channel subscribers
                let result = Arc::new(Err(err));
                self.channel_subscriber.notify(result).await;
            }
        }
    }

    async fn tick_accept(&self, listener: &Async<TcpListener>) -> Result<ChannelPtr> {
        let (stream, peer_addr) = listener.accept().await?;
        info!("Accepted client: {}", peer_addr);

        let channel = Channel::new(stream, peer_addr, self.settings.clone());
        Ok(channel)
    }
}
