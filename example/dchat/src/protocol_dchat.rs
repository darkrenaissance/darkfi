use async_executor::Executor;
use async_std::sync::Arc;
use async_trait::async_trait;
use darkfi::{net, Result};
use log::debug;

use crate::dchatmsg::{DchatMsg, DchatMsgsBuffer};

pub struct ProtocolDchat {
    jobsman: net::ProtocolJobsManagerPtr,
    msg_sub: net::MessageSubscription<DchatMsg>,
    msgs: DchatMsgsBuffer,
}

impl ProtocolDchat {
    pub async fn init(channel: net::ChannelPtr, msgs: DchatMsgsBuffer) -> net::ProtocolBasePtr {
        debug!(target: "dchat", "ProtocolDchat::init() [START]");
        let message_subsytem = channel.get_message_subsystem();
        message_subsytem.add_dispatch::<DchatMsg>().await;

        let msg_sub =
            channel.subscribe_msg::<DchatMsg>().await.expect("Missing DchatMsg dispatcher!");

        Arc::new(Self {
            jobsman: net::ProtocolJobsManager::new("ProtocolDchat", channel.clone()),
            msg_sub,
            msgs,
        })
    }

    async fn handle_receive_msg(self: Arc<Self>) -> Result<()> {
        debug!(target: "dchat", "ProtocolDchat::handle_receive_msg() [START]");
        while let Ok(msg) = self.msg_sub.receive().await {
            let msg = (*msg).to_owned();
            self.msgs.lock().await.push(msg);
        }

        Ok(())
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
