use crate::{Error, Result};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::marker::PhantomData;
use std::{
    fs,
    path::{Path, PathBuf},
    str,
};

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
            Err(Error::ConfigNotFound)
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct DrkConfig {
    #[serde(default)]
    pub rpc_url: String,

    #[serde(default)]
    pub log_path: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct DarkfidConfig {
    #[serde(default)]
    #[serde(rename = "connect_url")]
    pub connect_url: String,

    #[serde(default)]
    #[serde(rename = "subscriber_url")]
    pub subscriber_url: String,

    #[serde(default)]
    #[serde(rename = "cashier_url")]
    pub cashier_url: String,

    #[serde(default)]
    #[serde(rename = "rpc_url")]
    pub rpc_url: String,

    #[serde(default)]
    #[serde(rename = "database_path")]
    pub database_path: String,

    #[serde(default)]
    #[serde(rename = "walletdb_path")]
    pub walletdb_path: String,

    #[serde(default)]
    #[serde(rename = "log_path")]
    pub log_path: String,

    #[serde(default)]
    #[serde(rename = "password")]
    pub password: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GatewaydConfig {
    #[serde(default)]
    #[serde(rename = "connect_url")]
    pub accept_url: String,

    #[serde(default)]
    #[serde(rename = "publisher_url")]
    pub publisher_url: String,

    #[serde(default)]
    #[serde(rename = "database_path")]
    pub database_path: String,

    #[serde(default)]
    #[serde(rename = "log_path")]
    pub log_path: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CashierdConfig {
    #[serde(default)]
    #[serde(rename = "accept_url")]
    pub accept_url: String,

    #[serde(default)]
    #[serde(rename = "rpc_url")]
    pub rpc_url: String,

    #[serde(default)]
    #[serde(rename = "client_database_path")]
    pub client_database_path: String,

    #[serde(default)]
    #[serde(rename = "btc_endpoint")]
    pub btc_endpoint: String,

    #[serde(default)]
    #[serde(rename = "gateway_url")]
    pub gateway_url: String,

    #[serde(default)]
    #[serde(rename = "log_path")]
    pub log_path: String,

    #[serde(default)]
    #[serde(rename = "cashierdb_path")]
    pub cashierdb_path: String,

    #[serde(default)]
    #[serde(rename = "client_walletdb_path")]
    pub client_walletdb_path: String,

    #[serde(default)]
    #[serde(rename = "password")]
    pub password: String,

    #[serde(default)]
    #[serde(rename = "client_password")]
    pub client_password: String,
}
