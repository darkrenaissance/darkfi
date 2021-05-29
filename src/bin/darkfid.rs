use std::net::SocketAddr;
use async_executor::Executor;
use async_std::sync::Arc;
use easy_parallel::Parallel;

use drk::service::{GatewayClient, ClientProgramOptions};
use drk::{slab::Slab, Result};

fn setup_addr(address: Option<SocketAddr>, default: SocketAddr) -> SocketAddr {
    match address {
        Some(addr) => addr,
        None => default,
    }
}


async fn start(executor: Arc<Executor<'_>>, options: ClientProgramOptions) -> Result<()> {
    let connect_addr: SocketAddr = setup_addr(options.connect_addr, "127.0.0.1:3333".parse()?);
    let sub_addr: SocketAddr = setup_addr(options.sub_addr, "127.0.0.1:4444".parse()?);
    let slabstore_path = options.slabstore_path.as_path();


    // create gateway client
    let mut client = GatewayClient::new(connect_addr, slabstore_path)?;

    // start gateway client
    client.start().await?;

    // start subscribe to gateway publisher
    let slabstore = client.get_slabstore();
    let subscriber_task = executor.spawn(GatewayClient::subscribe(
            slabstore,
            sub_addr,
    ));

    // TEST
    let _slab = Slab::new("testcoin".to_string(), vec![0, 0, 0, 0]);
    // client.put_slab(_slab).await?;

    subscriber_task.cancel().await;
    Ok(())
}

fn main() -> Result<()> {
    use simplelog::*;

    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = async_channel::unbounded::<()>();

    let options = ClientProgramOptions::load()?;

    let logger_config = ConfigBuilder::new().set_time_format_str("%T%.6f").build();

    let debug_level = if options.verbose {
        LevelFilter::Debug
    } else {
        LevelFilter::Off
    };

    CombinedLogger::init(vec![
        TermLogger::new(debug_level, logger_config, TerminalMode::Mixed).unwrap(),
        WriteLogger::new(
            LevelFilter::Debug,
            Config::default(),
            std::fs::File::create(options.log_path.as_path()).unwrap(),
        ),
    ])
        .unwrap();



    let ex2 = ex.clone();

    let (_, result) = Parallel::new()
        // Run four executor threads.
        .each(0..3, |_| smol::future::block_on(ex.run(shutdown.recv())))
        // Run the main future on the current thread.
        .finish(|| {
            smol::future::block_on(async move {
                start(ex2, options).await?;
                drop(signal);
                Ok::<(), drk::Error>(())
            })
        });

    result
}
