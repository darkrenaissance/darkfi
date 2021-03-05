use async_executor::Executor;
use futures::FutureExt;
use log::*;
use std::net::SocketAddr;
use std::sync::{Arc, Weak};

use crate::net::error::{NetError, NetResult};
use crate::net::protocols::{ProtocolPing, ProtocolSeed};
use crate::net::sessions::Session;
use crate::net::utility::sleep;
use crate::net::{ChannelPtr, Connector, HostsPtr, P2p, SettingsPtr};

pub struct SeedSession {
    p2p: Weak<P2p>,
}

impl SeedSession {
    pub fn new(p2p: Weak<P2p>) -> Arc<Self> {
        Arc::new(Self { p2p })
    }

    pub async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> NetResult<()> {
        debug!(target: "net", "SeedSession::start() [START]");
        let settings = self.p2p().settings();

        if settings.seeds.is_empty() {
            warn!("Skipping seed sync process since no seeds are configured.");
            return Ok(());
        }

        // if cached addresses then quit

        let mut tasks = Vec::new();

        for (i, seed) in settings.seeds.iter().enumerate() {
            tasks.push(executor.spawn(self.clone().start_seed(i, seed.clone(), executor.clone())));
        }

        futures::select! {
            _ = async move {
                for (i, task) in tasks.into_iter().enumerate() {
                    // Ignore errors
                    match task.await {
                        Ok(()) => info!("Successfully queried seed #{}", i),
                        Err(err) => warn!("Seed query #{} failed for reason: {}", i, err),
                    }
                }
            }.fuse() => {
            }
            _ = sleep(settings.seed_query_timeout_seconds).fuse() => {
                error!("Querying seeds timed out");
                return Err(NetError::OperationFailed);
            }
        }

        // Seed process complete
        if self.p2p().hosts().is_empty().await {
            error!("Hosts pool still empty after seeding");
            return Err(NetError::OperationFailed);
        }

        debug!(target: "net", "SeedSession::start() [END]");
        Ok(())
    }

    async fn start_seed(
        self: Arc<Self>,
        seed_index: usize,
        seed: SocketAddr,
        executor: Arc<Executor<'_>>,
    ) -> NetResult<()> {
        debug!(target: "net", "SeedSession::start_seed(i={}) [START]", seed_index);
        let (hosts, settings) = {
            let p2p = self.p2p.upgrade().unwrap();
            (p2p.hosts(), p2p.settings())
        };

        let connector = Connector::new(settings.clone());
        match connector.connect(seed).await {
            Ok(channel) => {
                // Blacklist goes here

                info!("Connected seed #{} [{}]", seed_index, seed);

                self.clone()
                    .register_channel(channel.clone(), executor.clone())
                    .await?;

                self.attach_protocols(channel, hosts, settings, executor)
                    .await?;

                debug!(target: "net", "SeedSession::start_seed(i={}) [END]", seed_index);
                Ok(())
            }
            Err(err) => {
                info!(
                    "Failure contacting seed #{} [{}]: {}",
                    seed_index, seed, err
                );
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
        // This will block until seed process is complete
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
