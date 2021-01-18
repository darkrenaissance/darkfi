use futures::FutureExt;
use smol::Executor;
use std::sync::Arc;

use crate::error::{Error, Result};
use crate::net::{ChannelPtr, SettingsPtr};

pub struct ProtocolPong {
    channel: ChannelPtr,
    settings: SettingsPtr,
}

impl ProtocolPong {
    pub fn new(channel: ChannelPtr, settings: SettingsPtr) -> Arc<Self> {
        Arc::new(Self { channel, settings })
    }

    pub async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        Ok(())
    }
}

