extern crate clap;
use drk::rpc::adapter::RpcAdapter;
use drk::rpc::jsonserver;
//use drk::rpc::options::ProgramOptions;
use std::net::SocketAddr;

use drk::blockchain::{rocks::columns, Rocks, RocksColumn};
use drk::serial::Decodable;
use drk::service::{ClientProgramOptions, GatewayClient, GatewaySlabsSubscriber};
use drk::{tx, Result};

use async_executor::Executor;
use easy_parallel::Parallel;
use std::sync::Arc;

fn setup_addr(address: Option<SocketAddr>, default: SocketAddr) -> SocketAddr {
    match address {
        Some(addr) => addr,
        None => default,
    }
}

pub async fn subscribe(gateway_slabs_sub: GatewaySlabsSubscriber) -> Result<()> {
    loop {
        let slab = gateway_slabs_sub.recv().await?;
        let tx = tx::Transaction::decode(&slab.get_payload()[..])?;

        //let update = state_transition(&state, tx)?;
        //state.apply(update).await?;
    }
}
async fn start(executor: Arc<Executor<'_>>, options: Arc<ClientProgramOptions>) -> Result<()> {
    let connect_addr: SocketAddr = setup_addr(options.connect_addr, "127.0.0.1:3333".parse()?);
    let sub_addr: SocketAddr = setup_addr(options.sub_addr, "127.0.0.1:4444".parse()?);
    let database_path = options.database_path.as_path();

    let rocks = Rocks::new(database_path)?;

    let slabstore = RocksColumn::<columns::Slabs>::new(rocks.clone());

    // create gateway client
    let mut client = GatewayClient::new(connect_addr, slabstore)?;

    // start subscribing
    let gateway_slabs_sub: GatewaySlabsSubscriber =
        client.start_subscriber(sub_addr, executor.clone()).await?;
    let subscribe_task = executor.spawn(subscribe(gateway_slabs_sub));

    // start gateway client
    client.start().await?;

    subscribe_task.cancel().await;
    Ok(())
}

fn main() -> Result<()> {
    use simplelog::*;

    let options = Arc::new(ClientProgramOptions::load()?);

    let logger_config = ConfigBuilder::new().set_time_format_str("%T%.6f").build();

    CombinedLogger::init(vec![
        TermLogger::new(LevelFilter::Debug, logger_config, TerminalMode::Mixed).unwrap(),
        WriteLogger::new(
            LevelFilter::Debug,
            Config::default(),
            std::fs::File::create(options.log_path.as_path()).unwrap(),
        ),
    ])
    .unwrap();

    //let adapter = RpcAdapter::new("wallet.db")?;
    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = async_channel::unbounded::<()>();
    let ex2 = ex.clone();

    let (_, result) = Parallel::new()
        // Run four executor threads.
        .each(0..3, |_| smol::future::block_on(ex.run(shutdown.recv())))
        // Run the main future on the current thread.
        .finish(|| {
            smol::future::block_on(async move {
                //jsonserver::start(ex2, options, adapter).await?;
                start(ex2, options).await?;
                drop(signal);
                Ok::<(), drk::Error>(())
            })
        });

    result
}
