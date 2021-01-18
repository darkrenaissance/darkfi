use futures::FutureExt;
use smol::Executor;
use std::sync::Arc;

use crate::error::{Error, Result};
use crate::net::messages;
use crate::net::utility::{sleep, clone_net_error};
use crate::net::{ChannelPtr, SettingsPtr};

pub struct ProtocolVersion {
    channel: ChannelPtr,
    settings: SettingsPtr,
}

impl ProtocolVersion {
    pub fn new(channel: ChannelPtr, settings: SettingsPtr) -> Arc<Self> {
        Arc::new(Self { channel, settings })
    }

    pub async fn run(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        // Start timer
        // Send version, wait for verack
        // Wait for version, send verack
        // Fin.
        futures::select! {
            _ = self.clone().exchange_versions(executor).fuse() => Ok(()),
            _ = sleep(self.settings.channel_handshake_seconds).fuse() => Err(Error::ChannelTimeout)
        }
    }

    async fn exchange_versions(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        let send = executor.spawn(self.clone().send_version());
        let recv = executor.spawn(self.recv_version());

        send.await.and(recv.await)
    }

    async fn send_version(self: Arc<Self>) -> Result<()> {
        let version = messages::Message::Version(messages::VersionMessage {});

        self.channel.clone().send(version).await?;

        Ok(())
    }

    async fn recv_version(self: Arc<Self>) -> Result<()> {
        let version_sub = self.channel.clone().subscribe_msg(messages::PacketType::Version).await;

        let version_msg = version_sub.receive().await?;

        // Check the message is OK

        Ok(())
    }
}
