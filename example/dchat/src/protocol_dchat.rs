use async_executor::Executor;
use async_std::sync::Arc;
use async_trait::async_trait;
use darkfi::{net, Result};
use log::info;

use crate::dchatmsg::{Dchatmsg, DchatmsgsBuffer};

pub struct ProtocolDchat {
    jobsman: net::ProtocolJobsManagerPtr,
    msg_sub: net::MessageSubscription<Dchatmsg>,
    p2p: net::P2pPtr,
    msgs: DchatmsgsBuffer,
}

impl ProtocolDchat {
    pub async fn init(
        channel: net::ChannelPtr,
        p2p: net::P2pPtr,
        msgs: DchatmsgsBuffer,
    ) -> net::ProtocolBasePtr {
        info!(target: "dchat", "ProtocolDchat::init() [START]");

        let message_subsytem = channel.get_message_subsystem();
        message_subsytem.add_dispatch::<Dchatmsg>().await;

        let msg_sub =
            channel.subscribe_msg::<Dchatmsg>().await.expect("Missing DchatMsg dispatcher!");

        Arc::new(Self {
            jobsman: net::ProtocolJobsManager::new("ProtocolDchat", channel.clone()),
            msg_sub,
            p2p,
            msgs,
        })
    }

    async fn handle_receive_msg(self: Arc<Self>) -> Result<()> {
        //let mut msg_vec = Vec::new();
        info!(target: "dchat", "ProtocolDchat::handle_receive_msg() [START]");
        while let Ok(msg) = self.msg_sub.receive().await {
            let msg = (*msg).to_owned();
            self.msgs.lock().await.push(msg);
            //self.p2p.broadcast(msg).await?;
            //msg_vec.push(msg);
            //senders.notify(msg).await;
        }
        Ok(())

        //loop {
        //    let msg = self.msg_sub.receive().await?;
        //    let msg = (*msg).to_owned();
        //    self.p2p.broadcast(msg).await?;
        //}
    }
}

#[async_trait]
impl net::ProtocolBase for ProtocolDchat {
    async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        info!(target: "dchat", "ProtocolDchat::ProtocolBase::start() [START]");
        self.jobsman.clone().start(executor.clone());
        self.jobsman.clone().spawn(self.clone().handle_receive_msg(), executor.clone()).await;
        info!(target: "dchat", "ProtocolDchat::ProtocolBase::start() [STOP]");
        Ok(())
    }

    fn name(&self) -> &'static str {
        "ProtocolDchat"
    }
}
