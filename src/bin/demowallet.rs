use async_executor::Executor;
use async_std::sync::{Arc, Mutex};
use easy_parallel::Parallel;

use drk::service::{fetch_slabs_loop, GatewayClient};
use drk::Result;

async fn start(executor: Arc<Executor<'_>>) -> Result<()> {
    let mut client = GatewayClient::new("127.0.0.1:3333".parse()?)?;

    client.start().await?;
    println!("connected to a server");

    let slabs = Arc::new(Mutex::new(vec![]));

    let subscriber = client.subscribe("127.0.0.1:4444".parse()?).await?;

    println!("subscriber ready");

    let fetch_loop_task = executor.spawn(fetch_slabs_loop(subscriber.clone(), slabs.clone()));

    println!("send put slab");
    client.put_slab(vec![0, 0, 0, 0]).await?;

    println!("send get last index");
    let index = client.get_last_index().await?;
    println!("index: {}", index);

    println!("send get slab");
    let x = client.get_slab(index).await?;
    println!("{:?}", x);

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
                Ok::<(), drk::Error>(())
            })
        });

    result
}
