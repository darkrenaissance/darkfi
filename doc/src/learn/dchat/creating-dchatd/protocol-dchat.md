# ProtocolDchat

Let's start tying these concepts together. We'll define a struct called
`ProtocolDchat` that contains a `MessageSubscription` to `DchatMsg` and a
pointer to the `ProtocolJobsManager`. We'll also include the `DchatMsgsBuffer`
in the struct as it will come in handy later on.

```rust
{{#include ../../../../../example/dchat/dchatd/src/protocol_dchat.rs:protocol_dchat}}
```

Next we'll implement the trait `ProtocolBase`. `ProtocolBase` requires
two functions, `start` and `name`. In `start` we will start up the
`ProtocolJobsManager`. `name` will return a `str` of the protocol name.

```rust
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

Once that's done, we'll need to create a `ProtocolDchat` constructor
that we will pass to the `ProtocolRegistry` to register our protocol.
We'll invoke the `MessageSubsystem` and add `DchatMsg` as to the list
of dispatchers. Next, we'll create a `MessageSubscription` to `DchatMsg`
using the method `subscribe_msg`.

We'll also initialize the `ProtocolJobsManager` and finally return a
pointer to the protocol.

```rust
{{#include ../../../../../example/dchat/dchatd/src/protocol_dchat.rs:constructor}}
```

We're nearly there. But right now the protocol doesn't actually do
anything. Let's write a method called `handle_receive_msg` which receives
a message on our `MessageSubscription` and adds it to `DchatMsgsBuffer`.
 
Put this inside the `ProtocolDchat` implementation:

```rust
{{#include ../../../../../example/dchat/dchatd/src/protocol_dchat.rs:receive}}
```

As a final step, let's add that task to the `ProtocolJobManager` that is invoked
in `start`:

```rust
{{#include ../../../../../example/dchat/dchatd/src/protocol_dchat.rs:start}}
```
