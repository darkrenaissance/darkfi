use log::*;
use smol::{Async, Executor};
use std::net::{SocketAddr, TcpListener};
use std::sync::Arc;

use crate::net::error::{NetError, NetResult};
use crate::net::{Channel, ChannelPtr, SettingsPtr};
use crate::system::{StoppableTask, StoppableTaskPtr, Subscriber, SubscriberPtr, Subscription};

pub type AcceptorPtr = Arc<Acceptor>;

pub struct Acceptor {
    channel_subscriber: SubscriberPtr<NetResult<ChannelPtr>>,
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

    pub fn start(
        self: Arc<Self>,
        accept_addr: SocketAddr,
        executor: Arc<Executor<'_>>,
    ) -> NetResult<()> {
        let listener = Self::setup(accept_addr)?;

        // Start detached task and return instantly
        self.accept(listener, executor);

        Ok(())
    }

    pub async fn stop(&self) {
        // Send stop signal
        self.task.stop().await;
    }

    pub async fn subscribe(self: Arc<Self>) -> Subscription<NetResult<ChannelPtr>> {
        self.channel_subscriber.clone().subscribe().await
    }

    fn setup(accept_addr: SocketAddr) -> NetResult<Async<TcpListener>> {
        let listener = match Async::<TcpListener>::bind(accept_addr) {
            Ok(l) => l,
            Err(err) => {
                error!("Bind listener failed: {}", err);
                return Err(NetError::OperationFailed);
            }
        };
        let local_addr = match listener.get_ref().local_addr() {
            Ok(a) => a,
            Err(err) => {
                error!("Failed to get local address: {}", err);
                return Err(NetError::OperationFailed);
            }
        };
        info!("Listening on {}", local_addr);

        Ok(listener)
    }

    fn accept(self: Arc<Self>, listener: Async<TcpListener>, executor: Arc<Executor<'_>>) {
        self.task.clone().start(
            self.clone().run_accept_loop(listener),
            |result| self.handle_stop(result),
            NetError::ServiceStopped,
            executor,
        );
    }

    async fn run_accept_loop(self: Arc<Self>, listener: Async<TcpListener>) -> NetResult<()> {
        loop {
            let channel = self.tick_accept(&listener).await?;
            let channel_result = Arc::new(Ok(channel));
            self.channel_subscriber.notify(channel_result).await;
        }
    }

    async fn handle_stop(self: Arc<Self>, result: NetResult<()>) {
        match result {
            Ok(()) => panic!("Acceptor task should never complete without error status"),
            Err(err) => {
                // Send this error to all channel subscribers
                let result = Arc::new(Err(err));
                self.channel_subscriber.notify(result).await;
            }
        }
    }

    async fn tick_accept(&self, listener: &Async<TcpListener>) -> NetResult<ChannelPtr> {
        let (stream, peer_addr) = match listener.accept().await {
            Ok((s, a)) => (s, a),
            Err(err) => {
                error!("Error listening for connections: {}", err);
                return Err(NetError::ServiceStopped);
            }
        };
        info!("Accepted client: {}", peer_addr);

        let channel = Channel::new(stream, peer_addr, self.settings.clone());
        Ok(channel)
    }
}
