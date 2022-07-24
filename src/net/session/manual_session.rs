use async_std::sync::{Arc, Mutex, Weak};

use async_executor::Executor;
use async_trait::async_trait;
use log::*;
use serde_json::json;
use url::Url;

use crate::{
    system::{StoppableTask, StoppableTaskPtr},
    util::sleep,
    Error, Result,
};

use super::{
    super::{Connector, P2p},
    Session, SessionBitflag, SESSION_MANUAL,
};

pub struct ManualSession {
    p2p: Weak<P2p>,
    connect_slots: Mutex<Vec<StoppableTaskPtr>>,
}

impl ManualSession {
    /// Create a new inbound session.
    pub fn new(p2p: Weak<P2p>) -> Arc<Self> {
        Arc::new(Self { p2p, connect_slots: Mutex::new(Vec::new()) })
    }

    /// Stop the outbound session.
    pub async fn stop(&self) {
        let connect_slots = &*self.connect_slots.lock().await;

        for slot in connect_slots {
            slot.stop().await;
        }
    }

    pub async fn connect(self: Arc<Self>, addr: &Url, executor: Arc<Executor<'_>>) {
        let task = StoppableTask::new();

        task.clone().start(
            self.clone().channel_connect_loop(addr.clone(), executor.clone()),
            // Ignore stop handler
            |_| async {},
            Error::NetworkServiceStopped,
            executor.clone(),
        );

        self.connect_slots.lock().await.push(task);
    }

    pub async fn channel_connect_loop(
        self: Arc<Self>,
        addr: Url,
        executor: Arc<Executor<'_>>,
    ) -> Result<()> {
        let parent = Arc::downgrade(&self);
        let connector = Connector::new(self.p2p().settings(), Arc::new(parent));

        let settings = self.p2p().settings();

        let attempts = settings.manual_attempt_limit;
        let mut remaining = attempts;

        loop {
            // Loop forever if attempts is 0
            // Otherwise loop attempts number of times
            remaining = if attempts == 0 { 1 } else { remaining - 1 };
            if remaining == 0 {
                break
            }

            self.p2p().add_pending(addr.clone()).await;

            info!(target: "net", "Connecting to manual outbound [{}]", addr);

            match connector.connect(addr.clone()).await {
                Ok(channel) => {
                    // Blacklist goes here

                    info!(target: "net", "Connected to manual outbound [{}]", addr);

                    let stop_sub = channel.subscribe_stop().await;

                    if stop_sub.is_err() {
                        continue
                    }

                    self.clone().register_channel(channel.clone(), executor.clone()).await?;

                    // Channel is now connected but not yet setup

                    // Remove pending lock since register_channel will add the channel to p2p
                    self.p2p().remove_pending(&addr).await;

                    //self.clone().attach_protocols(channel, executor.clone()).await?;

                    // Wait for channel to close
                    stop_sub.unwrap().receive().await;
                }
                Err(err) => {
                    info!(target: "net", "Unable to connect to manual outbound [{}]: {}", addr, err);

                    sleep(settings.connect_timeout_seconds.into()).await;
                }
            }
        }

        warn!(
        target: "net",
        "Suspending manual connection to [{}] after {} failed attempts.",
        &addr,
        attempts
        );

        Ok(())
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

#[async_trait]
impl Session for ManualSession {
    async fn get_info(&self) -> serde_json::Value {
        json!({
            "key": 110
        })
    }

    fn p2p(&self) -> Arc<P2p> {
        self.p2p.upgrade().unwrap()
    }

    fn type_id(&self) -> SessionBitflag {
        SESSION_MANUAL
    }
}
