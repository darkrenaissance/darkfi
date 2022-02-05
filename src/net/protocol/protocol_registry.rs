use async_std::sync::Mutex;
use futures::future::BoxFuture;
use std::future::Future;

use super::protocol_base::ProtocolBase;
use std::sync::Arc;

//use super::protocol_base::ProtocolBasePtr;
use crate::net::{session::SessionBitflag, ChannelPtr, P2pPtr};

type ProtocolBasePtr = Arc<dyn ProtocolBase + Send + Sync>;

type Constructor = Box<
    dyn Fn(ChannelPtr, P2pPtr) -> BoxFuture<'static, Arc<dyn ProtocolBase + Send + Sync>>
        + Send
        + Sync,
>;

pub struct ProtocolRegistry {
    protocol_constructors: Mutex<Vec<(SessionBitflag, Constructor)>>,
}

impl ProtocolRegistry {
    pub fn new() -> Self {
        Self { protocol_constructors: Mutex::new(Vec::new()) }
    }

    // add_protocol()?
    pub async fn register<C, F>(&self, session_flags: SessionBitflag, constructor: C)
    where
        C: 'static + Fn(ChannelPtr, P2pPtr) -> F + Send + Sync,
        F: 'static + Future<Output = Arc<dyn ProtocolBase + Send + Sync>> + Send,
    {
        let constructor = move |channel, p2p| {
            Box::pin(constructor(channel, p2p))
                as BoxFuture<'static, Arc<dyn ProtocolBase + Send + Sync>>
        };
        self.protocol_constructors.lock().await.push((session_flags, Box::new(constructor)));
    }

    pub async fn attach(
        &self,
        selector_id: SessionBitflag,
        channel: ChannelPtr,
        p2p: P2pPtr,
    ) -> Vec<Arc<dyn ProtocolBase + Send + Sync>> {
        let mut protocols: Vec<Arc<dyn ProtocolBase + Send + Sync>> = Vec::new();
        for (session_flags, construct) in self.protocol_constructors.lock().await.iter() {
            // Skip protocols that are not registered for this session
            if selector_id & session_flags == 0 {
                continue
            }

            let protocol: Arc<dyn ProtocolBase + Send + Sync> =
                construct(channel.clone(), p2p.clone()).await;
            protocols.push(protocol)
        }
        protocols
    }
}
