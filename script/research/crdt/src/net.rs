use std::sync::Arc;

use async_executor::Executor;
use async_trait::async_trait;
use log::debug;

use darkfi::{net, Result};

use crate::Event;

pub struct CrdtP2p {}

impl CrdtP2p {
    pub async fn start(
        executor: Arc<Executor<'_>>,
        notify_queue_sender: async_channel::Sender<Event>,
    ) -> Result<()> {
        let p2p = net::P2p::new(net::Settings::default()).await;
        let registry = p2p.protocol_registry();

        registry
            .register(!net::SESSION_SEED, move |channel, p2p| {
                let sender = notify_queue_sender.clone();
                async move { ProtocolCrdt::init(channel, sender, p2p).await }
            })
            .await;

        //
        // p2p network main instance
        //
        // Performs seed session
        p2p.clone().start(executor.clone()).await?;
        // Actual main p2p session
        p2p.run(executor).await
    }
}

struct ProtocolCrdt {
    jobsman: net::ProtocolJobsManagerPtr,
    notify_queue_sender: async_channel::Sender<Event>,
    event_sub: net::MessageSubscription<Event>,
    p2p: net::P2pPtr,
}

impl ProtocolCrdt {
    pub async fn init(
        channel: net::ChannelPtr,
        notify_queue_sender: async_channel::Sender<Event>,
        p2p: net::P2pPtr,
    ) -> net::ProtocolBasePtr {
        let message_subsytem = channel.get_message_subsystem();
        message_subsytem.add_dispatch::<Event>().await;

        let event_sub = channel.subscribe_msg::<Event>().await.expect("Missing Event dispatcher!");

        Arc::new(Self {
            notify_queue_sender,
            event_sub,
            jobsman: net::ProtocolJobsManager::new("ProtocolCrdt", channel),
            p2p,
        })
    }

    async fn handle_receive_event(self: Arc<Self>) -> Result<()> {
        debug!(target: "crdt", "ProtocolCrdt::handle_receive_event() [START]");
        loop {
            let event = self.event_sub.receive().await?;

            debug!(
                target: "ircd",
                "ProtocolCrdt::handle_receive_event() received {:?}",
                event
            );

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
