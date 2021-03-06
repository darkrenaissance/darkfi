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
use crate::net::messages::Message;
use crate::serial::{Decodable, Encodable};

pub type MessageSubscriptionID = u64;
type MessageResult<M> = NetResult<Arc<M>>;

pub struct MessageSubscription<M: Message> {
    id: MessageSubscriptionID,
    recv_queue: async_channel::Receiver<MessageResult<M>>,
    parent: Arc<MessageDispatcher<M>>,
}

impl<M: Message> MessageSubscription<M> {
    pub async fn receive(&self) -> MessageResult<M> {
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
trait MessageDispatcherInterface: Send + Sync {
    async fn trigger(&self, payload: Vec<u8>);

    async fn trigger_error(&self, err: NetError);

    fn as_any(self: Arc<Self>) -> Arc<dyn Any + Send + Sync>;
}

struct MessageDispatcher<M: Message> {
    subs: Mutex<HashMap<MessageSubscriptionID, async_channel::Sender<MessageResult<M>>>>,
}

impl<M: Message> MessageDispatcher<M> {
    fn new() -> Self {
        MessageDispatcher {
            subs: Mutex::new(HashMap::new()),
        }
    }

    pub fn random_id() -> MessageSubscriptionID {
        let mut rng = rand::thread_rng();
        rng.gen()
    }

    pub async fn subscribe(self: Arc<Self>) -> MessageSubscription<M> {
        let (sender, recvr) = async_channel::unbounded();
        let sub_id = Self::random_id();
        self.subs.lock().await.insert(sub_id, sender);

        MessageSubscription {
            id: sub_id,
            recv_queue: recvr,
            parent: self,
        }
    }

    async fn unsubscribe(&self, sub_id: MessageSubscriptionID) {
        self.subs.lock().await.remove(&sub_id);
    }

    async fn trigger_all(&self, message: MessageResult<M>) {
        debug!(
            "MessageDispatcher<M={}>::trigger_all({}) [START, subs={}]",
            M::name(),
            if message.is_ok() { "msg" } else { "err" },
            self.subs.lock().await.len()
        );
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

        debug!(
            "MessageDispatcher<M={}>::trigger_all({}) [END, subs={}]",
            M::name(),
            if message.is_ok() { "msg" } else { "err" },
            self.subs.lock().await.len()
        );
    }

    async fn collect_garbage(&self, ids: Vec<MessageSubscriptionID>) {
        let mut subs = self.subs.lock().await;
        for id in &ids {
            subs.remove(id);
        }
    }
}

#[async_trait]
impl<M: Message> MessageDispatcherInterface for MessageDispatcher<M> {
    async fn trigger(&self, payload: Vec<u8>) {
        // deserialize data into type
        // send down the pipes
        let cursor = Cursor::new(payload);
        match M::decode(cursor) {
            Ok(message) => {
                let message = Ok(Arc::new(message));
                self.trigger_all(message).await
            }
            Err(err) => {
                error!("Unable to decode data. Dropping...: {}", err);
            }
        }
    }

    async fn trigger_error(&self, err: NetError) {
        self.trigger_all(Err(err)).await;
    }

    fn as_any(self: Arc<Self>) -> Arc<dyn Any + Send + Sync> {
        self
    }
}

pub struct MessageSubsystem {
    dispatchers: Mutex<HashMap<&'static str, Arc<dyn MessageDispatcherInterface>>>,
}

impl MessageSubsystem {
    pub fn new() -> Self {
        MessageSubsystem {
            dispatchers: Mutex::new(HashMap::new()),
        }
    }

    pub async fn add_dispatch<M: Message>(&self) {
        self.dispatchers
            .lock()
            .await
            .insert(M::name(), Arc::new(MessageDispatcher::<M>::new()));
    }

    pub async fn subscribe<M: Message>(&self) -> NetResult<MessageSubscription<M>> {
        let dispatcher = self.dispatchers.lock().await.get(M::name()).cloned();

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

    pub async fn notify(&self, command: &str, payload: Vec<u8>) {
        let dispatcher = self.dispatchers.lock().await.get(command).cloned();

        match dispatcher {
            Some(dispatcher) => {
                dispatcher.trigger(payload).await;
            }
            None => {
                warn!(
                    "MessageSubsystem::notify(\"{}\", payload) did not find a dispatcher",
                    command
                );
            }
        }
    }

    pub async fn trigger_error(&self, err: NetError) {
        // TODO: this could be parallelized
        for dispatcher in self.dispatchers.lock().await.values() {
            dispatcher.trigger_error(err).await;
        }
    }
}

// This is a test function for the message subsystem code above
// Normall we would use the #[test] macro but cannot since it is async code
// Instead we call it using smol::block_on() in the unit test code after this func
async fn _do_message_subscriber_test() {
    struct MyVersionMessage {
        x: u32,
    }

    impl Message for MyVersionMessage {
        fn name() -> &'static str {
            "verver"
        }
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
    subsystem.notify("verver", payload).await;

    // receive
    //    1. do a get easy
    let msg2 = sub.receive().await.unwrap();
    assert_eq!(msg2.x, 110);
    println!("{}", msg2.x);

    subsystem.trigger_error(NetError::ChannelStopped).await;

    let msg2 = sub.receive().await;
    assert!(msg2.is_err());

    sub.unsubscribe().await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_subscriber() {
        smol::block_on(_do_message_subscriber_test());
    }
}

