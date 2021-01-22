use async_executor::Executor;
use log::*;
use std::net::SocketAddr;
use std::sync::{Arc, Weak};

use crate::net::error::{NetError, NetResult};
use crate::net::protocols::{ProtocolPing, ProtocolSeed};
use crate::net::sessions::Session;
use crate::net::{Acceptor, AcceptorPtr};
use crate::net::{ChannelPtr, Connector, HostsPtr, P2p, SettingsPtr};
use crate::net::utility::clone_net_error;
use crate::system::{StoppableTask, StoppableTaskPtr};

pub struct InboundSession {
    p2p: Weak<P2p>,
    acceptor: AcceptorPtr,
    accept_task: StoppableTaskPtr,
}

impl InboundSession {
    pub fn new(p2p: Weak<P2p>) -> Arc<Self> {
        let settings = {
            let p2p = p2p.upgrade().unwrap();
            p2p.settings()
        };

        let acceptor = Acceptor::new(settings);

        Arc::new(Self { p2p, acceptor, accept_task: StoppableTask::new() })
    }

    pub fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> NetResult<()> {
        match self.p2p().settings().inbound {
            Some(accept_addr) => {
                self.clone().start_accept_session(accept_addr, executor.clone())?;
            }
            None => {
                info!("Not configured for accepting incoming connections.");
                return Ok(());
            }
        }

        self.accept_task.clone().start(
            self.clone().channel_sub_loop(),
            // Ignore stop handler
            |_| { async {} },
            NetError::ServiceStopped,
            executor);

        Ok(())
    }

    pub async fn stop(&self) {
        self.acceptor.stop().await;
    }

    fn start_accept_session(
        self: Arc<Self>,
        accept_addr: SocketAddr,
        executor: Arc<Executor<'_>>,
    ) -> NetResult<()> {
        info!("Starting inbound session on {}", accept_addr);
        match self.acceptor.clone().start(accept_addr, executor) {
            Ok(()) => {
            }
            Err(err) => {
                error!("Error starting listener: {}", err);
                return Err(err);
            }
        }
        Ok(())
    }

    async fn channel_sub_loop(self: Arc<Self>) -> NetResult<()> {
        let channel_sub = self.acceptor.clone().subscribe().await;
        loop {
            //let channel = (*channel_sub.receive().await)?;
        }
    }
}

impl Session for InboundSession {
    fn p2p(&self) -> Arc<P2p> {
        self.p2p.upgrade().unwrap()
    }
}
