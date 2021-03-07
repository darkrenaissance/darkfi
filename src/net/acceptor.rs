use log::*;
use smol::{Async, Executor};
use std::net::{SocketAddr, TcpListener};
use std::sync::Arc;

use crate::net::error::{NetError, NetResult};
use crate::net::{Channel, ChannelPtr};
use crate::system::{StoppableTask, StoppableTaskPtr, Subscriber, SubscriberPtr, Subscription};

pub type AcceptorPtr = Arc<Acceptor>;

pub struct Acceptor {
    channel_subscriber: SubscriberPtr<NetResult<ChannelPtr>>,
    task: StoppableTaskPtr,
}

impl Acceptor {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            channel_subscriber: Subscriber::new(),
            task: StoppableTask::new(),
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
            Ok(listener) => listener,
            Err(err) => {
                error!("Bind listener failed: {}", err);
                return Err(NetError::OperationFailed);
            }
        };
        let local_addr = match listener.get_ref().local_addr() {
            Ok(addr) => addr,
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
            self.channel_subscriber.notify(Ok(channel)).await;
        }
    }

    async fn handle_stop(self: Arc<Self>, result: NetResult<()>) {
        match result {
            Ok(()) => panic!("Acceptor task should never complete without error status"),
            Err(err) => {
                // Send this error to all channel subscribers
                let result = Err(err);
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

        let channel = Channel::new(stream, peer_addr).await;
        Ok(channel)
    }
}
