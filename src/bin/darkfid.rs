use async_executor::Executor;
use async_std::sync::Arc;
use easy_parallel::Parallel;
use std::net::SocketAddr;

use drk::service::{ClientProgramOptions, GatewayClient};
use drk::Result;
use drk::blockchain::{Slab, Rocks};

fn setup_addr(address: Option<SocketAddr>, default: SocketAddr) -> SocketAddr {
    match address {
        Some(addr) => addr,
        None => default,
    }
}

async fn start(executor: Arc<Executor<'_>>, options: ClientProgramOptions) -> Result<()> {
    let connect_addr: SocketAddr = setup_addr(options.connect_addr, "127.0.0.1:3333".parse()?);
    let sub_addr: SocketAddr = setup_addr(options.sub_addr, "127.0.0.1:4444".parse()?);
    let database_path = options.database_path.as_path();

    let rocks = Rocks::new(database_path)?;

    // create gateway client
    let mut client = GatewayClient::new(connect_addr, rocks)?;

    // start gateway client
    client.start().await?;

    // start subscribe to gateway publisher

    let subscriber = GatewayClient::start_subscriber(sub_addr).await?;
    let slabstore = client.get_slabstore();
    let subscribe_task = executor.spawn(GatewayClient::subscribe(subscriber, slabstore));

    // TEST
    let _slab = Slab::new("testcoin".to_string(), vec![0, 0, 0, 0]);
    //client.put_slab(_slab).await?;

    subscribe_task.cancel().await;
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

// $ cargo test --bin darkfid
// run 10 clients simultaneously
#[cfg(test)]
mod test {

    #[test]
    fn test_darkfid_client() {
        use std::path::Path;

        use drk::blockchain::{Rocks, Slab};
        use drk::service::GatewayClient;

        use log::*;
        use rand::Rng;
        use simplelog::*;

        let logger_config = ConfigBuilder::new().set_time_format_str("%T%.6f").build();

        CombinedLogger::init(vec![
            TermLogger::new(LevelFilter::Debug, logger_config, TerminalMode::Mixed).unwrap(),
            WriteLogger::new(
                LevelFilter::Debug,
                Config::default(),
                std::fs::File::create(Path::new("/tmp/dar.log")).unwrap(),
            ),
        ])
        .unwrap();

        let mut thread_pools: Vec<std::thread::JoinHandle<()>> = vec![];

        for _ in 0..10 {
            let thread = std::thread::spawn(|| {
                smol::future::block_on(async move {
                    let mut rng = rand::thread_rng();
                    let rnd: u32 = rng.gen();

                    let path_str = format!("database_{}.db", rnd);
                    let database_path = Path::new(path_str.as_str());
                    let rocks = Rocks::new(database_path.clone()).unwrap();

                    // create new client and use different slabstore
                    let mut client =
                        GatewayClient::new("127.0.0.1:3333".parse().unwrap(), rocks).unwrap();

                    // start client
                    client.start().await.unwrap();

                    // sending slab
                    let _slab = Slab::new("testcoin".to_string(), rnd.to_le_bytes().to_vec());
                    client.put_slab(_slab).await.unwrap();
                })
            });
            thread_pools.push(thread);
        }
        for t in thread_pools {
            t.join().unwrap();
        }
    }
}
