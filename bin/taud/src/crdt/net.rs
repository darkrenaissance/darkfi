use async_std::sync::{Arc, Mutex};

use async_executor::Executor;
use async_trait::async_trait;
use log::debug;

use darkfi::{net, Result};

use super::{Event, GSet};

pub struct ProtocolCrdt {
    jobsman: net::ProtocolJobsManagerPtr,
    notify_queue_sender: async_channel::Sender<Event>,
    event_sub: net::MessageSubscription<Event>,
    p2p: net::P2pPtr,
    gset: Arc<Mutex<GSet<Event>>>,
}

impl ProtocolCrdt {
    pub async fn init(
        channel: net::ChannelPtr,
        notify_queue_sender: async_channel::Sender<Event>,
        p2p: net::P2pPtr,
        gset: Arc<Mutex<GSet<Event>>>,
    ) -> net::ProtocolBasePtr {
        let message_subsytem = channel.get_message_subsystem();
        message_subsytem.add_dispatch::<Event>().await;

        let event_sub = channel.subscribe_msg::<Event>().await.expect("Missing Event dispatcher!");

        Arc::new(Self {
            notify_queue_sender,
            event_sub,
            jobsman: net::ProtocolJobsManager::new("ProtocolCrdt", channel),
            p2p,
            gset,
        })
    }

    async fn handle_receive_event(self: Arc<Self>) -> Result<()> {
        debug!(target: "crdt", "ProtocolCrdt::handle_receive_event() [START]");
        loop {
            let event = self.event_sub.receive().await?;

            debug!(
                target: "crdt",
                "ProtocolCrdt::handle_receive_event() received {:?}",
                event
            );

            if self.gset.lock().await.contains(&event) {
                continue
            }

            let event = (*event).clone();
            self.p2p.broadcast(event.clone()).await?;

            self.notify_queue_sender.send(event).await?;
        }
    }
}

#[async_trait]
impl net::ProtocolBase for ProtocolCrdt {
    /// Starts ping-pong keep-alive messages exchange. Runs ping-pong in the
    /// protocol task manager, then queues the reply. Sends out a ping and
    /// waits for pong reply. Waits for ping and replies with a pong.
    async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        debug!(target: "crdt", "ProtocolCrdt::start() [START]");
        self.jobsman.clone().start(executor.clone());
        self.jobsman.clone().spawn(self.clone().handle_receive_event(), executor.clone()).await;
        debug!(target: "crdt", "ProtocolCrdt::start() [END]");
        Ok(())
    }

    fn name(&self) -> &'static str {
        "ProtocolCrdt"
    }
}
