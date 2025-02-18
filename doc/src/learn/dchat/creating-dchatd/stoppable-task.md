# StoppableTask

Implementing a `JSON-RPC` `RequestHandler` also requires that we implement
a method called `connections_mut`. This introduces us to an important
`darkfi` type called `StoppableTask`.

`StoppableTask` is a async task that can be prematurely (and safely)
stopped at any time. We've already encountered this method when we
discussed `p2p.stop`, which triggers `StoppableTask` to cleanly shutdown
any inbound, outbound or manual sessions which are running.

This is the basic usage of `StoppableTask`:

```rust
    let task = StoppableTask::new();
    task.clone().start(
        my_method(),
        |result| self_.handle_stop(result),
        Error::MyStopError,
        executor,
    );
```

Then at any time we can call `task.stop` to close the task.

To make use of this, we will need to import `StoppableTask` to `dchatd`
and add it to the `Dchat` struct definition. We'll wrap it in a `Mutex`
to ensure thread safety.

```rust
//...

use darkfi::system::{StoppableTask, StoppableTaskPrc};

//...

struct Dchat {
    p2p: net::P2pPtr,
    recv_msgs: DchatMsgsBuffer,
    pub rpc_connections: Mutex<HashSet<StoppableTaskPtr>>,
}

impl Dchat {
    fn new(
        p2p: net::P2pPtr,
        recv_msgs: DchatMsgsBuffer,
        rpc_connections: Mutex<HashSet<StoppableTaskPtr>>,
    ) -> Self {
        Self { p2p, recv_msgs, rpc_connections }
    }
}

```

We'll then add the required trait method `connections_mut` to the `Dchat`
`RequestHandler` implementation that unlocks the `Mutex`, returning a
`HashSet` of `StoppableTaskPtr`.

```rust
    async fn connections_mut(&self) -> MutexGuard<'_, HashSet<StoppableTaskPtr>> {
        self.rpc_connections.lock().await
    }
```

Next, we invoke `JSON-RPC` in the main function of `dchatd`, wielding
`StoppableTask` to start a `JSON-RPC` server and wait for a stop signal as follows:

```rust
    let rpc_settings: RpcSettings = args.rpc.into();
    info!("Starting JSON-RPC server on port {}", rpc_settings.listen);
    let msgs: DchatMsgsBuffer = Arc::new(Mutex::new(vec![DchatMsg { msg: String::new() }]));
    let rpc_connections = Mutex::new(HashSet::new());
    let dchat = Arc::new(Dchat::new(p2p.clone(), msgs.clone(), rpc_connections));
    let _ex = ex.clone();

    let rpc_task = StoppableTask::new();
    rpc_task.clone().start(
        listen_and_serve(rpc_settings, dchat.clone(), None, ex.clone()),
        |res| async move {
            match res {
                Ok(()) | Err(Error::RpcServerStopped) => dchat.stop_connections().await,
                Err(e) => error!("Failed stopping JSON-RPC server: {}", e),
            }
        },
        Error::RpcServerStopped,
        ex.clone(),
    );

    //...

    info!("Stopping JSON-RPC server");
    rpc_task.stop().await;
```

The method `stop_connections` is implemented by `RequestHandler`
trait. Behind the scenes it calls the `connections_mut` method we
implemented above, loops through the `StoppableTaskPtr`'s it returns
and calls `stop` on them, safely closing each `JSON-RPC` connection.

Notice that when we start the `StoppableTask` using
`rpc.task.clone().start`, we also pass a method called `listen_and_serve`.
`listen_and_serve` is a method defined in DarkFi's [rpc
module](https://codeberg.org/darkrenaissance/darkfi/src/branch/master/src/rpc/server.rs).
It starts a JSON-RPC server that is bound to the provided rpc settings
and uses our previously implemented `RequestHandler` to handle incoming
requests.

The async block uses the `move` keyword to takes ownership of
the `settings` and `RequestHandler` values and pass them into
`listen_and_serve`.

We have enabled JSON-RPC.
