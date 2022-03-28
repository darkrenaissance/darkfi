use std::{net::SocketAddr, str::FromStr, sync::Arc};

use async_executor::Executor;
use clap::Parser;
use simplelog::*;

use darkfi::{net, util::cli::log_config, Result};

async fn start(executor: Arc<Executor<'_>>, options: ProgramOptions) -> Result<()> {
    let p2p = net::P2p::new(options.network_settings).await;

    p2p.clone().start(executor.clone()).await?;
    p2p.run(executor).await?;

    Ok(())
}

struct ProgramOptions {
    network_settings: net::Settings,
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

        Ok(ProgramOptions {
            network_settings: net::Settings {
                inbound: accept_addr,
                outbound_connections: connection_slots,
                external_addr: accept_addr,
                peers: manual_connects,
                seeds: seed_addrs,
                ..Default::default()
            },
        })
    }
}

fn main() -> Result<()> {
    let options = ProgramOptions::load()?;

    let (lvl, conf) = log_config(1)?;
    TermLogger::init(lvl, conf, TerminalMode::Mixed, ColorChoice::Auto)?;

    let ex = Arc::new(Executor::new());
    smol::block_on(ex.run(start(ex.clone(), options)))
}
