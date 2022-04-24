use std::{fs::create_dir_all, net::SocketAddr, path::PathBuf};

use clap::Parser;
use serde::{Deserialize, Serialize};

use darkfi::{
    util::{
        cli::UrlConfig,
        expand_path,
        serial::{SerialDecodable, SerialEncodable},
    },
    Error, Result,
};

pub const CONFIG_FILE_CONTENTS: &[u8] = include_bytes!("../../taud_config.toml");

#[derive(Clone, Debug)]
pub struct Settings {
    pub dataset_path: PathBuf,
    pub datastore_raft: PathBuf,
    pub rpc_listener_url: SocketAddr,
    pub accept_address: Option<SocketAddr>,
    pub outbound_connections: u32,
    pub connect: Vec<SocketAddr>,
    pub seeds: Vec<SocketAddr>,
}

impl Settings {
    pub fn load(args: CliTaud, config: TauConfig) -> Result<Self> {
        if config.dataset_path.is_empty() {
            return Err(Error::ParseFailed("Failed to parse dataset_path"))
        }

        let dataset_path = expand_path(&config.dataset_path)?;

        // mkdir dataset_path if not exists
        create_dir_all(dataset_path.join("month"))?;
        create_dir_all(dataset_path.join("task"))?;

        if config.datastore_raft.is_empty() {
            return Err(Error::ParseFailed("Failed to parse datastore_raft path"))
        }

        let datastore_raft = expand_path(&config.datastore_raft)?;

        let rpc_listener_url = SocketAddr::try_from(config.rpc_listener_url)?;

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
            dataset_path,
            datastore_raft,
            rpc_listener_url,
            accept_address,
            outbound_connections,
            connect,
            seeds,
        })
    }
}

#[derive(
    Clone, Debug, Serialize, Deserialize, SerialEncodable, SerialDecodable, PartialEq, PartialOrd,
)]
pub struct Timestamp(pub i64);

/// taud cli
#[derive(Parser)]
#[clap(name = "taud")]
pub struct CliTaud {
    /// Sets a custom config file
    #[clap(long)]
    pub config: Option<String>,
    /// Raft Accept address
    #[clap(short, long)]
    pub accept: Option<SocketAddr>,
    /// Raft Seed nodes (repeatable)
    #[clap(short, long)]
    pub seeds: Vec<SocketAddr>,
    /// Raft Manual connection (repeatable)
    #[clap(short, long)]
    pub connect: Vec<SocketAddr>,
    /// Raft Connection slots
    #[clap(long, default_value = "0")]
    pub slots: u32,
    /// Increase verbosity
    #[clap(short, parse(from_occurrences))]
    pub verbose: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TauConfig {
    /// path to dataset
    pub dataset_path: String,
    /// path to datastore  for raft
    pub datastore_raft: String,
    /// Path to DER-formatted PKCS#12 archive. (used only with tls listener url)
    pub tls_identity_path: String,
    /// The address where taud should bind its RPC socket
    pub rpc_listener_url: UrlConfig,
    /// Accept address for p2p network
    pub accept_address: Option<UrlConfig>,
    /// Number of outbound connections for p2p
    pub outbound_connections: Option<u32>,
    /// The seeds for receiving ip addresses from the p2p network
    pub seeds: Option<Vec<UrlConfig>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_free_id_test() -> Result<()> {
        let mut ids: Vec<u32> = vec![1, 3, 8, 9, 10, 3];
        let ids_empty: Vec<u32> = vec![];
        let ids_duplicate: Vec<u32> = vec![1; 100];

        let find_id = find_free_id(&ids);

        assert_eq!(find_id, 2);

        ids.push(find_id);

        assert_eq!(find_free_id(&ids), 4);

        assert_eq!(find_free_id(&ids_empty), 1);

        assert_eq!(find_free_id(&ids_duplicate), 2);

        Ok(())
    }
}
