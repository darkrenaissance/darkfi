use log::*;
use rand::Rng;
use smol::Executor;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::net::error::{NetError, NetResult};
use crate::net::messages;
use crate::net::protocols::{ProtocolJobsManager, ProtocolJobsManagerPtr};
use crate::net::utility::sleep;
use crate::net::{ChannelPtr, SettingsPtr};

pub struct ProtocolPing {
    channel: ChannelPtr,
    settings: SettingsPtr,

    jobsman: ProtocolJobsManagerPtr,
}

impl ProtocolPing {
    pub fn new(channel: ChannelPtr, settings: SettingsPtr) -> Arc<Self> {
        Arc::new(Self {
            channel: channel.clone(),
            settings,
            jobsman: ProtocolJobsManager::new("ProtocolPing", channel),
        })
    }

    pub async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) {
        debug!(target: "net", "ProtocolPing::start() [START]");
        self.jobsman.clone().start(executor.clone());
        self.jobsman
            .clone()
            .spawn(self.clone().run_ping_pong(), executor.clone())
            .await;
        self.jobsman
            .clone()
            .spawn(self.reply_to_ping(), executor)
            .await;
        debug!(target: "net", "ProtocolPing::start() [END]");
    }

    async fn run_ping_pong(self: Arc<Self>) -> NetResult<()> {
        debug!(target: "net", "ProtocolPing::run_ping_pong() [START]");
        let pong_sub = self
            .channel
            .clone()
            .subscribe_msg::<messages::PongMessage>()
            .await
            .expect("Missing pong dispatcher!");

        loop {
            // Wait channel_heartbeat amount of time
            sleep(self.settings.channel_heartbeat_seconds).await;

            // Create a random nonce
            let nonce = Self::random_nonce();

            // Send ping message
            let ping = messages::Message::Ping(messages::PingMessage { nonce });
            self.channel.clone().send(ping).await?;
            debug!(target: "net", "ProtocolPing::run_ping_pong() send Ping message");
            // Start the timer for ping timer
            let start = Instant::now();

            // Wait for pong, check nonce matches
            let pong_msg = pong_sub.receive().await?;
            if pong_msg.nonce != nonce {
                error!("Wrong nonce for ping reply. Disconnecting from channel.");
                self.channel.stop().await;
                return Err(NetError::ChannelStopped);
            }
            let duration = start.elapsed().as_millis();
            debug!(target: "net", "Received Pong message {}ms from [{:?}]", duration, self.channel.address());
        }
    }

    async fn reply_to_ping(self: Arc<Self>) -> NetResult<()> {
        debug!(target: "net", "ProtocolPing::reply_to_ping() [START]");
        let ping_sub = self
            .channel
            .clone()
            .subscribe_msg::<messages::PingMessage>()
            .await
            .expect("Missing ping dispatcher!");

        loop {
            // Wait for ping, reply with pong that has a matching nonce
            let ping = ping_sub.receive().await?;
            debug!(target: "net", "ProtocolPing::reply_to_ping() received Ping message");

            // Send ping message
            let pong = messages::Message::Pong(messages::PongMessage { nonce: ping.nonce });
            self.channel.clone().send(pong).await?;
            debug!(target: "net", "ProtocolPing::reply_to_ping() sent Pong reply");
        }
    }

    fn random_nonce() -> u32 {
        let mut rng = rand::thread_rng();
        rng.gen()
    }
}
