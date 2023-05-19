# Writing a daemon

DarkFi consists of many seperate daemons communicating with each other. To
run the p2p network, we'll need to implement our own daemon.  So we'll
start building `dchat` by configuring our main function into a daemon that
can run the p2p network.

```rust
{{#include ../../../../../example/dchat/src/main.rs:daemon_deps}}

#[async_std::main]
async fn main() -> Result<()> {
    let ex = Arc::new(Executor::new());
    let ex2 = ex.clone();

    let nthreads = std::thread::available_parallelism().unwrap().get();
    let (signal, shutdown) = smol::channel::unbounded::<()>();

    let (_, result) = Parallel::new()
        .each(0..nthreads, |_| smol::future::block_on(ex2.run(shutdown.recv())))
        .finish(|| {
            smol::future::block_on(async move {
                drop(signal);
                Ok(())
            })
        });

    result

}
```

We get the number of cpu cores using
`std::thread::available_parallelism()` and spin up a bunch of threads
in parallel using `easy_parallel`. Right now it doesn't do anything,
but soon we'll run dchat inside this block.

**Note**: DarkFi includes a macro called `async_daemonize` that is used by
DarkFi binaries to minimize boilerplate in the repo.  To keep things
simple we will ignore this macro for the purpose of this tutorial.  But
check it out if you are curious: [util/cli.rs](https://github.com/darkrenaissance/darkfi/blob/master/src/util/cli.rs#L154).

