use async_executor::Executor;
use async_std::sync::Mutex;
use log::*;
use std::net::SocketAddr;
use std::sync::{Arc, Weak};

use crate::net::error::{NetError, NetResult};
use crate::net::protocols::{ProtocolAddress, ProtocolPing};
use crate::net::sessions::Session;
use crate::net::{ChannelPtr, Connector, P2p};
use crate::system::{StoppableTask, StoppableTaskPtr};

pub struct OutboundSession {
    p2p: Weak<P2p>,
    connect_slots: Mutex<Vec<StoppableTaskPtr>>,
}

impl OutboundSession {
    pub fn new(p2p: Weak<P2p>) -> Arc<Self> {
        Arc::new(Self {
            p2p,
            connect_slots: Mutex::new(Vec::new()),
        })
    }

    pub async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> NetResult<()> {
        let slots_count = self.p2p().settings().outbound_connections;
        info!("Starting {} outbound connection slots.", slots_count);
        let mut connect_slots = self.connect_slots.lock().await;

        for i in 0..slots_count {
            let task = StoppableTask::new();

            task.clone().start(
                self.clone().channel_connect_loop(i, executor.clone()),
                // Ignore stop handler
                |_| async {},
                NetError::ServiceStopped,
                executor.clone(),
            );

            connect_slots.push(task);
        }

        Ok(())
    }

    pub async fn stop(&self) {
        let connect_slots = &*self.connect_slots.lock().await;

        for slot in connect_slots {
            slot.stop().await;
        }
    }

    pub async fn channel_connect_loop(
        self: Arc<Self>,
        slot_number: u32,
        executor: Arc<Executor<'_>>,
    ) -> NetResult<()> {
        let connector = Connector::new(self.p2p().settings().clone());

        loop {
            let addr = self.load_address(slot_number).await?;
            info!("#{} connecting to outbound [{}]", slot_number, addr);

            match connector.connect(addr).await {
                Ok(channel) => {
                    // Blacklist goes here

                    info!("#{} connected to outbound [{}]", slot_number, addr);

                    let stop_sub = channel.subscribe_stop().await;

                    self.clone()
                        .register_channel(channel.clone(), executor.clone())
                        .await?;

                    self.clone()
                        .attach_protocols(channel, executor.clone())
                        .await?;

                    // Wait for channel to close
                    stop_sub.receive().await;
                }
                Err(err) => {
                    info!("Unable to connect to outbound [{}]: {}", addr, err);
                }
            }
        }
    }

    async fn load_address(&self, slot_number: u32) -> NetResult<SocketAddr> {
        let hosts = self.p2p().hosts();

        match hosts.load_single().await {
            Some(addr) => Ok(addr),
            None => {
                error!(
                    "Hosts address pool is empty. Closing connect slot #{}",
                    slot_number
                );
                Err(NetError::ServiceStopped)
            }
        }
    }

    async fn attach_protocols(
        self: Arc<Self>,
        channel: ChannelPtr,
        executor: Arc<Executor<'_>>,
    ) -> NetResult<()> {
        let settings = self.p2p().settings().clone();
        let hosts = self.p2p().hosts().clone();

        let protocol_ping = ProtocolPing::new(channel.clone(), settings.clone());
        let protocol_addr = ProtocolAddress::new(channel, hosts, settings).await;

        protocol_ping.start(executor.clone()).await;
        protocol_addr.start(executor).await;

        Ok(())
    }
}

impl Session for OutboundSession {
    fn p2p(&self) -> Arc<P2p> {
        self.p2p.upgrade().unwrap()
    }
}
