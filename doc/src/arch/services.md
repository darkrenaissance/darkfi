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
        // ...
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

The method in `start()` is a future that returns `Result<()>`. If you do not want
to return a result (for example with long running processes), then simply use the future:

```rust
    async {
        foo().await;
        unreachable!()
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
stop_sub.unsubscribe().await;
```

## Parent-Child Relationships

In the async context we are forced to use `Arc<Self>`, but often times we want a parent-child
relationship where if both parties contain an Arc reference to the other it creates a
circular loop. For this case, we can use `std::sync::Weak` and `std::sync::Arc::new_cyclic()`.

```rust
pub struct Parent {
    child: Arc<Child>,
    // ...
}

impl Parent {
    pub async fn new(/* ... */) -> Arc<Self> {
        Arc::new_cyclic(|parent| Self {
            child: Child::new(parent.clone())
            // ...
        });
    }

    // ...
}


pub struct Child {
    pub parent: Weak<Parent>,
    // ...
}

impl Child {
    pub fn new(parent: Weak<Parent>) -> Arc<Self> {
        Arc::new(Self {
            parent: Weak::new(),
            // ...
        })
    }

    // ...
}
```

Otherwise if the relationship is just one way, use `Arc<Foo>`. For example if doing dependency
injection where component B is dependent on component A, then we could do:

```rust
let comp_a = Foo::new();
let comp_b = Bar::new(comp_a);
```

