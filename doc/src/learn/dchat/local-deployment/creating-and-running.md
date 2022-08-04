# Creating the p2p network

Now that we have initialized the network settings we can create an
instance of the p2p network.

Add the following to main():

```
let p2p = net::P2p::new(settings?.into()).await;
```

# Running the p2p network

We will next create a Dchat struct that will store all the data required
by dchat. For now, it will just hold a pointer to the p2p network.

```
use darkfi::net;

struct Dchat {
    p2p: net::P2pPtr,
}

impl Dchat {
    fn new(p2p: net::P2pPtr) -> Self {
        Self { p2p }
    }
}
```

Now let's add a start() function to the Dchat implementation. start()
takes an executor and runs two p2p methods, p2p::start() and p2p::run().

```
async fn start(&self, ex: Arc<Executor<'_>>) -> Result<()> {

    self.p2p.clone().start(ex.clone()).await?;
    self.p2p.clone().run(ex.clone()).await?;

    Ok(())
}
```

Let's take a quick look at the underlying p2p methods we're using here.

This is [start()](https://github.com/darkrenaissance/darkfi/blob/master/src/net/p2p.rs#L129):

```
pub async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
    debug!(target: "net", "P2p::start() [BEGIN]");

    *self.state.lock().await = P2pState::Start;

    // Start seed session
    let seed = SeedSession::new(Arc::downgrade(&self));
    // This will block until all seed queries have finished
    seed.start(executor.clone()).await?;

    *self.state.lock().await = P2pState::Started;

    debug!(target: "net", "P2p::start() [END]");
    Ok(())
}
```

start() changes the P2pState to P2pState::Start and runs a [seed
session](https://github.com/darkrenaissance/darkfi/blob/master/src/net/session/seed_session.rs).

This loops through the seed addresses specified in our Settings and
tries to connect to them. The seed session either connects successfully,
fails with an error or times out.

If a seed node connects successfully, it runs a version exchange protocol,
stores the channel in the p2p list of channels, and disconnects, removing
the channel from the channel list.

This is [run()](https://github.com/darkrenaissance/darkfi/blob/master/src/net/p2p.rs#L157):

```
pub async fn run(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
    debug!(target: "net", "P2p::run() [BEGIN]");

    *self.state.lock().await = P2pState::Run;

    let manual = self.session_manual().await;
    for peer in &self.settings.peers {
        manual.clone().connect(peer, executor.clone()).await;
    }

    let inbound = self.session_inbound().await;
    inbound.clone().start(executor.clone()).await?;

    let outbound = self.session_outbound().await;
    outbound.clone().start(executor.clone()).await?;

    let stop_sub = self.subscribe_stop().await;
    // Wait for stop signal
    stop_sub.receive().await;

    // Stop the sessions
    manual.stop().await;
    inbound.stop().await;
    outbound.stop().await;

    debug!(target: "net", "P2p::run() [END]");
    Ok(())
}
```

run() changes the P2pState to P2pState::Run. It then calls start()
on manual, inbound and outbound sessions that are contained with the
P2p struct. The outcome of start() will depend on how your node is
configured. start() will try to run each kind of session, but if the
configuration doesn't match attemping to start a session will simply
return without doing anything. For example, if you are an outbound node,
inbound.start() will return with the following message:

```
info!(target: "net", "Not configured for accepting incoming connections.");
```

run() then waits for a stop signal and shuts down the sessions when it
is received.

