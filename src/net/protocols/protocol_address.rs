use smol::Executor;
use std::sync::Arc;

use crate::net::protocols::{ProtocolJobsManager, ProtocolJobsManagerPtr};
use crate::net::{ChannelPtr, SettingsPtr};

pub struct ProtocolAddress {
    channel: ChannelPtr,
    settings: SettingsPtr,

    jobsman: ProtocolJobsManagerPtr,
}

impl ProtocolAddress {
    pub fn new(channel: ChannelPtr, settings: SettingsPtr) -> Arc<Self> {
        Arc::new(Self {
            channel: channel.clone(),
            settings,
            jobsman: ProtocolJobsManager::new(channel),
        })
    }

    pub async fn start(self: Arc<Self>, _executor: Arc<Executor<'_>>) {}
}
