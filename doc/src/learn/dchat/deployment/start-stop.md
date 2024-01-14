# Start-Stop

Now that we have initialized the network settings we can create an
instance of the p2p network.

We will next create a `Dchat` struct that will store all the data required
by dchat. For now, it will just hold a pointer to the p2p network.

```rust
struct Dchat {
    p2p: net::P2pPtr,
}

impl Dchat {
    fn new(
        p2p: net::P2pPtr,
    ) -> Self {
        Self { p2p }
    }
}
```

Let's build out our `realmain` function as follows:

```rust
use log::info;

async_daemonize!(realmain);
async fn realmain(args: Args, ex: Arc<smol::Executor<'static>>) -> Result<()> {
    let p2p = net::P2p::new(args.net.into(), ex.clone()).await;
    info!("Starting P2P network");
    p2p.clone().start().await?;

    let (signals_handler, signals_task) = SignalHandler::new(ex)?;
    signals_handler.wait_termination(signals_task).await?;
    info!("Caught termination signal, cleaning up and exiting...");

    info!("Stopping P2P network");
    p2p.stop().await;

    Ok(())
}
```

Here, we instantiate the p2p network using `P2p::new`. We then start it
by calling `start`, and handle shutdown signals to safely shutdown the
network using `P2p::stop`.

Let's take a quick look at the underlying p2p methods we're using here.

## Start

This is [start](https://github.com/darkrenaissance/darkfi/blob/master/src/net/p2p.rs#L126):

```rust
/// Starts inbound, outbound, and manual sessions.
pub async fn start(self: Arc<Self>) -> Result<()> {
    debug!(target: "net::p2p::start()", "P2P::start() [BEGIN]");
    info!(target: "net::p2p::start()", "[P2P] Starting P2P subsystem");

    // First attempt any set manual connections
    for peer in &self.settings.peers {
        self.session_manual().connect(peer.clone()).await;
    }

    // Start the inbound session
    if let Err(err) = self.session_inbound().start().await {
        error!(target: "net::p2p::start()", "Failed to start inbound session!: {}", err);
        self.session_manual().stop().await;
        return Err(err)
    }

    // Start the outbound session
    self.session_outbound().start().await;

    info!(target: "net::p2p::start()", "[P2P] P2P subsystem started");
    Ok(())
}
```

`start` attempts to start an `Inbound`, `Manual` or `Outbound` session,
which will succeed or fail depending on how your TOML is configured. For
example, if you are an outbound node, `session_inbound.start()` will
return with the following message:

```rust
info!(target: "net", "Not configured for accepting incoming connections.");
```

The function calls in `start` trigger the following processes:

`session_manual.connect`: tries to connect to any `peer` addresses we
have specified in the TOML, using a `Connector`.

`session_inbound.start`: starts an `Acceptor` on the inbound address
specified in the TOML, then creates and registers a `Channel` on that
address.

`session_outbound.start`: tries to establish a connection using a
`Connector` to the number of slots we have specified in the TOML field
`outbound_connections`. For every `Slot`, `run` tries to find a valid
address we can connect to through `PeerDiscovery`, which loops through
all connected channels and sends out a `GetAddr` message. If we don't
have any connected channels, `run` performs a `SeedSync`.

## Stop

This is [stop](https://github.com/darkrenaissance/darkfi/blob/master/src/net/p2p.rs#L164).

```rust
/// Stop the running P2P subsystem
pub async fn stop(&self) {
    // Stop the sessions
    self.session_manual().stop().await;
    self.session_inbound().stop().await;
    self.session_outbound().stop().await;
}
```

`stop` transmits a shutdown signal to all channels subscribed to the
stop signal and safely shuts down the network.
