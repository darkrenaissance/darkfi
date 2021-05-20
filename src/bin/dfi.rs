#[macro_use]
extern crate clap;
use async_executor::Executor;
use sapvi::rpc::options::ProgramOptions;
use easy_parallel::Parallel;
use sapvi::rpc::jsonserver;
use sapvi::Result;
use sapvi::rpc::adapter::RpcAdapter;
use std::sync::Arc;


/*
async fn start2(executor: Arc<Executor<'_>>, options: ProgramOptions) -> Result<()> {
    let connections = Arc::new(Mutex::new(HashMap::new()));

    let stored_addrs = Arc::new(Mutex::new(Vec::new()));

    let executor2 = executor.clone();
    let stored_addrs2 = stored_addrs.clone();

    let mut server_task = None;
    if let Some(accept_addr) = options.accept_addr {
        let accept_addr = accept_addr.clone();

        let protocol = ServerProtocol::new(connections.clone(), accept_addr, stored_addrs2);
        server_task = Some(executor.spawn(async move {
            protocol.start(executor2).await?;
            Ok::<(), sapvi::Error>(())
        }));
    }

    let mut seed_protocols = Vec::with_capacity(options.seed_addrs.len());

    // Normally we query this from a server
    let accept_addr = options.accept_addr.clone();

    for seed_addr in options.seed_addrs.iter() {
        let protocol = SeedProtocol::new(seed_addr.clone(), accept_addr, stored_addrs.clone());
        protocol.clone().start(executor.clone()).await;
        seed_protocols.push(protocol);
    }

    debug!("Waiting for seed node queries to finish...");

    for seed_protocol in seed_protocols {
        seed_protocol.await_finish().await;
    }

    debug!("Seed nodes queried.");

    let mut client_slots = vec![];
    for i in 0..options.connection_slots {
        debug!("Starting connection slot {}", i);

        let client = Channel::new(
            connections.clone(),
            accept_addr.clone(),
            stored_addrs.clone(),
        );
        client.clone().start(executor.clone()).await;
        client_slots.push(client);
    }

    for remote_addr in options.manual_connects {
        debug!("Starting connection (manual) to {}", remote_addr);

        let client = Channel::new(
            connections.clone(),
            accept_addr.clone(),
            stored_addrs.clone(),
        );
        client
            .clone()
            .start_manual(remote_addr, executor.clone())
            .await;
        client_slots.push(client);
    }

    let rpc = RpcInterface::new();
    let http = listen(
        executor.clone(),
        rpc.clone(),
        Async::<TcpListener>::bind(([127, 0, 0, 1], 8000))?,
        None,
    );

    let http_task = executor.spawn(http);

    rpc.stop_recv.recv().await?;

    http_task.cancel().await;

    match server_task {
        None => {}
        Some(server_task) => {
            server_task.cancel().await;
        }
    }
    Ok(())
}
*/

//struct ProgramOptions {
//    network_settings: net::Settings,
//    log_path: Box<std::path::PathBuf>,
//    rpc_port: u16,
//}
//
//impl ProgramOptions {
//    fn load() -> Result<ProgramOptions> {
//        let app = clap_app!(dfi =>
//            (version: "0.1.0")
//            (author: "Amir Taaki <amir@dyne.org>")
//            (about: "Dark node")
//            (@arg ACCEPT: -a --accept +takes_value "Accept address")
//            (@arg SEED_NODES: -s --seeds ... "Seed nodes")
//            (@arg CONNECTS: -c --connect ... "Manual connections")
//            (@arg CONNECT_SLOTS: --slots +takes_value "Connection slots")
//            (@arg LOG_PATH: --log +takes_value "Logfile path")
//            (@arg RPC_PORT: -r --rpc +takes_value "RPC port")
//        )
//        .get_matches();
//
//        let accept_addr = if let Some(accept_addr) = app.value_of("ACCEPT") {
//            Some(accept_addr.parse()?)
//        } else {
//            None
//        };
//
//        let mut seed_addrs: Vec<SocketAddr> = vec![];
//        if let Some(seeds) = app.values_of("SEED_NODES") {
//            for seed in seeds {
//                seed_addrs.push(seed.parse()?);
//            }
//        }
//
//        let mut manual_connects: Vec<SocketAddr> = vec![];
//        if let Some(connections) = app.values_of("CONNECTS") {
//            for connect in connections {
//                manual_connects.push(connect.parse()?);
//            }
//        }
//
//        let connection_slots = if let Some(connection_slots) = app.value_of("CONNECT_SLOTS") {
//            connection_slots.parse()?
//        } else {
//            0
//        };
//
//        let log_path = Box::new(
//            if let Some(log_path) = app.value_of("LOG_PATH") {
//                std::path::Path::new(log_path)
//            } else {
//                std::path::Path::new("/tmp/darkfid.log")
//            }
//            .to_path_buf(),
//        );
//
//        let rpc_port = if let Some(rpc_port) = app.value_of("RPC_PORT") {
//            rpc_port.parse()?
//        } else {
//            8000
//        };
//
//        Ok(ProgramOptions {
//            network_settings: net::Settings {
//                inbound: accept_addr,
//                outbound_connections: connection_slots,
//                external_addr: accept_addr,
//                peers: manual_connects,
//                seeds: seed_addrs,
//                ..Default::default()
//            },
//            log_path,
//            rpc_port,
//        })
//    }
//}

fn main() -> Result<()> {
    use simplelog::*;

    let options = ProgramOptions::load()?;

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

    let adapter = RpcAdapter::new();
    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = async_channel::unbounded::<()>();
    let ex2 = ex.clone();

    let (_, result) = Parallel::new()
        // Run four executor threads.
        .each(0..3, |_| smol::future::block_on(ex.run(shutdown.recv())))
        // Run the main future on the current thread.
        .finish(|| {
            smol::future::block_on(async move {
                jsonserver::start(ex2, options, adapter).await?;
                drop(signal);
                Ok::<(), sapvi::Error>(())
            })
        });

    result
}
