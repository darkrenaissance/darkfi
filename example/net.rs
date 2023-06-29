/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use std::sync::Arc;

use clap::Parser;
use simplelog::*;
use smol::Executor;

use darkfi::{
    net,
    util::cli::{get_log_config, get_log_level},
    Result,
};

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
    pub connect_slots: Option<usize>,
    /// RPC port
    #[clap(long)]
    pub rpc_port: Option<String>,
}

impl ProgramOptions {
    fn load() -> Result<ProgramOptions> {
        let programcli = DarkCli::parse();

        let accept_addr = if let Some(accept_addr) = programcli.accept {
            vec![accept_addr.parse()?]
        } else {
            vec![]
        };

        let mut seed_addrs: Vec<url::Url> = vec![];
        if let Some(seeds) = programcli.seeds {
            for seed in seeds {
                seed_addrs.push(seed.parse()?);
            }
        }

        let mut manual_connects: Vec<url::Url> = vec![];
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
                inbound_addrs: accept_addr.clone(),
                outbound_connections: connection_slots,
                external_addrs: accept_addr,
                peers: manual_connects,
                seeds: seed_addrs,
                ..Default::default()
            },
        })
    }
}

fn main() -> Result<()> {
    let options = ProgramOptions::load()?;

    let lvl = get_log_level(1);
    let conf = get_log_config(1);

    TermLogger::init(lvl, conf, TerminalMode::Mixed, ColorChoice::Auto)?;

    let ex = Arc::new(Executor::new());
    smol::block_on(ex.run(start(ex.clone(), options)))
}
