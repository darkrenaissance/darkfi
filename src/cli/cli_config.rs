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
            let config: T = toml::from_str(str_buff.clone())?;
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

    #[serde(rename = "database_path")]
    pub database_path: String,

    #[serde(rename = "walletdb_path")]
    pub walletdb_path: String,

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

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct CashierdConfig {
    #[serde(rename = "accept_url")]
    pub accept_url: String,

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

    #[serde(rename = "client_password")]
    pub client_password: String,
}
