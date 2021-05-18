#[macro_use]
use std::net::SocketAddr;
use crate::{net, Error, Result};
use async_executor::Executor;
use async_native_tls::TlsAcceptor;
use async_std::sync::Mutex;
use easy_parallel::Parallel;
use http_types::{Request, Response, StatusCode};
use log::*;
use smol::Async;
use std::net::TcpListener;
use std::sync::Arc;

pub struct ProgramOptions {
    pub network_settings: net::Settings,
    pub log_path: Box<std::path::PathBuf>,
    pub rpc_port: u16,
}

impl ProgramOptions {
    pub fn load() -> Result<ProgramOptions> {
        let app = clap_app!(dfi =>
            (version: "0.1.0")
            (author: "Amir Taaki <amir@dyne.org>")
            (about: "Dark node")
            (@arg ACCEPT: -a --accept +takes_value "Accept add//ress")
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

        let rpc_port = if let Some(rpc_port) = app.value_of("RPC_PORT") {
            rpc_port.parse()?
        } else {
            8000
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
            log_path,
            rpc_port,
        })
    }
}
