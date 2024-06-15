use rand::{rngs::OsRng, Rng};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use crate::error::{Error, Result};

pub type SubscriptionId = usize;

// Waiting for trait aliases
trait Piped: Clone + Send + 'static {}
impl<T> Piped for T where T: Clone + Send + 'static {}

#[derive(Debug)]
/// Subscription to the Publisher. Created using `publisher.subscribe().await`.
pub struct Subscription<T: Piped> {
    id: SubscriptionId,
    recv_queue: smol::channel::Receiver<T>,
    parent: Arc<Publisher<T>>,
}

impl<T: Piped> Subscription<T> {
    pub fn get_id(&self) -> SubscriptionId {
        self.id
    }

    /// Receive message.
    pub async fn receive(&self) -> Result<T> {
        let msg_result = self.recv_queue.recv().await;
        msg_result.or(Err(Error::PublisherDestroyed))
    }
}

impl<T: Piped> Drop for Subscription<T> {
    fn drop(&mut self) {
        self.parent.unsubscribe(self.id)
    }
}

pub type PublisherPtr<T> = Arc<Publisher<T>>;

#[derive(Debug)]
pub struct Publisher<T> {
    subs: Mutex<HashMap<SubscriptionId, smol::channel::Sender<T>>>,
}

impl<T: Piped> Publisher<T> {
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

    fn unsubscribe(&self, sub_id: SubscriptionId) {
        self.subs.lock().unwrap().remove(&sub_id);
    }

    /// Publish a message to all listening subscriptions.
    pub fn notify(&self, msg: T) {
        for (id, sub) in self.subs.lock().unwrap().iter() {
            if let Err(e) = sub.try_send(msg.clone()) {
                // This should never happen since Drop calls unsubscribe()
                panic!("Error in notify() call for sub={}! {}", id, e);
            }
        }
    }
}
