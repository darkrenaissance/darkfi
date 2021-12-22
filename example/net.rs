#[macro_use]
extern crate clap;
use async_executor::Executor;
//use easy_parallel::Parallel;
use std::{net::SocketAddr, sync::Arc};

use drk::{net, Result};

async fn start(executor: Arc<Executor<'_>>, options: ProgramOptions) -> Result<()> {
    let p2p = net::P2p::new(options.network_settings);

    p2p.clone().start(executor.clone()).await?;

    p2p.run(executor).await?;

    Ok(())
}

struct ProgramOptions {
    network_settings: net::Settings,
    log_path: Box<std::path::PathBuf>,
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
            (@arg LOG_PATH: --log +takes_value "Logfile path")
            (@arg RPC_PORT: -r --rpc +takes_value "RPC port")
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

        let log_path = Box::new(
            if let Some(log_path) = app.value_of("LOG_PATH") {
                std::path::Path::new(log_path)
            } else {
                std::path::Path::new("/tmp/darkfid.log")
            }
            .to_path_buf(),
        );

        Ok(ProgramOptions {
            network_settings: net::Settings {
                inbound: accept_addr,
                outbound_connections: connection_slots,
                external_addr: accept_addr,
                peers: manual_connects,
                seeds: seed_addrs,
                ..Default::default()
            },
            log_path,
        })
    }
}

fn main() -> Result<()> {
    use simplelog::*;

    let options = ProgramOptions::load()?;

    let logger_config = ConfigBuilder::new().set_time_format_str("%T%.6f").build();

    CombinedLogger::init(vec![
        TermLogger::new(LevelFilter::Debug, logger_config, TerminalMode::Mixed, ColorChoice::Auto),
        WriteLogger::new(
            LevelFilter::Debug,
            Config::default(),
            std::fs::File::create(options.log_path.as_path()).unwrap(),
        ),
    ])
    .unwrap();

    let ex = Arc::new(Executor::new());
    smol::block_on(ex.run(start(ex.clone(), options)))

    /*
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
    */
}
