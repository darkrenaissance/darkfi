use std::net::SocketAddr;

use darkfi::{net, Result};

pub struct ProgramOptions {
    pub network_settings: net::Settings,
    pub log_path: Box<std::path::PathBuf>,
    pub irc_accept_addr: SocketAddr,
    pub rpc_listen_addr: SocketAddr,
}

impl ProgramOptions {
    pub fn load() -> Result<ProgramOptions> {
        let app = clap_app!(dfi =>
            (version: "0.1.0")
            (author: "Amir Taaki <amir@dyne.org>")
            (about: "Dark node")
            (@arg ACCEPT: -a --accept +takes_value "Accept address")
            (@arg SEED_NODES: -s --seeds +takes_value ... "Seed nodes")
            (@arg CONNECTS: -c --connect +takes_value ... "Manual connections")
            (@arg CONNECT_SLOTS: --slots +takes_value "Connection slots")
            (@arg EXTERNAL_ADDR: -e --external +takes_value "External address")
            (@arg LOG_PATH: --log +takes_value "Logfile path")
            (@arg IRC_ACCEPT: -r --irc +takes_value "IRC accept address")
            (@arg RPC_LISTEN: --rpc +takes_value "RPC listen address")
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

        let external_addr = if let Some(external_addr) = app.value_of("EXTERNAL_ADDR") {
            Some(external_addr.parse()?)
        } else {
            None
        };

        let log_path = Box::new(
            if let Some(log_path) = app.value_of("LOG_PATH") {
                std::path::Path::new(log_path)
            } else {
                std::path::Path::new("/tmp/darkfid.log")
            }
            .to_path_buf(),
        );

        let irc_accept_addr = if let Some(accept_addr) = app.value_of("IRC_ACCEPT") {
            accept_addr.parse()?
        } else {
            ([127, 0, 0, 1], 6667).into()
        };

        let rpc_listen_addr = if let Some(rpc_addr) = app.value_of("RPC_LISTEN") {
            rpc_addr.parse()?
        } else {
            ([127, 0, 0, 1], 8000).into()
        };

        Ok(ProgramOptions {
            network_settings: net::Settings {
                inbound: accept_addr,
                outbound_connections: connection_slots,
                external_addr,
                peers: manual_connects,
                seeds: seed_addrs,
                ..Default::default()
            },
            log_path,
            irc_accept_addr,
            rpc_listen_addr,
        })
    }
}
