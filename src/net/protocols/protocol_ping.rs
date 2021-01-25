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

pub struct ProtocolPing {
    channel: ChannelPtr,
    settings: SettingsPtr,

    jobsman: ProtocolJobsManagerPtr
}

impl ProtocolPing {
    pub fn new(channel: ChannelPtr, settings: SettingsPtr) -> Arc<Self> {
        Arc::new(Self { channel: channel.clone(), settings, jobsman: ProtocolJobsManager::new(channel) })
    }

    pub async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) {
        self.jobsman.clone().start(executor.clone());
        self.jobsman.clone().spawn(self.clone().run_ping_pong(), executor.clone()).await;
        self.jobsman.clone().spawn(self.reply_to_ping(), executor).await;
    }

    async fn run_ping_pong(self: Arc<Self>) -> NetResult<()> {
        let pong_sub = self
            .channel
            .clone()
            .subscribe_msg(messages::PacketType::Pong)
            .await;

        loop {
            // Wait channel_heartbeat amount of time
            sleep(self.settings.channel_heartbeat_seconds).await;

            // Create a random nonce
            let nonce = Self::random_nonce();

            // Send ping message
            let ping = messages::Message::Ping(messages::PingMessage {
                nonce
            });
            self.channel.clone().send(ping).await?;

            // Wait for pong, check nonce matches
            let pong_msg = receive_message!(pong_sub, messages::Message::Pong);
            if pong_msg.nonce != nonce {
                error!("Wrong nonce for ping reply. Disconnecting from channel.");
                self.channel.stop().await;
                return Err(NetError::ChannelStopped);
            }
        }
    }

    async fn reply_to_ping(self: Arc<Self>) -> NetResult<()> {
        let ping_sub = self
            .channel
            .clone()
            .subscribe_msg(messages::PacketType::Ping)
            .await;

        loop {
            // Wait for ping, reply with pong that has a matching nonce
            let ping = receive_message!(ping_sub, messages::Message::Ping);

            // Send ping message
            let pong = messages::Message::Pong(messages::PongMessage {
                nonce: ping.nonce
            });
            self.channel.clone().send(pong).await?;
        }
    }

    fn random_nonce() -> u32 {
        let mut rng = rand::thread_rng();
        rng.gen()
    }
}
