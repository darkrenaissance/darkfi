# Registering a protocol

We've now successfully created a custom protocol. The next step is the
register the protocol with the protocol registry.

We'll define a new function inside the Dchat implementation called
register_protocol(). It will invoke the protocol_registry using the
handle to the p2p network contained in the Dchat struct. It will then
call register() on the registry and pass the ProtocolDchat constructor.

Be sure to import Dchatmsg and ProtocolDchat so we can access their data.

```
use crate::{dchatmsg::DchatmsgsBuffer, protocol_dchat::ProtocolDchat};

pub mod dchatmsg;
pub mod protocol_dchat;

async fn register_protocol(&self, msgs: DchatmsgsBuffer) -> Result<()> {
    let registry = self.p2p.protocol_registry();
    registry
        .register(net::SESSION_ALL, move |channel, _p2p| {
            let msgs2 = msgs.clone();
            async move { ProtocolDchat::init(channel, msgs2).await }
        })
        .await;
    Ok(())
}
```

We set the bitflag to SESSION_ALL to specify that this protocol should
be performed by every session. We also use a closure to capture a pointer
to Channel, which we pass into the ProtocolDchat constructor. This gives
us access to the message subscriber methods.

Notice that register_protocol() requires a DchatmsgsBuffer that we send
to the ProtocolDchat constructor. We'll create the DchatmsgsBuffer in
main() and pass it to Dchat::new(). Let's add DchatmsgsBuffer to the
Dchat struct definition first.

```
struct Dchat {
    p2p: net::P2pPtr,
    recv_msgs: DchatmsgsBuffer,
}

impl Dchat {
    fn new(p2p: net::P2pPtr, recv_msgs: DchatmsgsBuffer) -> Self {
        Self { p2p, recv_msgs }
    }
}
```

And initialize it, adding Mutex and Dchatmsg to our imports:

```
use async_std::sync::Mutex;
use crate::dchatmsg::Dchatmsg;


async fn main() -> Result<()> {
    // ...

    let msgs: DchatmsgsBuffer = Arc::new(Mutex::new(vec![Dchatmsg { msg: String::new() }]));
    let dchat = Dchat::new(p2p, msgs);

    //... 

    }

```

Finally, call register_protocol() in dchat::start():

```
async fn start(&self, ex: Arc<Executor<'_>>) -> Result<()> {
    self.register_protocol(self.recv_msgs.clone()).await?;

    self.p2p.clone().start(ex.clone()).await?;
    self.p2p.clone().run(ex.clone()).await?;

    Ok(())
}
```
Now try running Alice and Bob and seeing what debug output you get. Keep
an eye out for the following:

```
[DEBUG] (1) net: Channel::subscribe_msg() [START, command="Dchatmsg", address=tcp://127.0.0.1:55555]
[DEBUG] (1) net: Channel::subscribe_msg() [END, command="Dchatmsg", address=tcp://127.0.0.1:55555]
[DEBUG] (1) net: Attached ProtocolDchat
```

If you see that, we have successfully:

* Implemented a custom message type and created a message subscription.
* Implemented a custom protocol and registered it with the protocol registry.

