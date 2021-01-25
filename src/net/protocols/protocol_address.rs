use futures::FutureExt;
use log::*;
use rand::Rng;
use smol::{Executor, Task};
use std::sync::Arc;

use crate::net::error::{NetError, NetResult};
use crate::net::messages;
use crate::net::utility::sleep;
use crate::net::{ChannelPtr, SettingsPtr};
use crate::net::protocols::{ProtocolJobsManager, ProtocolJobsManagerPtr};

pub struct ProtocolAddress {
    channel: ChannelPtr,
    settings: SettingsPtr,

    jobsman: ProtocolJobsManagerPtr
}

impl ProtocolAddress {
    pub fn new(channel: ChannelPtr, settings: SettingsPtr) -> Arc<Self> {
        Arc::new(Self { channel: channel.clone(), settings, jobsman: ProtocolJobsManager::new(channel) })
    }

    pub async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) {
    }
}

