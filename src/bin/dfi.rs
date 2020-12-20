#[macro_use]
extern crate clap;
use async_channel::unbounded;
use async_executor::Executor;
use async_std::sync::Mutex;
use easy_parallel::Parallel;
use log::*;
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use sapvi::{ClientProtocol, Result, SeedProtocol, ServerProtocol};

async fn start(executor: Arc<Executor<'_>>, options: ProgramOptions) -> Result<()> {
    let connections = Arc::new(Mutex::new(HashMap::new()));

    let stored_addrs = Arc::new(Mutex::new(Vec::new()));

    let executor2 = executor.clone();
    let stored_addrs2 = stored_addrs.clone();

    let mut server_task = None;
    if let Some(accept_addr) = options.accept_addr {
        let accept_addr = accept_addr.clone();

        let mut protocol = ServerProtocol::new(connections.clone());
        server_task = Some(executor.spawn(async move {
            protocol
                .start(accept_addr, stored_addrs2, executor2)
                .await?;
            Ok::<(), sapvi::Error>(())
        }));
    }

    let mut seed_protocols = Vec::with_capacity(options.seed_addrs.len());

    // Normally we query this from a server
    let local_addr = options.accept_addr.clone();

    for seed_addr in options.seed_addrs.iter() {
        let mut protocol = SeedProtocol::new();
        protocol
            .start(
                seed_addr.clone(),
                local_addr,
                stored_addrs.clone(),
                executor.clone(),
            )
            .await;
        seed_protocols.push(protocol);
    }

    debug!("Waiting for seed node queries to finish...");

    for seed_protocol in seed_protocols {
        seed_protocol.await_finish().await;
    }

    debug!("Seed nodes queried.");

    let accept_addr = options.accept_addr.clone();

    let mut client_slots = vec![];
    for i in 0..options.connection_slots {
        debug!("Starting connection slot {}", i);

        let mut client = ClientProtocol::new(
            connections.clone(),
            accept_addr.clone(),
            stored_addrs.clone(),
        );
        client.clone().start(executor.clone()).await;
        client_slots.push(client);
    }

    for remote_addr in options.manual_connects {
        debug!("Starting connection (manual) to {}", remote_addr);

        let mut client = ClientProtocol::new(
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

    loop {
        sapvi::sleep(2).await;
    }

    //server_task.cancel().await;
    //Ok(())
}

struct ProgramOptions {
    accept_addr: Option<SocketAddr>,
    seed_addrs: Vec<SocketAddr>,
    manual_connects: Vec<SocketAddr>,
    connection_slots: u32,
}

impl ProgramOptions {
    fn load() -> Result<ProgramOptions> {
        let app = clap_app!(dfi =>
            (version: "0.1.0")
            (author: "Amir Taaki <amir@dyne.org>")
            (about: "Dark node")
            (@arg ACCEPT: -a --accept +takes_value "Accept address")
            (@arg SEED_NODES: -s --seeds ... "Seed nodes")
            (@arg CONNECTS: -c --connect ... "Manual connections")
            (@arg CONNECT_SLOTS: --slots +takes_value "Connection slots")
        )
        .get_matches();

        let accept_addr = if let Some(accept_addr) = app.value_of("ACCEPT") {
            Some(accept_addr.parse()?)
        } else {
            None
        };

        let mut seed_addrs: Vec<SocketAddr> = vec![];
        if let Some(seeds) = app.values_of("SEED_NODES") {
            for seed in seeds {
                seed_addrs.push(seed.parse()?);
            }
        }

        let mut manual_connects: Vec<SocketAddr> = vec![];
        if let Some(connections) = app.values_of("CONNECTS") {
            for connect in connections {
                manual_connects.push(connect.parse()?);
            }
        }

        let connection_slots = if let Some(connection_slots) = app.value_of("CONNECT_SLOTS") {
            connection_slots.parse()?
        } else {
            0
        };

        Ok(ProgramOptions {
            accept_addr,
            seed_addrs,
            manual_connects,
            connection_slots,
        })
    }
}

fn main() -> Result<()> {
    use simplelog::*;
    CombinedLogger::init(vec![TermLogger::new(
        LevelFilter::Debug,
        Config::default(),
        TerminalMode::Mixed,
    )
    .unwrap()])
    .unwrap();

    let options = ProgramOptions::load()?;

    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = unbounded::<()>();
    let ex2 = ex.clone();

    let (_, result) = Parallel::new()
        // Run four executor threads.
        .each(0..3, |_| smol::future::block_on(ex.run(shutdown.recv())))
        // Run the main future on the current thread.
        .finish(|| {
            smol::future::block_on(async move {
                start(ex2, options).await?;
                drop(signal);
                Ok::<(), sapvi::Error>(())
            })
        });

    result
}
