use async_executor::Executor;
use clap::clap_app;
use easy_parallel::Parallel;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use drk::{
    blockchain::{rocks::columns, Rocks, RocksColumn},
    cli::{Config, GatewaydConfig},
    service::GatewayService,
    util::join_config_path,
    Result,
};

async fn start(executor: Arc<Executor<'_>>, config: Arc<&GatewaydConfig>) -> Result<()> {
    let accept_addr: SocketAddr = config.accept_url.parse()?;
    let pub_addr: SocketAddr = config.publisher_url.parse()?;
    let database_path = join_config_path(&PathBuf::from("gatewayd.db"))?;

    let rocks = Rocks::new(&database_path)?;
    let rocks_slabstore_column = RocksColumn::<columns::Slabs>::new(rocks);

    let gateway = GatewayService::new(accept_addr, pub_addr, rocks_slabstore_column)?;

    Ok(gateway.start(executor.clone()).await?)
}

#[async_std::main]
async fn main() -> Result<()> {
    let args = clap_app!(gatewayd =>
        (@arg CONFIG: -c --config +takes_value "Sets a custom config file")
        (@arg verbose: -v --verbose "Increase verbosity")
    )
    .get_matches();

    let config_path = if args.is_present("CONFIG") {
        PathBuf::from(args.value_of("CONFIG").unwrap())
    } else {
        join_config_path(&PathBuf::from("gatewayd.toml"))?
    };

    let loglevel = if args.is_present("verbose") {
        log::Level::Debug
    } else {
        log::Level::Info
    };

    simple_logger::init_with_level(loglevel)?;

    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = async_channel::unbounded::<()>();

    let config: GatewaydConfig = Config::<GatewaydConfig>::load(config_path)?;

    let config_ptr = Arc::new(&config);

    let ex2 = ex.clone();

    let (_, result) = Parallel::new()
        // Run four executor threads.
        .each(0..3, |_| smol::future::block_on(ex.run(shutdown.recv())))
        // Run the main future on the current thread.
        .finish(|| {
            smol::future::block_on(async move {
                start(ex2, config_ptr).await?;
                drop(signal);
                Ok::<(), drk::Error>(())
            })
        });

    result
}
