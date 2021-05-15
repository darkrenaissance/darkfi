use async_executor::Executor;
use easy_parallel::Parallel;
use std::sync::Arc;

use sapvi::Result;

use sapvi::service::gateway;

async fn start(executor: Arc<Executor<'_>>) -> Result<()> {

    let gateway =
        gateway::GatewayService::new(
            String::from("tcp://127.0.0.1:3333"),
            String::from("tcp://127.0.0.1:4444")
        );

    gateway.start(executor.clone()).await?;
    Ok(())
}

fn main() -> Result<()> {
    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = async_channel::unbounded::<()>();
    let ex2 = ex.clone();

    let (_, result) = Parallel::new()
        // Run four executor threads.
        .each(0..3, |_| smol::future::block_on(ex.run(shutdown.recv())))
        // Run the main future on the current thread.
        .finish(|| {
            smol::future::block_on(async move {
                start(ex2).await?;
                drop(signal);
                Ok::<(), sapvi::Error>(())
            })
        });

    result
}
