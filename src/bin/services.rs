use async_executor::Executor;
use easy_parallel::Parallel;
use std::sync::Arc;

use sapvi::Result;

use sapvi::service::{gateway, reqrep};

async fn start(executor: Arc<Executor<'_>>) -> Result<()> {
    
    executor.clone().spawn(reqrep::ReqRepAPI::start()).detach();

    gateway::GatewayService::start(executor.clone()).await; 
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



