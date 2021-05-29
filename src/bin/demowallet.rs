use async_executor::Executor;
use async_std::sync::Arc;
use easy_parallel::Parallel;

use drk::service::GatewayClient;
use drk::{slab::Slab, Result};

async fn start(executor: Arc<Executor<'_>>) -> Result<()> {
    let mut client = GatewayClient::new("127.0.0.1:3333".parse()?, "slabstore_client.db")?;

    client.start().await?;
    println!("connected to a server");


    let slabstore = client.get_slabstore();
    let subscriber_task =  executor.spawn(GatewayClient::subscribe(slabstore,"127.0.0.1:4444".parse()?));

    println!("subscriber ready");

    println!("send put slab");
    let slab = Slab::new("testcoin".to_string(), vec![0, 0, 0, 0]);
    client.put_slab(slab).await?;

    subscriber_task.cancel().await;
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
