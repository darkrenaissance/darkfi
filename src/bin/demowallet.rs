use async_executor::Executor;
use async_std::sync::{Arc, Mutex};
use easy_parallel::Parallel;

use sapvi::service::{fetch_slabs_loop, GatewayClient};
use sapvi::Result;

async fn start(executor: Arc<Executor<'_>>) -> Result<()> {
    let mut client = GatewayClient::new(String::from("tcp://127.0.0.1:3333"));

    client.start().await?;
    println!("connected to a server");

    let slabs = Arc::new(Mutex::new(vec![]));

    let subscriber = client
        .subscribe(String::from("tcp://127.0.0.1:4444"))
        .await?;

    println!("subscription ready");

    let fetch_loop_task = executor.spawn(fetch_slabs_loop(subscriber.clone(), slabs.clone()));

    client.put_slab(vec![0, 0, 0, 0]).await?;
    client.put_slab(vec![0, 0, 0, 0]).await?;
    client.put_slab(vec![0, 0, 0, 0]).await?;

    fetch_loop_task.cancel().await;

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
