use std::{
    env, fs,
    io::Write,
    marker::PhantomData,
    net::SocketAddr,
    path::{Path, PathBuf},
    str,
};

use serde::{de::DeserializeOwned, Deserialize, Serialize};

use clap::ArgMatches;
use simplelog::ConfigBuilder;

use crate::{Error, Result};

#[derive(Clone, Default)]
pub struct Config<T> {
    config: PhantomData<T>,
}

impl<T: Serialize + DeserializeOwned> Config<T> {
    pub fn load(path: PathBuf) -> Result<T> {
        if Path::new(&path).exists() {
            let toml = fs::read(&path)?;
            let str_buff = str::from_utf8(&toml)?;
            let config: T = toml::from_str(str_buff)?;
            Ok(config)
        } else {
            let path = path.to_str();
            if path.is_some() {
                println!("Could not find/parse configuration file in: {}", path.unwrap());
            } else {
                println!("Could not find/parse configuration file");
            }
            println!("Please follow the instructions in the README");
            Err(Error::ConfigNotFound)
        }
    }
}

/// The configuration for drk
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct DrkConfig {
    /// The URL where darkfid RPC is listening on
    pub darkfid_rpc_url: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Cashier {
    /// Cashier name
    pub name: String,
    /// The RPC endpoint for a selected cashier
    pub rpc_url: String,
    /// The selected cashier public key
    pub public_key: String,
}

/// The configuration for darkfid
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct DarkfidConfig {
    /// The address where darkfid should bind its RPC socket
    pub rpc_listen_address: SocketAddr,
    /// Whether to listen with TLS or plain TCP
    pub serve_tls: bool,
    /// Path to DER-formatted PKCS#12 archive. (Unused if serve_tls=false)
    pub tls_identity_path: String,
    /// Password for the TLS identity. (Unused if serve_tls=false)
    pub tls_identity_password: String,
    /// The endpoint to a gatewayd protocol API
    pub gateway_protocol_url: String,
    /// The endpoint to a gatewayd publisher API
    pub gateway_publisher_url: String,
    /// Path to the client database
    pub database_path: String,
    /// Path to the wallet database
    pub wallet_path: String,
    /// The wallet password
    pub wallet_password: String,
    /// The configured cashiers to use
    pub cashiers: Vec<Cashier>,
}

/// The configuration for gatewayd
#[derive(Serialize, Deserialize, Debug)]
pub struct GatewaydConfig {
    /// The address where gatewayd should bind its protocol socket
    pub protocol_listen_address: SocketAddr,
    /// The address where gatewayd should bind its publisher socket
    pub publisher_listen_address: SocketAddr,
    /// Whether to listen with TLS or plain TCP
    pub serve_tls: bool,
    /// Path to DER-formatted PKCS#12 archive. (Unused if serve_tls=false)
    pub tls_identity_path: String,
    /// Password for the TLS identity. (Unused if serve_tls=false)
    pub tls_identity_password: String,
    /// Path to the database
    pub database_path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FeatureNetwork {
    /// Network name
    pub name: String,
    /// Blockchain (mainnet/testnet/etc.)
    pub blockchain: String,
    /// Keypair
    pub keypair: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct CashierdConfig {
    /// The DNS name of the cashier (can also be an IP, or a .onion address)
    pub dns_addr: String,
    /// The endpoint where cashierd will bind its RPC socket
    pub rpc_listen_address: SocketAddr,
    /// Whether to listen with TLS or plain TCP
    pub serve_tls: bool,
    /// Path to DER-formatted PKCS#12 archive. (Unused if serve_tls=false)
    pub tls_identity_path: String,
    /// Password for the TLS identity. (Unused if serve_tls=false)
    pub tls_identity_password: String,
    /// The endpoint to a gatewayd protocol API
    pub gateway_protocol_url: String,
    /// The endpoint to a gatewayd publisher API
    pub gateway_publisher_url: String,
    /// Path to cashierd wallet
    pub cashier_wallet_path: String,
    /// Password for cashierd wallet
    pub cashier_wallet_password: String,
    /// Path to client wallet
    pub client_wallet_path: String,
    /// Password for client wallet
    pub client_wallet_password: String,
    /// Path to database
    pub database_path: String,
    /// Geth IPC endpoint
    pub geth_socket: String,
    /// Geth passphrase
    pub geth_passphrase: String,
    /// The configured networks to use
    pub networks: Vec<FeatureNetwork>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct MapConfig {
    pub nodes: Vec<IrcNode>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct IrcNode {
    pub node_id: String,
}

pub fn spawn_config(path: &Path, contents: &[u8]) -> Result<()> {
    if !path.exists() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut file = fs::File::create(path)?;
        file.write_all(contents)?;
        println!("Config file created in `{:?}`. Please review it and try again.", path);
        std::process::exit(2);
    }

    Ok(())
}

pub fn log_config(matches: ArgMatches) -> Result<(simplelog::LevelFilter, simplelog::Config)> {
    let mut verbosity_level = 0;
    verbosity_level += matches.occurrences_of("verbose");
    let log_level = match verbosity_level {
        0 => simplelog::LevelFilter::Info,
        1 => simplelog::LevelFilter::Debug,
        _ => simplelog::LevelFilter::Trace,
    };

    let log_config = match env::var("LOG_TARGETS") {
        Ok(x) => {
            let targets: Vec<&str> = x.split(',').collect();
            let mut cfgbuilder = ConfigBuilder::new();
            for i in targets {
                cfgbuilder.add_filter_allow(i.to_string());
            }

            cfgbuilder.build()
        }
        Err(_) => simplelog::Config::default(),
    };

    Ok((log_level, log_config))
}
