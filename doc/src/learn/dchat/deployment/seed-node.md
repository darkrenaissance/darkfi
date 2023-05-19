# The seed node

Let's create an instance of dchat inside our main function and pass the
p2p network into it.  Then we'll add `dchat::start()` to our async loop
in the main function. 

```rust
#[async_std::main]
async fn main() -> Result<()> {
    let settings: Result<Settings> = match std::env::args().nth(1) {
        Some(id) => match id.as_str() {
            "a" => alice(),
            "b" => bob(),
            _ => Err(MissingSpecifier.into()),
        },
        None => Err(MissingSpecifier.into()),
    };

    let p2p = net::P2p::new(settings?.into()).await;

    let dchat = Dchat::new(p2p);

    let nthreads = std::thread::available_parallelism().unwrap().get();
    let (signal, shutdown) = async_channel::unbounded::<()>();

    let ex = Arc::new(Executor::new());
    let ex2 = ex.clone();

    let (_, result) = Parallel::new()
        .each(0..nthreads, |_| {
            smol::future::block_on(ex.run(shutdown.recv()))
        })
        .finish(|| {
            smol::future::block_on(async move {
                dchat.start(ex2).await?;
                drop(signal);
                Ok(())
            })
        });

    result
}
```

Now try to run the program, don't forget to add a specifier `a` or `b`
to define the type of node.

It should output the following error: 

```
Error: NetworkOperationFailed
```

That's because there is no seed node online for our nodes to connect to. A
seed node is used when connecting to the network: it is a special kind
of inbound node that gets connected to, sends over a list of addresses
and disconnects again.  This behavior is defined in the `ProtocolSeed`.

Everytime we run `p2p.start()` we attempt to connect to a seed using a
`SeedSyncSession`.  If the `SeedSyncSession` fails, `p2p.start()` will fail,
so without a seed node, our inbound and outbound nodes cannot establish
a connection to the network. Let's remedy that.

We have two options here. First, we could implement our own seed node.
Alternatively, DarkFi maintains a master seed node called `lilith` that
can act as the seed for many different protocols at the same time. For
the purpose of this tutorial let's use `lilith`.

What `lilith` does in the background is very simple. Just like any p2p
daemon, a seed node defines its networks settings into a type called
`Settings` and creates a new instance of the p2p network. It then runs
`p2p::start()` and `p2p::run()`. The difference is in the settings: a seed
node just specifies an inbound address which other nodes will connect to.

Crucially, this inbound address must match the seed address we specified
earlier in Alice and Bob's settings.

