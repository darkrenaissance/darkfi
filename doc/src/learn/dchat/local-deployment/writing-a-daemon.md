# Writing a daemon

DarkFi consists of many seperate daemons communicating with each other. To
run the p2p network, we'll need to implement our own daemon.  So we'll
start building dchat by configuring our main function into a daemon that
can run the p2p network.

```
use async_executor::Executor;
use async_std::sync::Arc;
use easy_parallel::Parallel;

use std::fs::File;
use simplelog::WriteLogger;

use darkfi::Result;

#[async_std::main]
async fn main() -> Result<()> {
    let nthreads = num_cpus::get();
    let (signal, shutdown) = async_channel::unbounded::<()>();

    let ex = Arc::new(Executor::new());
    //let ex2 = ex.clone();

    let (_, result) = Parallel::new()
        .each(0..nthreads, |_| {
            smol::future::block_on(ex.run(shutdown.recv()))
        })
        .finish(|| {
            smol::future::block_on(async move {
                // TODO
                // dchat.start(ex2).await?;
                drop(signal);
                Ok(())
            })
        });

    result
}
```

We get the number of cpu cores using num_cpus::get() and spin up a bunch
of threads in parallel using easy_parallel. For now it's commented out,
but soon we'll run dchat inside this block.

Note: DarkFi includes a macro called async_daemonize that is used by
DarkFi binaries to minimize boilerplate in the repo.  To keep things
simple we will ignore this macro for the purpose of this tutorial.  But
check it out if you are curious: [util/cli.rs](../../../src/util/cli.rs).

