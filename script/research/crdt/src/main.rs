use std::sync::Arc;

extern crate clap;
use async_executor::Executor;
use easy_parallel::Parallel;
use simplelog::{ColorChoice, Config, LevelFilter, TermLogger, TerminalMode};

use darkfi::Result;

use crdt::{CrdtP2p, Event};

fn main() -> Result<()> {
    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = async_channel::unbounded::<()>();

    let ex2 = ex.clone();

    TermLogger::init(
        LevelFilter::Debug,
        Config::default(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    )?;

    // let nthreads = num_cpus::get();
    // debug!(target: "IRC DAEMON", "Run {} executor threads", nthreads);

    let (sender, _) = async_channel::unbounded::<Event>();

    let (_, result) = Parallel::new()
        .each(0..4, |_| smol::future::block_on(ex.run(shutdown.recv())))
        // Run the main future on the current thread.
        .finish(|| {
            smol::future::block_on(async move {
                CrdtP2p::start(ex2.clone(), sender).await?;
                drop(signal);
                Ok::<(), darkfi::Error>(())
            })
        });

    result
}
