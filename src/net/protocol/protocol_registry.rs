use async_std::sync::Mutex;
use futures::future::BoxFuture;
use std::future::Future;

use super::protocol_base::ProtocolBase;
use std::sync::Arc;

use super::protocol_base::ProtocolBasePtr;
use crate::net::{ChannelPtr, P2pPtr};

type Constructor = Box<
    dyn Fn(ChannelPtr, P2pPtr) -> BoxFuture<'static, Arc<dyn 'static + ProtocolBase + Send + Sync>>
        + Send
        + Sync,
>;

pub struct ProtocolRegistry {
    protocol_constructors: Mutex<Vec<Constructor>>,
}

impl ProtocolRegistry {
    pub fn new() -> Self {
        Self { protocol_constructors: Mutex::new(Vec::new()) }
    }

    // add_protocol()?
    pub async fn register<C, F>(&self, constructor: C)
    where
        C: 'static + Fn(ChannelPtr, P2pPtr) -> F + Send + Sync,
        F: 'static + Future<Output = Arc<dyn 'static + ProtocolBase + Send + Sync>> + Send,
    {
        let constructor = move |channel, p2p| {
            Box::pin(constructor(channel, p2p)) as BoxFuture<'static, ProtocolBasePtr>
        };
        self.protocol_constructors.lock().await.push(Box::new(constructor));
    }

    pub async fn attach(&self, channel: ChannelPtr, p2p: P2pPtr) -> Vec<ProtocolBasePtr> {
        let mut protocols: Vec<Arc<dyn ProtocolBase + 'static + Send + Sync>> = Vec::new();
        for construct in self.protocol_constructors.lock().await.iter() {
            let protocol: Arc<dyn ProtocolBase + 'static + Send + Sync> =
                construct(channel.clone(), p2p.clone()).await;
            protocols.push(protocol)
        }
        protocols
    }
}
