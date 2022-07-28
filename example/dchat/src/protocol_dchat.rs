use async_executor::Executor;
use async_std::sync::Arc;
use async_trait::async_trait;
use darkfi::{net, Result};
use log::{error, info};

use crate::dchatmsg::Dchatmsg;

pub struct ProtocolDchat {
    jobsman: net::ProtocolJobsManagerPtr,
    msg_sub: net::MessageSubscription<Dchatmsg>,
    p2p: net::P2pPtr,
    p2p_send_channel: async_channel::Sender<Dchatmsg>,
}

impl ProtocolDchat {
    pub async fn init(
        channel: net::ChannelPtr,
        p2p: net::P2pPtr,
        p2p_send_channel: async_channel::Sender<Dchatmsg>,
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
            p2p_send_channel,
        })
    }

    async fn handle_receive_msg(self: Arc<Self>) -> Result<()> {
        info!(target: "dchat", "ProtocolDchat::handle_receive_msg() [START]");
        loop {
            match self.msg_sub.receive().await {
                Ok(msg) => {
                    info!(target: "dchat", "RECEIVED HANDLE_RECEIVE_MSG {:?}", msg);
                    let msg = (*msg).to_owned();
                    match self.p2p_send_channel.send(msg.clone()).await {
                        Ok(o) => {
                            info!(target: "dchat", "SENT MSG ACROSS p2p CHANNEL");
                        }
                        Err(e) => {
                            error!(target: "dchat", "MSG SEND FAILED {}", e);
                        }
                    }
                    info!(target: "dchat", "BROADCASTING MSG {:?}", msg);
                    self.p2p.broadcast(msg).await?;
                }
                Err(e) => {
                    error!(target: "dchat", "ERROR HANDLE_RECEIVE_MSG {:?}", e);
                }
            }
        }

        //loop {

        //    //let msg = (*msg).to_owned();

        //    //self.sender.send(msg.clone()).await?;

        //    //self.p2p.broadcast(msg).await?;
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
