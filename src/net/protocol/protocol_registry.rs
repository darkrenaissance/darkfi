use async_std::sync::Mutex;
use std::future::Future;

use futures::future::BoxFuture;
use log::debug;

use super::{
    super::{session::SessionBitflag, ChannelPtr, P2pPtr},
    ProtocolBasePtr,
};

type Constructor =
    Box<dyn Fn(ChannelPtr, P2pPtr) -> BoxFuture<'static, ProtocolBasePtr> + Send + Sync>;

pub struct ProtocolRegistry {
    protocol_constructors: Mutex<Vec<(SessionBitflag, Constructor)>>,
}

impl Default for ProtocolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ProtocolRegistry {
    pub fn new() -> Self {
        Self { protocol_constructors: Mutex::new(Vec::new()) }
    }

    // add_protocol()?
    pub async fn register<C, F>(&self, session_flags: SessionBitflag, constructor: C)
    where
        C: 'static + Fn(ChannelPtr, P2pPtr) -> F + Send + Sync,
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
        channel: ChannelPtr,
        p2p: P2pPtr,
    ) -> Vec<ProtocolBasePtr> {
        let mut protocols: Vec<ProtocolBasePtr> = Vec::new();
        for (session_flags, construct) in self.protocol_constructors.lock().await.iter() {
            // Skip protocols that are not registered for this session
            if selector_id & session_flags == 0 {
                debug!("Skipping {selector_id:#b}, {session_flags:#b}");
                continue
            }

            let protocol: ProtocolBasePtr = construct(channel.clone(), p2p.clone()).await;
            debug!(target: "net", "Attached {}", protocol.name());

            protocols.push(protocol)
        }
        protocols
    }
}
