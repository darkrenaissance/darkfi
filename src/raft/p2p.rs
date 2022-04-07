use async_std::sync::{Arc, Mutex};

use async_executor::Executor;
use async_trait::async_trait;
use log::debug;

use crate::{net, Result};

use super::{NetMsg, NodeId};

pub struct ProtocolRaft {
    id: NodeId,
    jobsman: net::ProtocolJobsManagerPtr,
    notify_queue_sender: async_channel::Sender<NetMsg>,
    msg_sub: net::MessageSubscription<NetMsg>,
    p2p: net::P2pPtr,
    msgs: Arc<Mutex<Vec<NetMsg>>>,
}

impl ProtocolRaft {
    pub async fn init(
        id: NodeId,
        channel: net::ChannelPtr,
        notify_queue_sender: async_channel::Sender<NetMsg>,
        p2p: net::P2pPtr,
    ) -> net::ProtocolBasePtr {
        let message_subsytem = channel.get_message_subsystem();
        message_subsytem.add_dispatch::<NetMsg>().await;

        let msg_sub = channel.subscribe_msg::<NetMsg>().await.expect("Missing NetMsg dispatcher!");

        Arc::new(Self {
            id,
            notify_queue_sender,
            msg_sub,
            jobsman: net::ProtocolJobsManager::new("ProtocolRaft", channel),
            p2p,
            msgs: Arc::new(Mutex::new(vec![])),
        })
    }

    async fn handle_receive_msg(self: Arc<Self>) -> Result<()> {
        debug!(target: "raft", "ProtocolRaft::handle_receive_msg() [START]");
        loop {
            let msg = self.msg_sub.receive().await?;

            debug!(
                target: "raft",
                "ProtocolRaft::handle_receive_msg() received {:?}",
                msg
            );

            if self.msgs.lock().await.contains(&msg) {
                continue
            }

            let msg = (*msg).clone();
            self.p2p.broadcast(msg.clone()).await?;

            if let Some(rec_id) = msg.recipient_id.clone() {
                if rec_id != self.id {
                    continue
                }
            }

            self.notify_queue_sender.send(msg).await?;
        }
    }
}

#[async_trait]
impl net::ProtocolBase for ProtocolRaft {
    /// Starts ping-pong keep-alive messages exchange. Runs ping-pong in the
    /// protocol task manager, then queues the reply. Sends out a ping and
    /// waits for pong reply. Waits for ping and replies with a pong.
    async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        debug!(target: "raft", "ProtocolRaft::start() [START]");
        self.jobsman.clone().start(executor.clone());
        self.jobsman.clone().spawn(self.clone().handle_receive_msg(), executor.clone()).await;
        debug!(target: "raft", "ProtocolRaft::start() [END]");
        Ok(())
    }

    fn name(&self) -> &'static str {
        "ProtocolRaft"
    }
}

impl net::Message for NetMsg {
    fn name() -> &'static str {
        "netmsg"
    }
}
