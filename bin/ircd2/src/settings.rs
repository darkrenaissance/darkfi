use std::{net::SocketAddr, path::PathBuf};

use clap::Parser;
use serde::{Deserialize, Serialize};

use darkfi::{
    cli_desc,
    util::{cli::UrlConfig, path::expand_path},
    Error, Result,
};

pub const CONFIG_FILE_CONTENTS: &[u8] = include_bytes!("../ircd_config.toml");

#[derive(Clone, Debug)]
pub struct Settings {
    pub datastore_raft: PathBuf,
    pub rpc_listener_url: SocketAddr,
    pub irc_listener_url: SocketAddr,
    pub accept_address: Option<SocketAddr>,
    pub outbound_connections: u32,
    pub connect: Vec<SocketAddr>,
    pub seeds: Vec<SocketAddr>,
}

impl Settings {
    pub fn load(args: CliArgs, config: IrcdConfig) -> Result<Self> {
        if config.datastore_raft.is_empty() {
            return Err(Error::ParseFailed("Failed to parse datastore_raft path"))
        }

        let datastore_raft = expand_path(&config.datastore_raft)?;

        let rpc_listener_url = if args.rpc.is_none() {
            SocketAddr::try_from(config.rpc_listener_url)?
        } else {
            args.rpc.unwrap()
        };

        let irc_listener_url = if args.irc.is_none() {
            SocketAddr::try_from(config.irc_listener_url)?
        } else {
            args.irc.unwrap()
        };

        let accept_address = if args.accept.is_none() {
            match config.accept_address {
                Some(addr) => {
                    let socket_addr = SocketAddr::try_from(addr)?;
                    Some(socket_addr)
                }
                None => None,
            }
        } else {
            args.accept
        };

        let outbound_connections = if args.slots == 0 {
            config.outbound_connections.unwrap_or_default()
        } else {
            args.slots
        };

        let connect = args.connect;

        let config_seeds = config
            .seeds
            .map(|addrs| {
                addrs.iter().filter_map(|addr| SocketAddr::try_from(addr.clone()).ok()).collect()
            })
            .unwrap_or_default();

        let seeds = if args.seeds.is_empty() { config_seeds } else { args.seeds };

        Ok(Settings {
            datastore_raft,
            rpc_listener_url,
            irc_listener_url,
            accept_address,
            outbound_connections,
            connect,
            seeds,
        })
    }
}

#[derive(Parser)]
#[clap(name = "ircd", about = cli_desc!(), version)]
pub struct CliArgs {
    /// Sets a custom config file
    #[clap(long)]
    pub config: Option<String>,
    /// Accept address
    #[clap(short, long)]
    pub accept: Option<SocketAddr>,
    /// Seed node (repeatable)
    #[clap(short, long)]
    pub seeds: Vec<SocketAddr>,
    /// Manual connection (repeatable)
    #[clap(short, long)]
    pub connect: Vec<SocketAddr>,
    /// Connection slots
    #[clap(long, default_value = "0")]
    pub slots: u32,
    /// External address
    #[clap(short, long)]
    pub external: Option<SocketAddr>,
    /// IRC listen address
    #[clap(short = 'r', long)]
    pub irc: Option<SocketAddr>,
    /// RPC listen address
    #[clap(long)]
    pub rpc: Option<SocketAddr>,
    /// Verbosity level
    #[clap(short, parse(from_occurrences))]
    pub verbose: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IrcdConfig {
    /// path to datastore  for raft
    pub datastore_raft: String,
    /// Path to DER-formatted PKCS#12 archive. (used only with tls listener url)
    pub tls_identity_path: String,
    /// The address where taud should bind its RPC socket
    pub rpc_listener_url: UrlConfig,
    /// IRC listen address
    pub irc_listener_url: UrlConfig,
    /// Accept address for p2p network
    pub accept_address: Option<UrlConfig>,
    /// Number of outbound connections for p2p
    pub outbound_connections: Option<u32>,
    /// The seeds for receiving ip addresses from the p2p network
    pub seeds: Option<Vec<UrlConfig>>,
}
