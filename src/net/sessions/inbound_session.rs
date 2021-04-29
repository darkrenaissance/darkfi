use async_executor::Executor;
use log::*;
use std::net::SocketAddr;
use std::sync::{Arc, Weak};

use crate::net::error::{NetError, NetResult};
use crate::net::protocols::{ProtocolAddress, ProtocolPing};
use crate::net::sessions::Session;
use crate::net::{Acceptor, AcceptorPtr};
use crate::net::{ChannelPtr, P2p};
use crate::system::{StoppableTask, StoppableTaskPtr};

/// Defines inbound connections session.
pub struct InboundSession {
    p2p: Weak<P2p>,
    acceptor: AcceptorPtr,
    accept_task: StoppableTaskPtr,
}

impl InboundSession {
    /// Create a new inbound session.
    pub fn new(p2p: Weak<P2p>) -> Arc<Self> {
        let acceptor = Acceptor::new();

        Arc::new(Self {
            p2p,
            acceptor,
            accept_task: StoppableTask::new(),
        })
    }
    /// Starts the inbound session. Begins by accepting connections and fails if
    /// the address is not configured. Then runs the channel subscription
    /// loop.
    pub fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> NetResult<()> {
        match self.p2p().settings().inbound {
            Some(accept_addr) => {
                self.clone()
                    .start_accept_session(accept_addr, executor.clone())?;
            }
            None => {
                info!("Not configured for accepting incoming connections.");
                return Ok(());
            }
        }

        self.accept_task.clone().start(
            self.clone().channel_sub_loop(executor.clone()),
            // Ignore stop handler
            |_| async {},
            NetError::ServiceStopped,
            executor,
        );

        Ok(())
    }
    /// Stops the inbound session.
    pub async fn stop(&self) {
        self.acceptor.stop().await;
        self.accept_task.stop().await;
    }
    /// Start accepting connections for inbound session.
    fn start_accept_session(
        self: Arc<Self>,
        accept_addr: SocketAddr,
        executor: Arc<Executor<'_>>,
    ) -> NetResult<()> {
        info!("Starting inbound session on {}", accept_addr);
        let result = self.acceptor.clone().start(accept_addr, executor);
        if let Err(err) = result {
            error!("Error starting listener: {}", err);
        }
        result
    }

    /// Wait for all new channels created by the acceptor and call
    /// setup_channel() on them.
    async fn channel_sub_loop(self: Arc<Self>, executor: Arc<Executor<'_>>) -> NetResult<()> {
        let channel_sub = self.acceptor.clone().subscribe().await;
        loop {
            let channel = channel_sub.receive().await?;
            // Spawn a detached task to process the channel
            // This will just perform the channel setup then exit.
            executor
                .spawn(self.clone().setup_channel(channel, executor.clone()))
                .detach();
        }
    }

    /// Registers the channel. First performs a network handshake and starts the
    /// channel. Then starts sending keep-alive and address messages across the
    /// channel.
    async fn setup_channel(
        self: Arc<Self>,
        channel: ChannelPtr,
        executor: Arc<Executor<'_>>,
    ) -> NetResult<()> {
        info!("Connected inbound [{}]", channel.address());

        self.clone()
            .register_channel(channel.clone(), executor.clone())
            .await?;

        self.attach_protocols(channel, executor).await
    }

    /// Starts sending keep-alive and address messages across the channels.
    async fn attach_protocols(
        self: Arc<Self>,
        channel: ChannelPtr,
        executor: Arc<Executor<'_>>,
    ) -> NetResult<()> {
        let settings = self.p2p().settings().clone();
        let hosts = self.p2p().hosts().clone();

        let protocol_ping = ProtocolPing::new(channel.clone(), settings.clone());
        let protocol_addr = ProtocolAddress::new(channel, hosts).await;

        protocol_ping.start(executor.clone()).await;
        protocol_addr.start(executor).await;

        Ok(())
    }
}

impl Session for InboundSession {
    fn p2p(&self) -> Arc<P2p> {
        self.p2p.upgrade().unwrap()
    }
}
