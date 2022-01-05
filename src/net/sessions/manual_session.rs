use async_executor::Executor;
use log::*;
use std::{
    net::SocketAddr,
    sync::{Arc, Weak},
};

use crate::error::{Error, Result};
//use crate::net::error::{Error, Result};
use crate::{
    net::{
        protocols::{ProtocolAddress, ProtocolPing},
        sessions::Session,
        Acceptor, AcceptorPtr, ChannelPtr, P2p,
    },
    system::{StoppableTask, StoppableTaskPtr},
};

pub struct ManualSession {
    p2p: Weak<P2p>,
}

impl ManualSession {
    /// Create a new inbound session.
    pub fn new(p2p: Weak<P2p>) -> Arc<Self> {
        Arc::new(Self { p2p, })
    }
    /// Starts the inbound session. Begins by accepting connections and fails if
    /// the address is not configured. Then runs the channel subscription
    /// loop.
    pub fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        Ok(())
    }

    pub fn connect(self: Arc<Self>, addr: &SocketAddr) {
    }
}

