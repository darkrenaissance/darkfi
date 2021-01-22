use futures::FutureExt;
use rand::Rng;
use smol::{Executor, Task};
use std::sync::Arc;

use crate::net::error::{NetError, NetResult};
use crate::net::messages;
use crate::net::utility::{clone_net_error, sleep};
use crate::net::{ChannelPtr, SettingsPtr};

pub struct ProtocolPing {
    channel: ChannelPtr,
    settings: SettingsPtr,
}

impl ProtocolPing {
    pub fn new(channel: ChannelPtr, settings: SettingsPtr) -> Arc<Self> {
        Arc::new(Self { channel, settings })
    }

    pub fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Task<NetResult<()>> {
        executor.spawn(self.run_ping_pong())
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
            let _nonce = Self::random_nonce();
            // TODO: add the nonce after delete other crappy network code

            // Send ping message
            let ping = messages::Message::Ping;
            self.channel.clone().send(ping).await?;

            // Wait for pong, check nonce matches
            let _pong_msg = pong_sub.receive().await?;
            // TODO: fix pong enum
            //let _pong_msg = receive_message!(pong_sub, messages::Message::Pong);
            // TODO: add nonce check here
        }
    }

    fn random_nonce() -> u32 {
        let mut rng = rand::thread_rng();
        rng.gen()
    }
}
