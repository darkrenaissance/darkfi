use rand::{rngs::OsRng, Rng};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

pub type SubscriptionId = usize;

#[derive(Debug)]
/// Subscription to the Publisher. Created using `publisher.subscribe().await`.
pub struct Subscription<T> {
    id: SubscriptionId,
    recv_queue: smol::channel::Receiver<T>,
    parent: Arc<Publisher<T>>,
}

impl<T: Clone + Send + 'static> Subscription<T> {
    pub fn get_id(&self) -> SubscriptionId {
        self.id
    }

    /// Receive message.
    pub async fn receive(&self) -> T {
        let message_result = self.recv_queue.recv().await;

        match message_result {
            Ok(message_result) => message_result,
            Err(err) => {
                panic!("Subscription::receive() recv_queue failed! {}", err);
            }
        }
    }

    /// Must be called manually since async Drop is not possible in Rust
    pub fn unsubscribe(&self) {
        self.parent.clone().unsubscribe(self.id)
    }
}

#[derive(Debug)]
pub struct Publisher<T> {
    subs: Mutex<HashMap<SubscriptionId, smol::channel::Sender<T>>>,
}

impl<T: Clone + Send + 'static> Publisher<T> {
    pub fn new() -> Arc<Self> {
        Arc::new(Self { subs: Mutex::new(HashMap::new()) })
    }

    pub fn subscribe(self: Arc<Self>) -> Subscription<T> {
        let (sendr, recvr) = smol::channel::unbounded();
        let sub_id = OsRng.gen();
        // Optional to check whether this ID already exists.
        // It is nearly impossible to ever happen.
        self.subs.lock().unwrap().insert(sub_id, sendr);

        Subscription { id: sub_id, recv_queue: recvr, parent: self.clone() }
    }

    fn unsubscribe(self: Arc<Self>, sub_id: SubscriptionId) {
        self.subs.lock().unwrap().remove(&sub_id);
    }

    /// Publish a message to all listening subscriptions.
    pub fn notify_sync(&self, message_result: T) {
        self.notify_with_exclude_sync(message_result, &[])
    }

    /// Publish a message to all listening subscriptions but exclude some subset.
    /// Sync version.
    pub fn notify_with_exclude_sync(&self, message_result: T, exclude_list: &[SubscriptionId]) {
        for (id, sub) in self.subs.lock().unwrap().iter() {
            if exclude_list.contains(id) {
                continue
            }

            if let Err(e) = sub.try_send(message_result.clone()) {
                warn!(
                    target: "system::publisher",
                    "[system::publisher] Error returned sending message in notify_with_exclude_sync() call! {}", e,
                );
            }
        }
    }
}
