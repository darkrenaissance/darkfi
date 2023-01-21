# Start-Run-Stop

Now that we have initialized the network settings we can create an
instance of the p2p network.

Add the following to `main()`:

```rust
    let p2p = net::P2p::new(settings?).await;
```

We will next create a `Dchat` struct that will store all the data required
by dchat. For now, it will just hold a pointer to the p2p network.

```rust
struct Dchat {
    p2p: net::P2pPtr,
}

impl Dchat {
    fn new(p2p: net::P2pPtr) -> Self {
        Self { p2p }
    }
}
```

Now let's add a `start()` function to the `Dchat` implementation. `start()`
takes an executor and runs three p2p methods, `p2p::start()`, `p2p::run()`,
and `p2p::stop()`.

```rust
    async fn start(&mut self, ex: Arc<Executor<'_>>) -> Result<()> {
        let ex2 = ex.clone();

        self.p2p.clone().start(ex.clone()).await?;
        ex2.spawn(self.p2p.clone().run(ex.clone())).detach();

        self.p2p.stop().await;

        Ok(())
    }

```
Let's take a quick look at the underlying p2p methods we're using here.

## Start

This is [start()](https://github.com/darkrenaissance/darkfi/blob/master/src/net/p2p.rs#L135):

```rust
{{#include ../../../../../src/net/p2p.rs:start}}
```

`start()` changes the `P2pState` to `P2pState::Start` and runs a [seed
session](https://github.com/darkrenaissance/darkfi/blob/master/src/net/session/seed_session.rs).

This loops through the seed addresses specified in our `Settings` and
tries to connect to them. The seed session either connects successfully,
fails with an error or times out.

If a seed node connects successfully, it runs a version exchange protocol,
stores the channel in the p2p list of channels, and disconnects, removing
the channel from the channel list.

## Run

This is [run()](https://github.com/darkrenaissance/darkfi/blob/master/src/net/p2p.rs#L163):

```rust
{{#include ../../../../../src/net/p2p.rs:run}}
```

`run()` changes the P2pState to `P2pState::Run`. It then calls `start()`
on manual, inbound and outbound sessions that are contained with the
`P2p` struct. The outcome of `start()` will depend on how your node is
configured. `start()` will try to run each kind of session, but if the
configuration doesn't match attemping to start a session will simply
return without doing anything. For example, if you are an outbound node,
`inbound.start()` will return with the following message:

```rust
info!(target: "net", "Not configured for accepting incoming connections.");
```

`run()` then waits for a stop signal and shuts down the sessions when it
is received.

## Stop

This is [stop()](https://github.com/darkrenaissance/darkfi/blob/master/src/net/p2p.rs#L306).

```rust
    {{#include ../../../../../src/net/p2p.rs:stop}}
```

`stop()` transmits a shutdown signal to all channels subscribed to the
stop signal and safely shuts down the network.

