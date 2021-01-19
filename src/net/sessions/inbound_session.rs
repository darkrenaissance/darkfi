use async_executor::Executor;
use log::*;
use std::net::SocketAddr;
use std::sync::{Arc, Weak};

use crate::error::{Error, Result};
use crate::net::protocols::{ProtocolPing, ProtocolSeed};
use crate::net::sessions::Session;
use crate::net::{Acceptor, AcceptorPtr};
use crate::net::{ChannelPtr, Connector, HostsPtr, P2p, SettingsPtr};

pub struct InboundSession {
    p2p: Weak<P2p>,
    acceptor: AcceptorPtr,
}

impl InboundSession {
    pub fn new(p2p: Weak<P2p>) -> Arc<Self> {
        let settings = {
            let p2p = p2p.upgrade().unwrap();
            p2p.settings()
        };

        let acceptor = Acceptor::new(settings);

        Arc::new(Self { p2p, acceptor })
    }

    pub async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        match self.p2p().settings().inbound {
            Some(accept_addr) => {
                self.start_accept_session(accept_addr, executor).await?;
            }
            None => {
                info!("Not configured for accepting incoming connections.");
            }
        }

        Ok(())
    }

    pub async fn stop(&self) {
        self.acceptor.stop().await;
    }

    async fn start_accept_session(
        self: Arc<Self>,
        accept_addr: SocketAddr,
        executor: Arc<Executor<'_>>,
    ) -> Result<()> {
        info!("Starting inbound session on {}", accept_addr);
        match self.acceptor.clone().accept(accept_addr, executor) {
            Ok(()) => {}
            Err(err) => {
                error!("Error starting listener: {}", err);
                return Err(err);
            }
        }
        Ok(())
    }
}

impl Session for InboundSession {
    fn p2p(&self) -> Arc<P2p> {
        self.p2p.upgrade().unwrap()
    }
}
