use async_std::future::timeout;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::{
    net::SocketAddr,
    sync::{Arc, Weak},
    time::Duration,
};

use async_executor::Executor;
use log::*;

use crate::{
    error::{Error, Result},
    net::{
        session::{Session, SessionBitflag, SESSION_SEED},
        Connector, P2p,
    },
};

/// Defines seed connections session.
pub struct SeedSession {
    p2p: Weak<P2p>,
}

impl SeedSession {
    /// Create a new seed session instance.
    pub fn new(p2p: Weak<P2p>) -> Arc<Self> {
        Arc::new(Self { p2p })
    }

    /// Start the seed session. Creates a new task for every seed connection and
    /// starts the seed on each task.
    pub async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        debug!(target: "net", "SeedSession::start() [START]");
        let settings = self.p2p().settings();

        if settings.seeds.is_empty() {
            warn!("Skipping seed sync process since no seeds are configured.");
            return Ok(())
        }

        // if cached addresses then quit

        let mut tasks = Vec::new();

        for (i, seed) in settings.seeds.iter().enumerate() {
            tasks.push(executor.spawn(self.clone().start_seed(i, *seed, executor.clone())));
        }

        // This line loops through all the tasks and waits for them to finish.
        // But if the seed_query_timeout_seconds times out before they are finished,
        // then it will simply quit and the tasks will get dropped.
        let result =
            timeout(Duration::from_secs(settings.seed_query_timeout_seconds.into()), async move {
                for (i, task) in tasks.into_iter().enumerate() {
                    // Ignore errors
                    match task.await {
                        Ok(()) => info!("Successfully queried seed #{}", i),
                        Err(err) => warn!("Seed query #{} failed for reason: {}", i, err),
                    }
                }
            })
            .await;
        match result {
            Ok(_) => {}
            Err(_) => {
                error!("Querying seeds timed out");
                return Err(Error::OperationFailed)
            }
        }

        // Seed process complete
        if self.p2p().hosts().is_empty().await {
            error!("Hosts pool still empty after seeding");
            return Err(Error::OperationFailed)
        }

        debug!(target: "net", "SeedSession::start() [END]");
        Ok(())
    }

    /// Connects to a seed socket address. Registers a new channel with a
    /// network handshake, then starts the keep-alive messages and seed
    /// protocol.
    async fn start_seed(
        self: Arc<Self>,
        seed_index: usize,
        seed: SocketAddr,
        executor: Arc<Executor<'_>>,
    ) -> Result<()> {
        debug!(target: "net", "SeedSession::start_seed(i={}) [START]", seed_index);
        let (_hosts, settings) = {
            let p2p = self.p2p.upgrade().unwrap();
            (p2p.hosts(), p2p.settings())
        };

        let connector = Connector::new(settings.clone());
        match connector.connect(seed).await {
            Ok(channel) => {
                // Blacklist goes here

                info!("Connected seed #{} [{}]", seed_index, seed);

                self.clone().register_channel(channel.clone(), executor.clone()).await?;

                //self.attach_protocols(channel, hosts, settings, executor).await?;

                debug!(target: "net", "SeedSession::start_seed(i={}) [END]", seed_index);
                Ok(())
            }
            Err(err) => {
                info!("Failure contacting seed #{} [{}]: {}", seed_index, seed, err);
                Err(err)
            }
        }
    }

    // Starts keep-alive messages and seed protocol.
    /*async fn attach_protocols(
        self: Arc<Self>,
        channel: ChannelPtr,
        hosts: HostsPtr,
        settings: SettingsPtr,
        executor: Arc<Executor<'_>>,
    ) -> Result<()> {
        let protocol_ping = ProtocolPing::new(channel.clone(), self.p2p());
        protocol_ping.start(executor.clone()).await;

        let protocol_seed = ProtocolSeed::new(channel.clone(), hosts, settings.clone());
        // This will block until seed process is complete
        protocol_seed.start(executor.clone()).await?;

        channel.stop().await;

        Ok(())
    }*/
}

#[async_trait]
impl Session for SeedSession {
    async fn get_info(&self) -> serde_json::Value {
        json!({
            "key": 110
        })
    }

    fn p2p(&self) -> Arc<P2p> {
        self.p2p.upgrade().unwrap()
    }

    fn selector_id(&self) -> SessionBitflag {
        SESSION_SEED
    }
}
