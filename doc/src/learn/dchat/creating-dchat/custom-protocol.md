# ProtocolDchat

Let's start tying these concepts together. We'll define a struct called
ProtocolDchat that contains a MessageSubscription to Dchatmsg and a
pointer to the ProtocolJobsManager. We'll also include the DchatmsgsBuffer
in the struct as it will come in handy later on.

```
use darkfi::net;

use crate::dchatmsg::{Dchatmsg, DchatmsgsBuffer};

pub struct ProtocolDchat {
    jobsman: net::ProtocolJobsManagerPtr,
    msg_sub: net::MessageSubscription<Dchatmsg>,
    msgs: DchatmsgsBuffer,
}
```

Next we'll implement the trait ProtocolBase. ProtocolBase requires two
functions, start() and name(). In start() we will start up the Protocol
Jobs Manager. name() will return a str of the protocol name.

```
use async_executor::Executor;
use async_std::sync::Arc;
use async_trait::async_trait;
use darkfi::{net, Result};

use crate::dchatmsg::{Dchatmsg, DchatmsgsBuffer};

#[async_trait]
impl net::ProtocolBase for ProtocolDchat {
    async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        self.jobsman.clone().start(executor.clone());
        Ok(())
    }

    fn name(&self) -> &'static str {
        "ProtocolDchat"
    }
}
```

Once that's done, we'll need to create a ProtocolDchat constructor that
we will pass to the ProtocolRegistry to register our protocol. The
constructor takes a pointer to channel which it uses to invoke the
Message Subsystem and add Dchatmsg as to the list of dispatchers. Next,
we'll create a message subscription to Dchatmsg using the method
subscribe_msg().

We'll also initialize the Protocol Jobs Manager and finally return a
pointer to the protocol.

```
impl ProtocolDchat {
    pub async fn init(channel: net::ChannelPtr, msgs: DchatmsgsBuffer) -> net::ProtocolBasePtr {
        let message_subsytem = channel.get_message_subsystem();
        message_subsytem.add_dispatch::<Dchatmsg>().await;

        let msg_sub = channel
            .subscribe_msg::<Dchatmsg>()
            .await
            .expect("Missing DchatMsg dispatcher!");

        Arc::new(Self {
            jobsman: net::ProtocolJobsManager::new("ProtocolDchat", channel.clone()),
            msg_sub,
            msgs,
        })
    }
}
```

We're nearly there. But right now the protocol doesn't actually do
anything. Let's write a method called handle_receive_msg() which receives
a message on our message subscription and adds it to DchatmsgsBuffer.
 
Put this inside the ProtocolDchat implementation:

```
async fn handle_receive_msg(self: Arc<Self>) -> Result<()> {
    while let Ok(msg) = self.msg_sub.receive().await {
        let msg = (*msg).to_owned();
        self.msgs.lock().await.push(msg);
    }

    Ok(())
}
```

As a final step, let's add that task to the jobs manager that is invoked
in start():

```
async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
    self.jobsman.clone().start(executor.clone());
    self.jobsman
        .clone()
        .spawn(self.clone().handle_receive_msg(), executor.clone())
        .await;
    Ok(())
}
```

