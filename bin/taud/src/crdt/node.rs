use async_std::sync::{Arc, Mutex};
use std::{cmp::max, fmt::Debug};

use async_executor::Executor;
use log::debug;

use darkfi::{net, util::serial::Encodable, Result};

use super::{Event, GSet, ProtocolCrdt};

pub struct Node {
    // name to idnetifie the node
    name: String,
    // a grow-only set
    gset: Arc<Mutex<GSet<Event>>>,
    // a counter for the node
    time: Mutex<u64>,
    p2p: net::P2pPtr,
    notifier: async_channel::Sender<Vec<u8>>,
}

impl Node {
    pub async fn new(
        name: &str,
        net_settings: net::Settings,
        notifier: async_channel::Sender<Vec<u8>>,
    ) -> Arc<Self> {
        debug!(target: "crdt", "Node::new() [BEGIN]");
        let p2p = net::P2p::new(net_settings).await;
        Arc::new(Self {
            name: name.into(),
            gset: Arc::new(Mutex::new(GSet::new())),
            time: Mutex::new(0),
            p2p,
            notifier,
        })
    }

    pub async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        debug!(target: "crdt", "Node::start() [BEGIN]");
        let (snd, rcv) = async_channel::unbounded::<Event>();

        let p2p = self.p2p.clone();

        let registry = p2p.protocol_registry();

        let gset = self.gset.clone();

        registry
            .register(!net::SESSION_SEED, move |channel, p2p| {
                let sender = snd.clone();
                let gset = gset.clone();
                async move { ProtocolCrdt::init(channel, sender, p2p, gset).await }
            })
            .await;

        //
        // p2p network main instance
        //
        // Performs seed session
        p2p.clone().start(executor.clone()).await?;
        // Actual main p2p session

        let recv_task: smol::Task<Result<()>> = executor.spawn(async move {
            loop {
                if let Ok(event) = rcv.recv().await {
                    self.clone().receive_event(&event).await;
                    let payload = event.get_value()?;
                    self.notifier.send(payload).await?;
                }
            }
        });

        p2p.clone().run(executor.clone()).await?;

        recv_task.cancel().await;

        debug!(target: "crdt", "Node::start() [END]");

        Ok(())
    }

    pub async fn receive_event(self: Arc<Self>, event: &Event) {
        debug!(target: "crdt", "Node receive an event: {:?}", event);

        let mut time = self.time.lock().await;
        *time = max(*time, event.counter) + 1;

        self.gset.lock().await.insert(event);
    }

    pub async fn send_event<T: Encodable + Debug>(self: Arc<Self>, value: T) -> Result<()> {
        debug!(target: "crdt", "Node send an event: {:?}", value);

        let event_time: u64;

        {
            let mut time = self.time.lock().await;
            *time += 1;
            event_time = *time;
        }

        let event = Event::new_update_event(value, event_time, self.name.clone());
        debug!(target: "crdt", "Node create new event: {:?}", event);

        {
            self.gset.lock().await.insert(&event);
        }

        debug!(target: "crdt", "Node broadcast the event: {:?}", event);
        self.p2p.broadcast(event).await?;

        Ok(())
    }

    pub async fn sync(self: Arc<Self>) -> Result<()> {
        debug!(target: "crdt", "Node create new sync event");
        let event = Event::new_sync_event(self.name.clone());

        debug!(target: "crdt", "Node broadcast the sync event");
        self.p2p.broadcast(event).await?;

        Ok(())
    }
}
