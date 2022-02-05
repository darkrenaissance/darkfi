use async_executor::Executor;
use async_std::{sync::Mutex, task::yield_now};
use log::*;
use std::{
    net::SocketAddr,
    sync::{Arc, Weak},
};

use crate::{
    error::{Error, Result},
    net::{
        protocol::{ProtocolAddress, ProtocolBase, ProtocolPing},
        session::{Session, SessionBitflag, SESSION_OUTBOUND},
        ChannelPtr, Connector, P2p,
    },
    system::{StoppableTask, StoppableTaskPtr},
};

/// Defines outbound connections session.
pub struct OutboundSession {
    p2p: Weak<P2p>,
    connect_slots: Mutex<Vec<StoppableTaskPtr>>,
}

impl OutboundSession {
    /// Create a new outbound session.
    pub fn new(p2p: Weak<P2p>) -> Arc<Self> {
        Arc::new(Self { p2p, connect_slots: Mutex::new(Vec::new()) })
    }
    /// Start the outbound session. Runs the channel connect loop.
    pub async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        let slots_count = self.p2p().settings().outbound_connections;
        info!(target: "net", "Starting {} outbound connection slots.", slots_count);
        // Activate mutex lock on connection slots.
        let mut connect_slots = self.connect_slots.lock().await;

        for i in 0..slots_count {
            let task = StoppableTask::new();

            task.clone().start(
                self.clone().channel_connect_loop(i, executor.clone()),
                // Ignore stop handler
                |_| async {},
                Error::ServiceStopped,
                executor.clone(),
            );

            connect_slots.push(task);
        }

        Ok(())
    }

    /// Stop the outbound session.
    pub async fn stop(&self) {
        let connect_slots = &*self.connect_slots.lock().await;

        for slot in connect_slots {
            slot.stop().await;
        }
    }

    /// Start making outbound connections. Creates a connector object, then
    /// starts a connect loop. Loads a valid address then tries to connect.
    /// Once connected, registers the channel, removes it from the list of
    /// pending channels, and starts sending messages across the channel.
    /// Otherwise returns a network error.
    pub async fn channel_connect_loop(
        self: Arc<Self>,
        slot_number: u32,
        executor: Arc<Executor<'_>>,
    ) -> Result<()> {
        let connector = Connector::new(self.p2p().settings());

        loop {
            let addr = self.load_address(slot_number).await?;
            info!(target: "net", "#{} connecting to outbound [{}]", slot_number, addr);

            match connector.connect(addr).await {
                Ok(channel) => {
                    // Blacklist goes here

                    info!(target: "net", "#{} connected to outbound [{}]", slot_number, addr);

                    let stop_sub = channel.subscribe_stop().await;

                    self.clone().register_channel(channel.clone(), executor.clone()).await?;

                    // Channel is now connected but not yet setup

                    // Remove pending lock since register_channel will add the channel to p2p
                    self.p2p().remove_pending(&addr).await;

                    //self.clone().attach_protocols(channel, executor.clone()).await?;

                    // Wait for channel to close
                    stop_sub.receive().await;
                }
                Err(err) => {
                    info!(target: "net", "Unable to connect to outbound [{}]: {}", addr, err);
                }
            }
        }
    }

    /// Loops through host addresses to find a outbound address that we can
    /// connect to. Checks whether address is valid by making sure it isn't
    /// our own inbound address, then checks whether it is already connected
    /// (exists) or connecting (pending). Keeps looping until address is
    /// found that passes all checks.
    async fn load_address(&self, slot_number: u32) -> Result<SocketAddr> {
        let p2p = self.p2p();
        let hosts = p2p.hosts();
        let self_inbound_addr = p2p.settings().external_addr;

        loop {
            yield_now().await;

            let addr = hosts.load_single().await;

            if addr.is_none() {
                error!(target: "net", "Hosts address pool is empty. Closing connect slot #{}", slot_number);
                return Err(Error::ServiceStopped)
            }
            let addr = addr.unwrap();

            if Self::is_self_inbound(&addr, &self_inbound_addr) {
                continue
            }

            if p2p.exists(&addr).await {
                continue
            }

            // Obtain a lock on this address to prevent duplicate connections
            if !p2p.add_pending(addr).await {
                continue
            }

            return Ok(addr)
        }
    }

    /// Checks whether an address is our own inbound address to avoid connecting
    /// to ourselves.
    fn is_self_inbound(addr: &SocketAddr, inbound_addr: &Option<SocketAddr>) -> bool {
        match inbound_addr {
            Some(inbound_addr) => inbound_addr == addr,
            // No inbound listening address configured
            None => false,
        }
    }

    // Starts sending keep-alive and address messages across the channels.
    /*async fn attach_protocols(
        self: Arc<Self>,
        channel: ChannelPtr,
        executor: Arc<Executor<'_>>,
    ) -> Result<()> {
        let hosts = self.p2p().hosts();

        let protocol_ping = ProtocolPing::new(channel.clone(), self.p2p());
        let protocol_addr = ProtocolAddress::new(channel, hosts).await;

        protocol_ping.start(executor.clone()).await;
        protocol_addr.start(executor).await;

        Ok(())
    }*/
}

impl Session for OutboundSession {
    fn p2p(&self) -> Arc<P2p> {
        self.p2p.upgrade().unwrap()
    }

    fn selector_id(&self) -> SessionBitflag {
        SESSION_OUTBOUND
    }
}
