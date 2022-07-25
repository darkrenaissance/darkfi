use async_executor::Executor;
use async_std::sync::Arc;
use async_trait::async_trait;
use darkfi::{net, Result};
use log::debug;

use crate::dchatmsg::Dchatmsg;

pub struct ProtocolDchat {
    jobsman: net::ProtocolJobsManagerPtr,
    msg_sub: net::MessageSubscription<Dchatmsg>,
    p2p: net::P2pPtr,
}

impl ProtocolDchat {
    pub async fn init(channel: net::ChannelPtr, p2p: net::P2pPtr) -> net::ProtocolBasePtr {
        debug!(target: "dchat", "ProtocolDchat::init() [START]");

        let message_subsytem = channel.get_message_subsystem();
        message_subsytem.add_dispatch::<Dchatmsg>().await;

        let msg_sub =
            channel.subscribe_msg::<Dchatmsg>().await.expect("Missing DchatMsg dispatcher!");

        Arc::new(Self {
            jobsman: net::ProtocolJobsManager::new("ProtocolDchat", channel.clone()),
            msg_sub,
            p2p,
        })
    }

    async fn handle_receive_msg(self: Arc<Self>) -> Result<()> {
        debug!(target: "dchat", "ProtocolDchat::handle_receive_msg() [START]");

        loop {
            let msg = self.msg_sub.receive().await?;

            let msg = (*msg).to_owned();

            //self.sender.send(msg.clone()).await?;

            self.p2p.broadcast(msg).await?;
        }
    }
}

#[async_trait]
impl net::ProtocolBase for ProtocolDchat {
    async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        debug!(target: "dchat", "ProtocolDchat::ProtocolBase::start() [START]");
        self.jobsman.clone().start(executor.clone());
        self.jobsman.clone().spawn(self.clone().handle_receive_msg(), executor.clone()).await;
        debug!(target: "dchat", "ProtocolDchat::ProtocolBase::start() [STOP]");
        Ok(())
    }

    fn name(&self) -> &'static str {
        "ProtocolDchat"
    }
}
