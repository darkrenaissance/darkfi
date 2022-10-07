use std::collections::VecDeque;

use async_executor::Executor;
use async_std::sync::{Arc, Mutex};
use async_trait::async_trait;
use fxhash::FxHashMap;
use log::debug;
use rand::{rngs::OsRng, RngCore};

use darkfi::{
    net,
    serial::{SerialDecodable, SerialEncodable},
    util::async_util::sleep,
    Result,
};

use crate::{
    model::{Event, EventId, Model},
    settings::get_current_time,
};

const UNREAD_EVENT_EXPIRE_TIME: u64 = 3600; // in seconds
const SIZE_OF_SEEN_BUFFER: usize = 65536;
const MAX_CONFIRM: u8 = 4;

#[derive(Default, Clone)]
struct RingBuffer<T> {
    pub items: VecDeque<T>,
}

impl<T: Eq + PartialEq + Clone> RingBuffer<T> {
    pub fn new(capacity: usize) -> Self {
        let items = VecDeque::with_capacity(capacity);
        Self { items }
    }

    pub fn push(&mut self, val: T) {
        if self.items.len() == self.items.capacity() {
            self.items.pop_front();
        }
        self.items.push_back(val);
    }

    pub fn contains(&self, val: &T) -> bool {
        self.items.contains(val)
    }
}

type InvId = u64;

#[derive(SerialEncodable, SerialDecodable, Clone, Debug, PartialEq, Eq, Hash)]
struct InvItem {
    id: InvId,
    hash: EventId,
}

#[derive(SerialDecodable, SerialEncodable, Clone, Debug)]
struct Inv {
    invs: Vec<InvItem>,
}

#[derive(SerialDecodable, SerialEncodable, Clone, Debug)]
struct SyncEvent {
    leaves: Vec<EventId>,
}

#[derive(SerialDecodable, SerialEncodable, Clone, Debug)]
struct GetData {
    events: Vec<EventId>,
}

#[derive(Default)]
pub struct Seen<T> {
    seen: Mutex<RingBuffer<T>>,
}

impl<T: Eq + PartialEq + Clone> Seen<T> {
    pub fn new() -> Self {
        Self { seen: Mutex::new(RingBuffer::new(SIZE_OF_SEEN_BUFFER)) }
    }

    pub async fn push(&self, item: &T) -> bool {
        let seen = &mut self.seen.lock().await;
        if !seen.contains(item) {
            seen.push(item.clone());
            return true
        }
        false
    }
}

#[derive(Default)]
pub struct UnreadEvents {
    events: FxHashMap<EventId, Event>,
}

impl UnreadEvents {
    pub fn new() -> Self {
        Self { events: FxHashMap::default() }
    }

    fn contains(&self, key: &EventId) -> bool {
        self.events.contains_key(key)
    }

    fn get(&self, key: &EventId) -> Option<Event> {
        self.events.get(key).cloned()
    }

    // Increase the read_confirms for an event, if it has exceeded the MAX_CONFIRM
    // then remove it from the hash_map and return Some(event), otherwise return None
    fn inc_read_confirms(&mut self, key: &EventId) -> Option<Event> {
        let mut result = None;

        if let Some(event) = self.events.get_mut(key) {
            event.read_confirms += 1;
            if event.read_confirms >= MAX_CONFIRM {
                result = Some(event.clone())
            }
        }

        if result.is_some() {
            self.events.remove(key);
        }

        result
    }

    fn insert(&mut self, event: &Event) {
        // prune expired events
        let mut prune_ids = vec![];
        for (id, e) in self.events.iter() {
            if e.timestamp + (UNREAD_EVENT_EXPIRE_TIME * 1000) < get_current_time() {
                prune_ids.push(*id);
            }
        }
        for id in prune_ids {
            self.events.remove(&id);
        }

        self.events.insert(event.hash(), event.clone());
    }
}

pub struct ProtocolEvent {
    jobsman: net::ProtocolJobsManagerPtr,
    event_sub: net::MessageSubscription<Event>,
    inv_sub: net::MessageSubscription<Inv>,
    getdata_sub: net::MessageSubscription<GetData>,
    syncevent_sub: net::MessageSubscription<SyncEvent>,
    p2p: net::P2pPtr,
    channel: net::ChannelPtr,
    model: Arc<Mutex<Model>>,
    seen_event: Seen<EventId>,
    seen_inv: Seen<InvId>,
    unread_events: Arc<Mutex<UnreadEvents>>,
}

impl ProtocolEvent {
    pub async fn init(
        channel: net::ChannelPtr,
        p2p: net::P2pPtr,
        model: Arc<Mutex<Model>>,
        seen_event: Seen<EventId>,
        seen_inv: Seen<InvId>,
        unread_events: Arc<Mutex<UnreadEvents>>,
    ) -> net::ProtocolBasePtr {
        let message_subsytem = channel.get_message_subsystem();
        message_subsytem.add_dispatch::<Event>().await;
        message_subsytem.add_dispatch::<Inv>().await;
        message_subsytem.add_dispatch::<GetData>().await;
        message_subsytem.add_dispatch::<SyncEvent>().await;

        let event_sub =
            channel.clone().subscribe_msg::<Event>().await.expect("Missing Event dispatcher!");

        let inv_sub = channel.subscribe_msg::<Inv>().await.expect("Missing Inv dispatcher!");

        let getdata_sub =
            channel.clone().subscribe_msg::<GetData>().await.expect("Missing GetData dispatcher!");

        let syncevent_sub = channel
            .clone()
            .subscribe_msg::<SyncEvent>()
            .await
            .expect("Missing SyncEvent dispatcher!");

        Arc::new(Self {
            jobsman: net::ProtocolJobsManager::new("ProtocolEvent", channel.clone()),
            event_sub,
            inv_sub,
            getdata_sub,
            syncevent_sub,
            p2p,
            channel,
            model,
            seen_event,
            seen_inv,
            unread_events,
        })
    }

    async fn handle_receive_event(self: Arc<Self>) -> Result<()> {
        debug!(target: "ircd", "ProtocolEvent::handle_receive_event() [START]");
        let exclude_list = vec![self.channel.address()];
        loop {
            let event = self.event_sub.receive().await?;
            let mut event = (*event).to_owned();

            if !self.seen_event.push(&event.hash()).await {
                continue
            }

            event.read_confirms += 1;

            if event.read_confirms >= MAX_CONFIRM {
                self.new_event(&event).await?;
            } else {
                self.unread_events.lock().await.insert(&event);
                self.send_inv(&event).await?;
            }

            // Broadcast the msg
            self.p2p.broadcast_with_exclude(event, &exclude_list).await?;
        }
    }

    async fn handle_receive_inv(self: Arc<Self>) -> Result<()> {
        debug!(target: "ircd", "ProtocolEvent::handle_receive_inv() [START]");
        let exclude_list = vec![self.channel.address()];
        loop {
            let inv = self.inv_sub.receive().await?;
            let inv = (*inv).to_owned();

            for inv in inv.invs.iter() {
                if !self.seen_inv.push(&inv.id).await {
                    continue
                }

                {
                    let mut unread_events = self.unread_events.lock().await;
                    if !unread_events.contains(&inv.hash) {
                        self.send_getdata(vec![inv.hash]).await?;
                    } else if let Some(event) = unread_events.inc_read_confirms(&inv.hash) {
                        self.new_event(&event).await?;
                    }
                }
            }

            // Broadcast the inv msg
            self.p2p.broadcast_with_exclude(inv, &exclude_list).await?;
        }
    }
    async fn handle_receive_getdata(self: Arc<Self>) -> Result<()> {
        debug!(target: "ircd", "ProtocolEvent::handle_receive_getdata() [START]");
        loop {
            let getdata = self.getdata_sub.receive().await?;
            let events = (*getdata).to_owned().events;

            for event_id in events {
                let unread_event = self.unread_events.lock().await.get(&event_id);
                if let Some(event) = unread_event {
                    self.channel.send(event).await?;
                    continue
                }

                let model_event = self.model.lock().await.get_event(&event_id);
                if let Some(event) = model_event {
                    self.channel.send(event).await?;
                }
            }
        }
    }

    async fn handle_receive_syncevent(self: Arc<Self>) -> Result<()> {
        debug!(target: "ircd", "ProtocolEvent::handle_receive_syncevent() [START]");
        loop {
            let syncevent = self.syncevent_sub.receive().await?;

            let model = self.model.lock().await;
            let leaves = model.find_leaves();

            if leaves == syncevent.leaves {
                continue
            }

            for leaf in syncevent.leaves.iter() {
                if leaves.contains(leaf) {
                    continue
                }

                let children = model.get_event_children(leaf);

                for child in children {
                    self.channel.send(child).await?;
                }
            }
        }
    }

    // every 2 seconds send a SyncEvent msg
    async fn send_sync_hash_loop(self: Arc<Self>) -> Result<()> {
        loop {
            sleep(2).await;
            let leaves = self.model.lock().await.find_leaves();
            self.channel.send(SyncEvent { leaves }).await?;
        }
    }

    async fn new_event(&self, event: &Event) -> Result<()> {
        let mut model = self.model.lock().await;
        if model.is_orphan(event) {
            self.send_getdata(vec![event.hash()]).await?;
        } else {
            model.add(event.clone());
        }

        Ok(())
    }

    async fn send_inv(&self, event: &Event) -> Result<()> {
        let id = OsRng.next_u64();
        self.p2p.broadcast(Inv { invs: vec![InvItem { id, hash: event.hash() }] }).await?;
        Ok(())
    }

    async fn send_getdata(&self, events: Vec<EventId>) -> Result<()> {
        self.channel.send(GetData { events }).await?;
        Ok(())
    }
}

#[async_trait]
impl net::ProtocolBase for ProtocolEvent {
    async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        debug!(target: "ircd", "ProtocolEvent::start() [START]");
        self.jobsman.clone().start(executor.clone());
        self.jobsman.clone().spawn(self.clone().handle_receive_event(), executor.clone()).await;
        self.jobsman.clone().spawn(self.clone().handle_receive_inv(), executor.clone()).await;
        self.jobsman.clone().spawn(self.clone().handle_receive_getdata(), executor.clone()).await;
        self.jobsman.clone().spawn(self.clone().handle_receive_syncevent(), executor.clone()).await;
        self.jobsman.clone().spawn(self.clone().send_sync_hash_loop(), executor.clone()).await;
        debug!(target: "ircd", "ProtocolEvent::start() [END]");
        Ok(())
    }

    fn name(&self) -> &'static str {
        "ProtocolEvent"
    }
}

impl net::Message for Event {
    fn name() -> &'static str {
        "event"
    }
}

impl net::Message for Inv {
    fn name() -> &'static str {
        "inv"
    }
}

impl net::Message for SyncEvent {
    fn name() -> &'static str {
        "syncevent"
    }
}

impl net::Message for GetData {
    fn name() -> &'static str {
        "getdata"
    }
}
