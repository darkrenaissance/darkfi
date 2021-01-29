use async_executor::Executor;
use log::*;
use std::net::SocketAddr;
use std::sync::{Arc, Weak};

use crate::net::error::{NetError, NetResult};
use crate::net::protocols::{ProtocolPing, ProtocolSeed};
use crate::net::sessions::Session;
use crate::net::{ChannelPtr, Connector, HostsPtr, P2p, SettingsPtr};

pub struct SeedSession {
    p2p: Weak<P2p>,
}

impl SeedSession {
    pub fn new(p2p: Weak<P2p>) -> Arc<Self> {
        Arc::new(Self { p2p })
    }

    pub async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> NetResult<()> {
        let settings = {
            let p2p = self.p2p.upgrade().unwrap();
            p2p.settings()
        };

        if settings.skip_seed_sync {
            info!("Configured to skip seed synchronization process.");
            return Ok(());
        }

        // if cached addresses then quit

        // if seeds empty then seeding required but empty
        if settings.seeds.is_empty() {
            error!("Seeding is required but no seeds are configured.");
            return Err(NetError::OperationFailed);
        }

        let mut tasks = Vec::new();

        for seed in settings.seeds.clone() {
            tasks.push(executor.spawn(self.clone().start_seed(seed, executor.clone())));
        }

        for task in tasks {
            // Ignore errors
            let _ = task.await;
        }

        // Seed process complete
        // TODO: check increase count of address

        Ok(())
    }

    async fn start_seed(
        self: Arc<Self>,
        seed: SocketAddr,
        executor: Arc<Executor<'_>>,
    ) -> NetResult<()> {
        let (hosts, settings) = {
            let p2p = self.p2p.upgrade().unwrap();
            (p2p.hosts(), p2p.settings())
        };

        let connector = Connector::new(settings.clone());
        match connector.connect(seed).await {
            Ok(channel) => {
                // Blacklist goes here

                info!("Connected seed [{}]", seed);

                self.clone()
                    .register_channel(channel.clone(), executor.clone())
                    .await?;

                self.attach_protocols(channel, hosts, settings, executor)
                    .await
            }
            Err(err) => {
                info!("Failure contacting seed [{}]: {}", seed, err);
                Err(err)
            }
        }
    }

    async fn attach_protocols(
        self: Arc<Self>,
        channel: ChannelPtr,
        hosts: HostsPtr,
        settings: SettingsPtr,
        executor: Arc<Executor<'_>>,
    ) -> NetResult<()> {
        let protocol_ping = ProtocolPing::new(channel.clone(), settings.clone());
        protocol_ping.start(executor.clone()).await;

        let protocol_seed = ProtocolSeed::new(channel.clone(), hosts, settings.clone());
        protocol_seed.start(executor.clone()).await?;

        channel.stop().await;

        Ok(())
    }
}

impl Session for SeedSession {
    fn p2p(&self) -> Arc<P2p> {
        self.p2p.upgrade().unwrap()
    }
}
