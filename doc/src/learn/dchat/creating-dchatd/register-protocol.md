# Registering a protocol

We've now successfully created a custom protocol. The next step is the
register the protocol with the `ProtocolRegistry`.

We'll define a new function inside the `Dchat` implementation called
`register_protocol()`. It will invoke the `ProtocolRegistry` using the
handle to the p2p network contained in the `Dchat` struct. It will then
call `register()` on the registry and pass the `ProtocolDchat` constructor.

```rust
{{#include ../../../../../example/dchat/dchatd/src/main.rs:register_protocol}}
```

`register` takes a closure with two arguments, `channel` and `p2p`. We
use `move` to capture these values. We then create an async closure
that captures these values and the value `msgs` and use them to call
`ProtocolDchat::init` in the async block.

The code would be expressed more simply as:

```rust
registry.register(!net::SESSION_SEED, async move |channel, _p2p| {
        ProtocolDchat::init(channel, msgs).await
    })
    .await;
```

However we cannot do this due to limitation with async closures. So
instead we wrap the `async move` in a `move` in order to capture the
variables needed by `ProtocolDchat::init`.

Notice the use of a `bitflag`. We use `!SESSION_SEED` to specify that
this protocol should be performed by all sessions aside from the
seed session.

Also notice that `register_protocol` requires a `DchatMsgsBuffer` that we
send to the `ProtocolDchat` constructor. We'll create the `DchatMsgsBuffer`
in `main` and pass it to `Dchat::new`. Let's add `DchatMsgsBuffer` to the
`Dchat` struct definition first.

```rust
struct Dchat {
    p2p: net::P2pPtr,
    recv_msgs: DchatMsgsBuffer,
}
```

And initialize it:

```rust
async_daemonize!(realmain);
async fn realmain(args: Args, ex: Arc<smol::Executor<'static>>) -> Result<()> {
    //...

    let msgs: DchatMsgsBuffer = Arc::new(Mutex::new(vec![DchatMsg { msg: String::new() }]));
    let mut dchat = Dchat::new(p2p.clone(), msgs);

    //...
}
```

Now try running `dchatd` with `lilith` and seeing what debug output you
get. Keep an eye out for the following:

```
[DEBUG] (1) net: Channel::subscribe_msg() [START, command="DchatMsg", address=tcp://127.0.0.1:50105]
[DEBUG] (1) net: Channel::subscribe_msg() [END, command="DchatMsg", address=tcp://127.0.0.1:50105]
[DEBUG] (1) net: Attached ProtocolDchat
```

If you see that, we have successfully:

* Implemented a custom `Message` and created a `MessageSubscription`.
* Implemented a custom `Protocol` and registered it with the `ProtocolRegistry`.
