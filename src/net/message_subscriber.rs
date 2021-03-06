use async_std::sync::Mutex;
use async_trait::async_trait;
use log::*;
use rand::Rng;
use std::any::Any;
use std::collections::HashMap;
use std::io;
use std::io::Cursor;
use std::sync::Arc;

use crate::error::Result;
use crate::net::error::{NetError, NetResult};
use crate::net::messages::{Message, PacketType};
use crate::serial::Decodable;
use crate::serial::Encodable;

pub type MessageSubscriberPtr = Arc<MessageSubscriber>;

pub type MessageResult = NetResult<Arc<Message>>;
pub type MessageSubscriptionID = u64;

macro_rules! receive_message {
    ($sub:expr, $message_type:path) => {{
        let wrapped_message = owning_ref::OwningRef::new($sub.receive().await?);

        wrapped_message.map(|msg| match msg {
            $message_type(msg_detail) => msg_detail,
            _ => {
                panic!("Filter for receive sub invalid!");
            }
        })
    }};
}

pub struct MessageSubscription {
    id: MessageSubscriptionID,
    filter: PacketType,
    recv_queue: async_channel::Receiver<MessageResult>,
    parent: Arc<MessageSubscriber>,
}

impl MessageSubscription {
    fn is_relevant_message(&self, message_result: &MessageResult) -> bool {
        match message_result {
            Ok(message) => {
                let packet_type = message.packet_type();

                // Apply the filter
                packet_type == self.filter
            }
            Err(_) => {
                // Propagate all errors
                true
            }
        }
    }

    pub async fn receive(&self) -> MessageResult {
        loop {
            let message_result = self.recv_queue.recv().await;

            match message_result {
                Ok(message_result) => {
                    if self.clone().is_relevant_message(&message_result) {
                        return message_result;
                    }
                }
                Err(err) => {
                    panic!("MessageSubscription::receive() recv_queue failed! {}", err);
                }
            }
        }
    }

    // Must be called manually since async Drop is not possible in Rust
    pub async fn unsubscribe(&self) {
        self.parent.clone().unsubscribe(self.id).await
    }
}

pub struct MessageSubscriber {
    subs: Mutex<HashMap<MessageSubscriptionID, async_channel::Sender<MessageResult>>>,
}

impl MessageSubscriber {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            subs: Mutex::new(HashMap::new()),
        })
    }

    pub fn random_id() -> MessageSubscriptionID {
        let mut rng = rand::thread_rng();
        rng.gen()
    }

    pub async fn subscribe(self: Arc<Self>, packet_type: PacketType) -> MessageSubscription {
        let (sender, recvr) = async_channel::unbounded();

        let sub_id = Self::random_id();

        self.subs.lock().await.insert(sub_id, sender);

        MessageSubscription {
            id: sub_id,
            filter: packet_type,
            recv_queue: recvr,
            parent: self.clone(),
        }
    }

    async fn unsubscribe(self: Arc<Self>, sub_id: MessageSubscriptionID) {
        self.subs.lock().await.remove(&sub_id);
    }

    pub async fn notify(&self, message_result: NetResult<Arc<Message>>) {
        let mut garbage_ids = Vec::new();

        for (sub_id, sub) in &*self.subs.lock().await {
            match sub.send(message_result.clone()).await {
                Ok(()) => {}
                Err(_err) => {
                    // Automatically clean out closed channels
                    garbage_ids.push(*sub_id);
                    //panic!("Error returned sending message in notify() call! {}", err);
                }
            }
        }

        self.collect_garbage(garbage_ids).await;
    }

    async fn collect_garbage(&self, ids: Vec<MessageSubscriptionID>) {
        let mut subs = self.subs.lock().await;
        for id in &ids {
            subs.remove(id);
        }
    }
}

//
//

pub trait Message2: 'static + Decodable + Send + Sync {
    fn name() -> &'static str;

    fn deserialize();
    fn serialize();
}

pub struct MessageSubscription2<M: Message2> {
    id: MessageSubscriptionID,
    recv_queue: async_channel::Receiver<Arc<M>>,
    parent: Arc<MessageDispatcher<M>>,
}

impl<M: Message2> MessageSubscription2<M> {
    pub async fn receive(&self) -> Arc<M> {
        match self.recv_queue.recv().await {
            Ok(message) => message,
            Err(err) => {
                panic!("MessageSubscription::receive() recv_queue failed! {}", err);
            }
        }
    }

    // Must be called manually since async Drop is not possible in Rust
    pub async fn unsubscribe(&self) {
        self.parent.clone().unsubscribe(self.id).await
    }
}

#[async_trait]
trait MessageDispatcherInterface: Sync {
    async fn notify(&self, payload: Vec<u8>);

    fn as_any(self: Arc<Self>) -> Arc<dyn Any + Send + Sync>;
}

struct MessageDispatcher<M: Message2> {
    subs: Mutex<HashMap<MessageSubscriptionID, async_channel::Sender<Arc<M>>>>,
}

impl<M: Message2> MessageDispatcher<M> {
    fn new() -> Self {
        MessageDispatcher {
            subs: Mutex::new(HashMap::new()),
        }
    }

    pub fn random_id() -> MessageSubscriptionID {
        let mut rng = rand::thread_rng();
        rng.gen()
    }

    pub async fn subscribe(self: Arc<Self>) -> MessageSubscription2<M> {
        let (sender, recvr) = async_channel::unbounded();
        let sub_id = Self::random_id();
        self.subs.lock().await.insert(sub_id, sender);

        MessageSubscription2 {
            id: sub_id,
            recv_queue: recvr,
            parent: self,
        }
    }

    async fn unsubscribe(&self, sub_id: MessageSubscriptionID) {
        self.subs.lock().await.remove(&sub_id);
    }

    async fn notify_all(&self, message: M) {
        let message = Arc::new(message);
        let mut garbage_ids = Vec::new();

        for (sub_id, sub) in &*self.subs.lock().await {
            match sub.send(message.clone()).await {
                Ok(()) => {}
                Err(_err) => {
                    // Automatically clean out closed channels
                    garbage_ids.push(*sub_id);
                    //panic!("Error returned sending message in notify() call! {}", err);
                }
            }
        }

        self.collect_garbage(garbage_ids).await;
    }

    async fn collect_garbage(&self, ids: Vec<MessageSubscriptionID>) {
        let mut subs = self.subs.lock().await;
        for id in &ids {
            subs.remove(id);
        }
    }
}

#[async_trait]
impl<M: Message2> MessageDispatcherInterface for MessageDispatcher<M> {
    async fn notify(&self, payload: Vec<u8>) {
        // deserialize data into type
        // send down the pipes
        let cursor = Cursor::new(payload);
        match M::decode(cursor) {
            Ok(message) => self.notify_all(message).await,
            Err(err) => {
                error!("Unable to decode data. Dropping...: {}", err);
            }
        }
    }

    fn as_any(self: Arc<Self>) -> Arc<dyn Any + Send + Sync> {
        self
    }
}

struct MyVersionMessage {
    x: u32,
}

impl Message2 for MyVersionMessage {
    fn name() -> &'static str {
        "verver"
    }

    fn deserialize() {}
    fn serialize() {}
}

impl Encodable for MyVersionMessage {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += self.x.encode(&mut s)?;
        Ok(len)
    }
}

impl Decodable for MyVersionMessage {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        Ok(Self {
            x: Decodable::decode(&mut d)?,
        })
    }
}

struct MessageSubsystem {
    dispatchers: Mutex<HashMap<&'static str, Arc<dyn MessageDispatcherInterface>>>,
}

impl MessageSubsystem {
    pub fn new() -> Self {
        MessageSubsystem {
            dispatchers: Mutex::new(HashMap::new()),
        }
    }

    pub async fn add_dispatch<M: Message2>(&self) {
        self.dispatchers
            .lock()
            .await
            .insert(M::name(), Arc::new(MessageDispatcher::<M>::new()));
    }

    pub async fn subscribe<M: Message2>(&self) -> NetResult<MessageSubscription2<M>> {
        let dispatcher = self
            .dispatchers
            .lock()
            .await
            .get(MyVersionMessage::name())
            .cloned();

        let sub = match dispatcher {
            Some(dispatcher) => {
                let dispatcher: Arc<MessageDispatcher<M>> = dispatcher
                    .as_any()
                    .downcast::<MessageDispatcher<M>>()
                    .expect("Multiple messages registered with different names");

                dispatcher.subscribe().await
            }
            None => {
                // normall return failure here
                // for now panic
                return Err(NetError::OperationFailed);
            }
        };

        Ok(sub)
    }

    pub async fn trigger(&self, name: &str, data: Vec<u8>) {
        let dispatcher = self.dispatchers.lock().await.get(name).cloned();

        match dispatcher {
            Some(dispatcher) => {
                dispatcher.notify(data).await;
            }
            None => {}
        }
    }
}

pub async fn doteste() {
    println!("hello");

    let subsystem = MessageSubsystem::new();
    subsystem.add_dispatch::<MyVersionMessage>().await;

    // subscribe
    //   1. get dispatcher
    //   2. cast to specific type
    //   3. do sub, return sub
    let sub = subsystem.subscribe::<MyVersionMessage>().await.unwrap();

    let msg = MyVersionMessage { x: 110 };
    let mut payload = Vec::new();
    msg.encode(&mut payload).unwrap();

    // receive message and publish
    //   1. based on string, lookup relevant dispatcher interface
    //   2. publish data there
    subsystem.trigger("verver", payload).await;

    // receive
    //    1. do a get easy
    let msg2 = sub.receive().await;
    println!("{}", msg2.x);

    sub.unsubscribe().await;
}
