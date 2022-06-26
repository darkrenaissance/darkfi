use async_std::sync::Mutex;
use std::sync::Arc;

use fxhash::FxHashMap;
use rand::Rng;

pub type SubscriberPtr<T> = Arc<Subscriber<T>>;

pub type SubscriptionId = u64;

pub struct Subscription<T> {
    id: SubscriptionId,
    recv_queue: async_channel::Receiver<T>,
    send_queue: async_channel::Sender<T>,
    parent: Arc<Subscriber<T>>,
}

impl<T: Clone> Subscription<T> {
    pub fn get_id(&self) -> SubscriptionId {
        self.id
    }

    pub async fn receive(&self) -> T {
        let message_result = self.recv_queue.recv().await;

        match message_result {
            Ok(message_result) => message_result,
            Err(err) => {
                panic!("MessageSubscription::receive() recv_queue failed! {}", err);
            }
        }
    }

    pub async fn self_notify(&self, message: T) {
        match self.send_queue.send(message).await {
            Ok(_) => {}
            Err(err) => {
                panic!("MessageSubscription::self_notify() send_queue failed! {}", err);
            }
        }
    }

    // Must be called manually since async Drop is not possible in Rust
    pub async fn unsubscribe(&self) {
        self.parent.clone().unsubscribe(self.id).await
    }
}

// Simple broadcast (publish-subscribe) class
pub struct Subscriber<T> {
    subs: Mutex<FxHashMap<u64, async_channel::Sender<T>>>,
}

impl<T: Clone> Subscriber<T> {
    pub fn new() -> Arc<Self> {
        Arc::new(Self { subs: Mutex::new(FxHashMap::default()) })
    }

    fn random_id() -> SubscriptionId {
        let mut rng = rand::thread_rng();
        rng.gen()
    }

    pub async fn subscribe(self: Arc<Self>) -> Subscription<T> {
        let (sender, recvr) = async_channel::unbounded();

        let sub_id = Self::random_id();

        self.subs.lock().await.insert(sub_id, sender.clone());

        Subscription { id: sub_id, recv_queue: recvr, send_queue: sender, parent: self.clone() }
    }

    async fn unsubscribe(self: Arc<Self>, sub_id: SubscriptionId) {
        self.subs.lock().await.remove(&sub_id);
    }

    pub async fn notify(&self, message_result: T) {
        for sub in (*self.subs.lock().await).values() {
            match sub.send(message_result.clone()).await {
                Ok(()) => {}
                Err(err) => {
                    panic!("Error returned sending message in notify() call! {}", err);
                }
            }
        }
    }

    pub async fn notify_with_exclude(&self, message_result: T, exclude_list: &[SubscriptionId]) {
        for (id, sub) in (*self.subs.lock().await).iter() {
            if exclude_list.contains(id) {
                continue
            }
            match sub.send(message_result.clone()).await {
                Ok(()) => {}
                Err(err) => {
                    panic!("Error returned sending message in notify_with_exclude() call! {}", err);
                }
            }
        }
    }
}
