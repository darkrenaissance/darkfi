/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use async_trait::async_trait;
use log::{debug, error, warn};
use rand::{rngs::OsRng, Rng};
use smol::{lock::RwLock as AsyncRwLock, Executor};

use super::{
    super::{
        channel::ChannelPtr,
        message::{PingMessage, PongMessage},
        message_publisher::MessageSubscription,
        p2p::P2pPtr,
        settings::Settings,
    },
    protocol_base::{ProtocolBase, ProtocolBasePtr},
    protocol_jobs_manager::{ProtocolJobsManager, ProtocolJobsManagerPtr},
};
use crate::{
    system::{sleep, timeout::timeout},
    Error, Result,
};

/// Defines ping and pong messages
pub struct ProtocolPing {
    channel: ChannelPtr,
    ping_sub: MessageSubscription<PingMessage>,
    pong_sub: MessageSubscription<PongMessage>,
    settings: Arc<AsyncRwLock<Settings>>,
    jobsman: ProtocolJobsManagerPtr,
}

const PROTO_NAME: &str = "ProtocolPing";

impl ProtocolPing {
    /// Create a new ping-pong protocol.
    pub async fn init(channel: ChannelPtr, p2p: P2pPtr) -> ProtocolBasePtr {
        // Creates a subscription to ping message
        let ping_sub =
            channel.subscribe_msg::<PingMessage>().await.expect("Missing ping dispatcher!");

        // Creates a subscription to pong message
        let pong_sub =
            channel.subscribe_msg::<PongMessage>().await.expect("Missing pong dispatcher!");

        Arc::new(Self {
            channel: channel.clone(),
            ping_sub,
            pong_sub,
            settings: p2p.settings(),
            jobsman: ProtocolJobsManager::new(PROTO_NAME, channel),
        })
    }

    /// Runs the ping-pong protocol. Creates a subscription to pong, then
    /// starts a loop. Loop sleeps for the duration of the channel heartbeat,
    /// then sends a ping message with a random nonce. Loop starts a timer,
    /// waits for the pong reply and ensures the nonce is the same.
    async fn run_ping_pong(self: Arc<Self>) -> Result<()> {
        debug!(
            target: "net::protocol_ping::run_ping_pong()",
            "START => address={}", self.channel.address(),
        );

        loop {
            let settings = self.settings.read().await;
            let outbound_connect_timeout = settings.outbound_connect_timeout;
            let channel_heartbeat_interval = settings.channel_heartbeat_interval;
            drop(settings);

            // Create a random nonce.
            let nonce = Self::random_nonce();

            // Send ping message.
            let ping = PingMessage { nonce };
            self.channel.send(&ping).await?;

            // Start the timer for the ping timer
            let timer = Instant::now();

            // Wait for pong, check nonce matches.
            let pong_msg = match timeout(
                Duration::from_secs(outbound_connect_timeout),
                self.pong_sub.receive(),
            )
            .await
            {
                Ok(msg) => {
                    // msg will be an error when the channel is stopped
                    // so just yield out of this function.
                    msg?
                }
                Err(_e) => {
                    // Pong timeout. We didn't receive any message back
                    // so close the connection.
                    warn!(
                        target: "net::protocol_ping::run_ping_pong()",
                        "[P2P] Ping-Pong protocol timed out for {}", self.channel.address(),
                    );
                    self.channel.stop().await;
                    return Err(Error::ChannelStopped)
                }
            };

            if pong_msg.nonce != nonce {
                error!(
                    target: "net::protocol_ping::run_ping_pong()",
                    "[P2P] Wrong nonce in pingpong, disconnecting {}",
                    self.channel.address(),
                );
                self.channel.stop().await;
                return Err(Error::ChannelStopped)
            }

            debug!(
                target: "net::protocol_ping::run_ping_pong()",
                "Received Pong from {}: {:?}",
                self.channel.address(),
                timer.elapsed(),
            );

            // Sleep until next heartbeat
            sleep(channel_heartbeat_interval).await;
        }
    }

    /// Waits for ping, then replies with pong.
    /// Copies ping's nonce into the pong reply.
    async fn reply_to_ping(self: Arc<Self>) -> Result<()> {
        debug!(
            target: "net::protocol_ping::reply_to_ping()",
            "START => address={}", self.channel.address(),
        );

        loop {
            // Wait for ping, reply with pong that has a matching nonce.
            let ping = self.ping_sub.receive().await?;
            debug!(
                target: "net::protocol_ping::reply_to_ping()",
                "Received Ping from {}", self.channel.address(),
            );

            // Send pong message
            let pong = PongMessage { nonce: ping.nonce };
            self.channel.send(&pong).await?;

            debug!(
                target: "net::protocol_ping::reply_to_ping()",
                "Sent Pong reply to {}", self.channel.address(),
            );
        }
    }

    fn random_nonce() -> u16 {
        OsRng::gen(&mut OsRng)
    }
}

#[async_trait]
impl ProtocolBase for ProtocolPing {
    /// Starts ping-pong keepalive messages exchange. Runs ping-pong in the
    /// protocol task manager, then queues the reply. Sends out a ping and
    /// waits for pong reply. Waits for ping and replies with a pong.
    async fn start(self: Arc<Self>, ex: Arc<Executor<'_>>) -> Result<()> {
        debug!(target: "net::protocol_ping::start()", "START => address={}", self.channel.address());
        self.jobsman.clone().start(ex.clone());
        self.jobsman.clone().spawn(self.clone().run_ping_pong(), ex.clone()).await;
        self.jobsman.clone().spawn(self.clone().reply_to_ping(), ex).await;
        debug!(target: "net::protocol_ping::start()", "END => address={}", self.channel.address());
        Ok(())
    }

    fn name(&self) -> &'static str {
        PROTO_NAME
    }
}
