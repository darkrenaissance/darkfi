/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use std::fmt::Debug;

use async_std::sync::{Arc, Mutex};
use async_trait::async_trait;
use darkfi_serial::{Decodable, Encodable, SerialDecodable, SerialEncodable};
use log::debug;

use super::EventMsg;
use crate::{
    event_graph::model::{Event, EventId, ModelPtr},
    impl_p2p_message, net,
    net::Message,
    system::sleep,
    util::ringbuffer::RingBuffer,
    Result,
};

const SIZE_OF_SEEN_BUFFER: usize = 65536;

#[derive(SerialEncodable, SerialDecodable, Clone, Debug, PartialEq, Eq, Hash)]
pub struct InvItem {
    pub hash: EventId,
}

#[derive(SerialDecodable, SerialEncodable, Clone, Debug)]
pub struct Inv {
    pub invs: Vec<InvItem>,
}
impl_p2p_message!(Inv, "inv");

#[derive(SerialDecodable, SerialEncodable, Clone, Debug)]
struct SyncEvent {
    leaves: Vec<EventId>,
}
impl_p2p_message!(SyncEvent, "syncevent");

#[derive(SerialDecodable, SerialEncodable, Clone, Debug)]
struct GetData {
    events: Vec<EventId>,
}
impl_p2p_message!(GetData, "getdata");

pub type SeenPtr<T> = Arc<Seen<T>>;

pub struct Seen<T> {
    seen: Mutex<RingBuffer<T, SIZE_OF_SEEN_BUFFER>>,
}

impl<T: Send + Sync + Eq + PartialEq + Clone> Seen<T> {
    pub fn new() -> SeenPtr<T> {
        Arc::new(Self { seen: Mutex::new(RingBuffer::new()) })
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

pub struct ProtocolEvent<T>
where
    T: Send + Sync + Encodable + Decodable + Debug + 'static,
{
    jobsman: net::ProtocolJobsManagerPtr,
    event_sub: net::MessageSubscription<Event<T>>,
    inv_sub: net::MessageSubscription<Inv>,
    getdata_sub: net::MessageSubscription<GetData>,
    syncevent_sub: net::MessageSubscription<SyncEvent>,
    p2p: net::P2pPtr,
    channel: net::ChannelPtr,
    model: ModelPtr<T>,
    seen_event: SeenPtr<EventId>,
    seen_inv: SeenPtr<EventId>,
}

impl<T> ProtocolEvent<T>
where
    T: Send + Sync + Encodable + Decodable + Clone + EventMsg + Debug + 'static,
{
    pub async fn init(
        channel: net::ChannelPtr,
        p2p: net::P2pPtr,
        model: ModelPtr<T>,
        seen_event: SeenPtr<EventId>,
        seen_inv: SeenPtr<EventId>,
    ) -> net::ProtocolBasePtr {
        let message_subsytem = channel.message_subsystem();
        message_subsytem.add_dispatch::<Event<T>>().await;
        message_subsytem.add_dispatch::<Inv>().await;
        message_subsytem.add_dispatch::<GetData>().await;
        message_subsytem.add_dispatch::<SyncEvent>().await;

        let event_sub =
            channel.clone().subscribe_msg::<Event<T>>().await.expect("Missing Event dispatcher!");

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
        })
    }

    async fn handle_receive_event(self: Arc<Self>) -> Result<()> {
        debug!(target: "event_graph", "ProtocolEvent::handle_receive_event() [START]");
        let exclude_list = vec![self.channel.address().clone()];
        loop {
            let event = self.event_sub.receive().await?;
            let event = (*event).to_owned();

            if !self.seen_event.push(&event.hash()).await {
                continue
            }

            debug!("[P2P] Received: {:?}", event.action);

            self.new_event(&event).await?;
            self.send_inv(&event).await?;

            // Broadcast the msg
            self.p2p.broadcast_with_exclude(&event, &exclude_list).await;
        }
    }

    async fn handle_receive_inv(self: Arc<Self>) -> Result<()> {
        debug!(target: "event_graph", "ProtocolEvent::handle_receive_inv() [START]");
        let exclude_list = vec![self.channel.address().clone()];
        loop {
            let inv = self.inv_sub.receive().await?;
            let inv = (*inv).to_owned();
            let inv_item = inv.invs[0].clone();

            // for inv in inv.invs.iter() {
            if !self.seen_inv.push(&inv_item.hash).await {
                continue
            }

            if self.model.lock().await.get_event(&inv_item.hash).is_none() {
                self.send_getdata(vec![inv_item.hash]).await?;
            }

            // }

            // Broadcast the inv msg
            self.p2p.broadcast_with_exclude(&inv, &exclude_list).await;
        }
    }
    async fn handle_receive_getdata(self: Arc<Self>) -> Result<()> {
        debug!(target: "event_graph", "ProtocolEvent::handle_receive_getdata() [START]");
        loop {
            let getdata = self.getdata_sub.receive().await?;
            let events = (*getdata).to_owned().events;

            for event_id in events {
                let model_event = self.model.lock().await.get_event(&event_id);
                if let Some(event) = model_event {
                    self.channel.send(&event).await?;
                }
            }
        }
    }

    async fn handle_receive_syncevent(self: Arc<Self>) -> Result<()> {
        debug!(target: "event_graph", "ProtocolEvent::handle_receive_syncevent() [START]");
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

                let children = model.get_offspring(leaf);

                for child in children {
                    self.channel.send(&child).await?;
                }
            }
        }
    }

    // every 6 seconds send a SyncEvent msg
    async fn send_sync_hash_loop(self: Arc<Self>) -> Result<()> {
        debug!(target: "event_graph", "ProtocolEvent::send_sync_hash_loop() [START]");
        loop {
            sleep(6).await;
            let leaves = self.model.lock().await.find_leaves();
            self.channel.send(&SyncEvent { leaves }).await?;
        }
    }

    async fn new_event(&self, event: &Event<T>) -> Result<()> {
        debug!(target: "event_graph", "ProtocolEvent::new_event()");
        let mut model = self.model.lock().await;
        model.add(event.clone()).await;

        Ok(())
    }

    async fn send_inv(&self, event: &Event<T>) -> Result<()> {
        debug!(target: "event_graph", "ProtocolEvent::send_inv()");
        self.p2p.broadcast(&Inv { invs: vec![InvItem { hash: event.hash() }] }).await;

        Ok(())
    }

    async fn send_getdata(&self, events: Vec<EventId>) -> Result<()> {
        debug!(target: "event_graph", "ProtocolEvent::send_getdata()");
        self.channel.send(&GetData { events }).await?;
        Ok(())
    }
}

#[async_trait]
impl<T> net::ProtocolBase for ProtocolEvent<T>
where
    T: Send + Sync + Encodable + Decodable + Clone + EventMsg + Debug,
{
    async fn start(self: Arc<Self>, executor: Arc<smol::Executor<'_>>) -> Result<()> {
        debug!(target: "event_graph", "ProtocolEvent::start() [START]");
        self.jobsman.clone().start(executor.clone());
        self.jobsman.clone().spawn(self.clone().handle_receive_event(), executor.clone()).await;
        self.jobsman.clone().spawn(self.clone().handle_receive_inv(), executor.clone()).await;
        self.jobsman.clone().spawn(self.clone().handle_receive_getdata(), executor.clone()).await;
        self.jobsman.clone().spawn(self.clone().handle_receive_syncevent(), executor.clone()).await;
        self.jobsman.clone().spawn(self.clone().send_sync_hash_loop(), executor.clone()).await;
        debug!(target: "event_graph", "ProtocolEvent::start() [END]");
        Ok(())
    }

    fn name(&self) -> &'static str {
        "ProtocolEvent"
    }
}

impl<T> net::Message for Event<T>
where
    T: Send + Sync + Decodable + Encodable + 'static,
{
    const NAME: &'static str = "event";
}
