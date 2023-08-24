# Services

Nodes and applications are composed out of services. These are long running
components that may communicate with each other.

The standard signature for a service is of the form:

```rust
use darkfi::ExecutorPtr;

pub struct Service {
    // ...
}

impl Service {
    pub fn new(/* ... */, executor: ExecutorPtr) -> Arc<Self> {
        Arc::new(Self {
            // ...
        })
    }

    pub async fn start(self: Arc<Self>) {
        // ...
    }

    pub async fn stop(&self) {
    }
}
```

Both `start()` and `stop()` should return immediately without blocking the caller.
Any long running tasks they need to perform should be done using `StoppableTask` (see below).

Of course you are free to vary this around within reason. For example, `P2p` looks like this:

```rust
pub struct P2p {
    executor: ExecutorPtr,
    session_outbound: Mutex<Option<Arc<OutboundSession>>>,
    // ...
}

impl P2p {
    pub async fn new(settings: Settings, executor: ExecutorPtr) -> P2pPtr {
        let self_ = Arc::new(Self {
            executor,
            session_outbound: Mutex::new(None),
            // ...
        });

        let parent = Arc::downgrade(&self_);
        *self_.session_outbound.lock().await = Some(OutboundSession::new(parent));

        self_
    }

    pub async fn start(self: Arc<Self>) -> Result<()> {
        // ...
        Ok(())
    }

    pub async fn stop(&self) {
    }
}
```

When services depend on other services, pass them into the `new()` function as an
`Arc<Foo>`. If the dependency also requires a reference to the object (such as when registering
children with parents).

## `StoppableTask`

Services will likely want to start any number of processes. For that you can use `StoppableTask`.

For example `ManualSession` looks like this:

```rust
pub struct ManualSession {
    p2p: Weak<P2p>,
    connect_slots: Mutex<Vec<StoppableTaskPtr>>,
    // ...
}

impl ManualSession {
    pub fn new(p2p: Weak<P2p>) -> ManualSessionPtr {
        Arc::new(Self {
            p2p,
            connect_slots: Mutex::new(Vec::new()),
            // ...
        })
    }

    pub async fn connect(self: Arc<Self>, addr: Url) {
        let ex = self.p2p().executor();
        let task = StoppableTask::new();

        task.clone().start(
            self.clone().channel_connect_loop(addr),
            // Ignore stop handler
            |_| async {},
            Error::NetworkServiceStopped,
            ex,
        );

        self.connect_slots.lock().await.push(task);
    }

    pub async fn stop(&self) {
        let connect_slots = &*self.connect_slots.lock().await;

        for slot in connect_slots {
            slot.stop().await;
        }
    }
    
    // ...
}
```

## Communicating Between Services

Another tool in our toolbox is the `subscribe()/notify()` paradigm.

We can use `system::Subscriber`. Then inside our method we can define a method like so:

```rust
    pub async fn subscribe_stop(&self) -> Result<Subscription<Error>> {
        let sub = self.stop_subscriber.clone().subscribe().await;
        Ok(sub)
    }

    // ...

    // Invoke it like this
    self.stop_subscriber.notify(Error::ChannelStopped).await;
```

Then the API user can simply do:

```
let stop_sub = channel.subscribe_stop().await?;
let err = stop_sub.receive().await;
```

