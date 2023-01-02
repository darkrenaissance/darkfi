/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use std::{sync::Arc, time::Instant};

use async_trait::async_trait;
use log::{debug, error};
use rand::Rng;
use smol::Executor;

use crate::{util::async_util::sleep, Error, Result};

use super::{
    super::{message, message_subscriber::MessageSubscription, ChannelPtr, P2pPtr, SettingsPtr},
    ProtocolBase, ProtocolBasePtr, ProtocolJobsManager, ProtocolJobsManagerPtr,
};

/// Defines ping and pong messages.
pub struct ProtocolPing {
    channel: ChannelPtr,
    ping_sub: MessageSubscription<message::PingMessage>,
    pong_sub: MessageSubscription<message::PongMessage>,
    settings: SettingsPtr,
    jobsman: ProtocolJobsManagerPtr,
}

impl ProtocolPing {
    /// Create a new ping-pong protocol.
    pub async fn init(channel: ChannelPtr, p2p: P2pPtr) -> ProtocolBasePtr {
        let settings = p2p.settings();

        // Creates a subscription to ping message.
        let ping_sub = channel
            .clone()
            .subscribe_msg::<message::PingMessage>()
            .await
            .expect("Missing ping dispatcher!");

        // Creates a subscription to pong message.
        let pong_sub = channel
            .clone()
            .subscribe_msg::<message::PongMessage>()
            .await
            .expect("Missing pong dispatcher!");

        Arc::new(Self {
            channel: channel.clone(),
            ping_sub,
            pong_sub,
            settings,
            jobsman: ProtocolJobsManager::new("ProtocolPing", channel),
        })
    }

    /// Runs ping-pong protocol. Creates a subscription to pong, then starts a
    /// loop. Loop sleeps for the duration of the channel heartbeat, then
    /// sends a ping message with a random nonce. Loop starts a timer, waits
    /// for the pong reply and insures the nonce is the same.
    async fn run_ping_pong(self: Arc<Self>) -> Result<()> {
        debug!(target: "net::protocol_ping::run_ping_pong()", "ProtocolPing::run_ping_pong() [START]");
        loop {
            // Wait channel_heartbeat amount of time.
            sleep(self.settings.channel_heartbeat_seconds.into()).await;

            // Create a random nonce.
            let nonce = Self::random_nonce();

            // Send ping message.
            let ping = message::PingMessage { nonce };
            self.channel.clone().send(ping).await?;
            debug!(target: "net::protocol_ping::run_ping_pong()", "ProtocolPing::run_ping_pong() send Ping message");
            // Start the timer for ping timer.
            let start = Instant::now();

            // Wait for pong, check nonce matches.
            let pong_msg = self.pong_sub.receive().await?;
            if pong_msg.nonce != nonce {
                // TODO: this is too extreme
                error!(target: "net::protocol_ping::run_ping_pong()", "Wrong nonce for ping reply. Disconnecting from channel.");
                self.channel.stop().await;
                return Err(Error::ChannelStopped)
            }
            let duration = start.elapsed().as_millis();
            debug!(target: "net::protocol_ping::run_ping_pong()", "Received Pong message {}ms from [{:?}]",
                   duration, self.channel.address());
        }
    }

    /// Waits for ping, then replies with pong. Copies ping's nonce into the
    /// pong reply.
    async fn reply_to_ping(self: Arc<Self>) -> Result<()> {
        debug!(target: "net::protocol_ping::run_ping_pong()", "ProtocolPing::reply_to_ping() [START]");
        loop {
            // Wait for ping, reply with pong that has a matching nonce.
            let ping = self.ping_sub.receive().await?;
            debug!(target: "net::protocol_ping::run_ping_pong()", "ProtocolPing::reply_to_ping() received Ping message");

            // Send pong message.
            let pong = message::PongMessage { nonce: ping.nonce };
            self.channel.clone().send(pong).await?;
            debug!(target: "net::protocol_ping::run_ping_pong()", "ProtocolPing::reply_to_ping() sent Pong reply");
        }
    }

    fn random_nonce() -> u32 {
        let mut rng = rand::thread_rng();
        rng.gen()
    }
}

#[async_trait]
impl ProtocolBase for ProtocolPing {
    /// Starts ping-pong keep-alive messages exchange. Runs ping-pong in the
    /// protocol task manager, then queues the reply. Sends out a ping and
    /// waits for pong reply. Waits for ping and replies with a pong.
    async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        debug!(target: "net::protocol_ping::run_ping_pong()", "ProtocolPing::start() [START]");
        self.jobsman.clone().start(executor.clone());
        self.jobsman.clone().spawn(self.clone().run_ping_pong(), executor.clone()).await;
        self.jobsman.clone().spawn(self.reply_to_ping(), executor).await;
        debug!(target: "net::protocol_ping::run_ping_pong()", "ProtocolPing::start() [END]");
        Ok(())
    }

    fn name(&self) -> &'static str {
        "ProtocolPing"
    }
}
