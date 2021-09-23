use crate::{Error, Result};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::marker::PhantomData;
use std::{
    fs,
    path::{Path, PathBuf},
    str,
};

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
            println!("No config files were found in .config/darkfi. Please follow the instructions in the README and add default configs.");
            Err(Error::ConfigNotFound)
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct DrkConfig {
    pub rpc_url: String,
    pub log_path: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct DarkfidConfig {
    #[serde(rename = "connect_url")]
    pub connect_url: String,

    #[serde(rename = "subscriber_url")]
    pub subscriber_url: String,

    #[serde(rename = "cashier_url")]
    pub cashier_url: String,

    #[serde(rename = "rpc_url")]
    pub rpc_url: String,

    #[serde(rename = "use_tls")]
    pub use_tls: bool,

    #[serde(rename = "tls_identity_path")]
    pub tls_identity_path: String,

    #[serde(rename = "tls_identity_password")]
    pub tls_identity_password: String,

    //TODO: reimplement this
    //#[serde(rename = "database_path")]
    //pub database_path: String,
    #[serde(rename = "wallet_path")]
    pub wallet_path: String,

    #[serde(rename = "log_path")]
    pub log_path: String,

    #[serde(rename = "password")]
    pub password: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GatewaydConfig {
    #[serde(rename = "connect_url")]
    pub accept_url: String,

    #[serde(rename = "publisher_url")]
    pub publisher_url: String,

    #[serde(rename = "log_path")]
    pub log_path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FeatureNetwork {
    pub name: String,
    pub blockchain: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct CashierdConfig {
    #[serde(rename = "rpc_url")]
    pub rpc_url: String,

    #[serde(rename = "gateway_url")]
    pub gateway_url: String,

    #[serde(rename = "gateway_subscriber_url")]
    pub gateway_subscriber_url: String,

    #[serde(rename = "log_path")]
    pub log_path: String,

    #[serde(rename = "password")]
    pub password: String,

    #[serde(rename = "use_tls")]
    pub use_tls: bool,

    #[serde(rename = "tls_identity_path")]
    pub tls_identity_path: String,

    #[serde(rename = "tls_identity_password")]
    pub tls_identity_password: String,

    #[serde(rename = "client_password")]
    pub client_password: String,

    #[serde(rename = "networks")]
    pub networks: Vec<FeatureNetwork>,
}
