use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    marker::PhantomData,
    path::{Path, PathBuf},
    str,
};

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
            println!("Could not parse configuration");
            println!("Please follow the instructions in the README");
            Err(Error::ConfigNotFound)
        }
    }
}

/// The configuration for drk
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct DrkConfig {
    /// The URL where darkfid is listening on.
    pub darkfid_url: String,
}

/// The configuration for darkfid
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct DarkfidConfig {
    /// The address where darkfid should bind its RPC socket
    pub listen_address: String,
    /// Whether to listen with TLS or plain TCP
    pub serve_tls: bool,
    /// Path to DER-formatted PKCS#12 archive. (Unused if serve_tls=false)
    pub tls_identity_path: String,
    /// Password for the TLS identity. (Unused if serve_tls=false)
    pub tls_identity_password: String,
    /// The RPC endpoint for a selected cashier
    pub cashier_url: String,
    /// Path to the client database
    pub database_path: String,
    /// Path to the wallet database
    pub wallet_path: String,
    /// The wallet password
    pub wallet_password: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GatewaydConfig {
    pub accept_url: String,
    pub publisher_url: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FeatureNetwork {
    /// Network name
    pub name: String,
    /// Blockchain (mainnet/testnet/etc.)
    pub blockchain: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct CashierdConfig {
    /// The endpoint where cashierd will bind its RPC socket
    pub listen_url: String,
    /// Whether to listen with TLS or plain TCP
    pub serve_tls: bool,
    /// Path to DER-formatted PKCS#12 archive. (Unused if serve_tls=false)
    pub tls_identity_path: String,
    /// Password for the TLS identity. (Unused if serve_tls=false)
    pub tls_identity_password: String,
    /// ?
    pub gateway_url: String,
    /// ?
    pub gateway_subscriber_url: String,
    /// Path to mint.params
    pub mint_params: String,
    /// Path to spend.params
    pub spend_params: String,
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
    /// The configured networks to use
    pub networks: Vec<FeatureNetwork>,
}
