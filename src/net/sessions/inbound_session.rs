use async_executor::Executor;
use log::*;
use std::net::SocketAddr;
use std::sync::{Arc, Weak};

use crate::error::{Error, Result};
use crate::net::sessions::Session;
use crate::net::{ChannelPtr, HostsPtr, Connector, P2p, SettingsPtr};
use crate::net::protocols::{ProtocolPing, ProtocolSeed};

pub struct InboundSession {
    p2p: Weak<P2p>
}

impl InboundSession {
    pub fn new(p2p: Weak<P2p>) -> Arc<Self> {
        Arc::new(Self { p2p })
    }

    pub async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        Ok(())
    }
}

impl Session for InboundSession {
    fn p2p(&self) -> Arc<P2p> {
        self.p2p.upgrade().unwrap()
    }
}

