use clap::{App, Arg, ArgMatches};
use std::net::SocketAddr;

use darkfi::{net, Result};

pub struct ProgramOptions {
    pub network_settings: net::Settings,
    pub log_path: Box<std::path::PathBuf>,
    pub irc_accept_addr: SocketAddr,
    pub rpc_listen_addr: SocketAddr,
    pub app: ArgMatches,
}

impl ProgramOptions {
    pub fn load() -> Result<ProgramOptions> {
        let app = App::new("dfi")
            .version("0.1.0")
            .author("Amir Taaki <amir@dyne.org>")
            .about("Dark node")
            .arg(
                Arg::new("ACCEPT")
                    .short('a')
                    .long("accept")
                    .value_name("ACCEPT")
                    .help("Accept address")
                    .takes_value(true),
            )
            .arg(
                Arg::new("SEED_NODES")
                    .short('s')
                    .long("seeds")
                    .value_name("SEED_NODES")
                    .help("Seed nodes")
                    .takes_value(true),
            )
            .arg(
                Arg::new("CONNECTS")
                    .short('c')
                    .long("connect")
                    .value_name("CONNECTS")
                    .help("Manual connections")
                    .takes_value(true),
            )
            .arg(
                Arg::new("CONNECT_SLOTS")
                    .long("slots")
                    .value_name("CONNECT_SLOTS")
                    .help("Connection slots")
                    .takes_value(true),
            )
            .arg(
                Arg::new("EXTERNAL_ADDR")
                    .short('e')
                    .long("external")
                    .value_name("EXTERNAL_ADDR")
                    .help("External address")
                    .takes_value(true),
            )
            .arg(
                Arg::new("LOG_PATH")
                    .long("log")
                    .value_name("LOG_PATH")
                    .help("Logfile path")
                    .takes_value(true),
            )
            .arg(
                Arg::new("IRC_ACCEPT")
                    .short('r')
                    .long("irc")
                    .value_name("IRC_ACCEPT")
                    .help("IRC accept address")
                    .takes_value(true),
            )
            .arg(
                Arg::new("RPC_LISTEN")
                    .long("rpc")
                    .value_name("RPC_LISTEN")
                    .help("RPC listen address")
                    .takes_value(true),
            )
            .arg(
                Arg::new("verbose")
                    .short('v')
                    .long("verbose")
                    .multiple_occurrences(true)
                    .help("Sets the level of verbosity"),
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
            app,
        })
    }
}
