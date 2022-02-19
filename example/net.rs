use std::{net::SocketAddr, str::FromStr, sync::Arc};

use async_executor::Executor;
use clap::Parser;

use darkfi::{net, Result};

async fn start(executor: Arc<Executor<'_>>, options: ProgramOptions) -> Result<()> {
    let p2p = net::P2p::new(options.network_settings).await;

    p2p.clone().start(executor.clone()).await?;
    p2p.run(executor).await?;

    Ok(())
}

struct ProgramOptions {
    network_settings: net::Settings,
    log_path: Box<std::path::PathBuf>,
}

#[derive(Parser)]
#[clap(name = "dnode")]
pub struct DarkCli {
    /// accept address
    #[clap(short, long)]
    pub accept: Option<String>,
    /// seed nodes
    #[clap(long, short)]
    pub seeds: Option<Vec<String>>,
    /// manual connections
    #[clap(short, short)]
    pub connect: Option<Vec<String>>,
    ///  connections slots
    #[clap(long)]
    pub connect_slots: Option<u32>,
    /// Logfile path
    #[clap(long)]
    pub log_path: Option<String>,
    /// RPC port
    #[clap(long)]
    pub rpc_port: Option<String>,
}

impl ProgramOptions {
    fn load() -> Result<ProgramOptions> {
        let programcli = DarkCli::parse();

        let accept_addr = if let Some(accept_addr) = programcli.accept {
            Some(accept_addr.parse()?)
        } else {
            None
        };

        let mut seed_addrs: Vec<SocketAddr> = vec![];
        if let Some(seeds) = programcli.seeds {
            for seed in seeds {
                seed_addrs.push(seed.parse()?);
            }
        }

        let mut manual_connects: Vec<SocketAddr> = vec![];
        if let Some(connections) = programcli.connect {
            for connect in connections {
                manual_connects.push(connect.parse()?);
            }
        }

        let connection_slots = if let Some(connection_slots) = programcli.connect_slots {
            connection_slots
        } else {
            0
        };

        let log_path = Box::new(if let Some(log_path) = programcli.log_path {
            std::path::PathBuf::from_str(&log_path)?
        } else {
            std::path::PathBuf::from_str("hello")?
        });

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
