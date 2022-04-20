use async_std::sync::Mutex;
use futures::future::BoxFuture;
use log::debug;
use std::future::Future;

use super::{
    super::{session::SessionBitflag, ChannelPtr, P2pPtr, Transport},
    ProtocolBasePtr,
};

type Constructor<T> =
    Box<dyn Fn(ChannelPtr<T>, P2pPtr<T>) -> BoxFuture<'static, ProtocolBasePtr> + Send + Sync>;

pub struct ProtocolRegistry<T: Transport> {
    protocol_constructors: Mutex<Vec<(SessionBitflag, Constructor<T>)>>,
}

impl<T: Transport> Default for ProtocolRegistry<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Transport> ProtocolRegistry<T> {
    pub fn new() -> Self {
        Self { protocol_constructors: Mutex::new(Vec::new()) }
    }

    // add_protocol()?
    pub async fn register<C, F>(&self, session_flags: SessionBitflag, constructor: C)
    where
        C: 'static + Fn(ChannelPtr<T>, P2pPtr<T>) -> F + Send + Sync,
        F: 'static + Future<Output = ProtocolBasePtr> + Send,
    {
        let constructor = move |channel, p2p| {
            Box::pin(constructor(channel, p2p)) as BoxFuture<'static, ProtocolBasePtr>
        };
        self.protocol_constructors.lock().await.push((session_flags, Box::new(constructor)));
    }

    pub async fn attach(
        &self,
        selector_id: SessionBitflag,
        channel: ChannelPtr<T>,
        p2p: P2pPtr<T>,
    ) -> Vec<ProtocolBasePtr> {
        let mut protocols: Vec<ProtocolBasePtr> = Vec::new();
        for (session_flags, construct) in self.protocol_constructors.lock().await.iter() {
            // Skip protocols that are not registered for this session
            if selector_id & session_flags == 0 {
                continue
            }

            let protocol: ProtocolBasePtr = construct(channel.clone(), p2p.clone()).await;
            debug!(target: "net", "Attached {}", protocol.name());
            protocols.push(protocol)
        }
        protocols
    }
}
