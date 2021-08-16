use crate::util::join_config_path;
use crate::Result;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::marker::PhantomData;
use std::{
    env, fs,
    fs::{create_dir_all, File},
    io::Write,
    path::PathBuf,
    str,
};

pub struct Config<T> {
    config: PhantomData<T>,
}

impl<T: Default + Serialize + DeserializeOwned> Config<T> {
    pub fn load(path: PathBuf) -> Result<T> {
        let toml = fs::read(&path)?;
        let str_buff = str::from_utf8(&toml)?;
        let config: T = toml::from_str(str_buff.clone())?;
        Ok(config)
    }

    pub fn load_default(path: PathBuf) -> Result<T> {
        let toml = T::default();
        let config_file = toml::to_string(&toml)?;

        if let Some(outdir) = path.parent() {
            create_dir_all(outdir)?;
        }

        let mut file = File::create(path.clone())?;
        file.write_all(&config_file.into_bytes())?;

        let config = Self::load(path)?;
        Ok(config)
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct DrkConfig {
    #[serde(default)]
    pub rpc_url: String,

    #[serde(default)]
    pub log_path: String,
}

impl Default for DrkConfig {
    fn default() -> Self {
        let rpc_url = String::from("http://127.0.0.1:8000");

        let mut lp = PathBuf::new();
        lp.push(env::temp_dir());
        lp.push("drk_cli.log");
        let log_path = String::from(lp.to_str().unwrap());

        Self { rpc_url, log_path }
    }
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

impl Default for DarkfidConfig {
    fn default() -> Self {
        let connect_url = String::from("127.0.0.1:3333");
        let subscriber_url = String::from("127.0.0.1:4444");
        let cashier_url = String::from("127.0.0.1:7777");
        let rpc_url = String::from("127.0.0.1:8000");

        let database_path = String::from("database_client.db");
        let database_path = join_config_path(&PathBuf::from(database_path))
            .expect("error during join database_path to config path");
        let database_path = String::from(
            database_path
                .to_str()
                .expect("error convert Path to String"),
        );

        let walletdb_path = String::from("walletdb.db");
        let walletdb_path = join_config_path(&PathBuf::from(walletdb_path))
            .expect("error during join walletdb_path to config path");
        let walletdb_path = String::from(
            walletdb_path
                .to_str()
                .expect("error convert Path to String"),
        );

        let mut lp = PathBuf::new();
        lp.push(env::temp_dir());
        lp.push("darkfid_service_daemon.log");
        let log_path = String::from(lp.to_str().unwrap());

        let password = String::new();

        Self {
            connect_url,
            subscriber_url,
            cashier_url,
            rpc_url,
            database_path,
            walletdb_path,
            log_path,
            password,
        }
    }
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

impl Default for GatewaydConfig {
    fn default() -> Self {
        let accept_url = String::from("127.0.0.1:3333");
        let publisher_url = String::from("127.0.0.1:4444");
        let database_path = String::from("gatewayd.db");

        let mut lp = PathBuf::new();
        lp.push(env::temp_dir());
        lp.push("gatewayd.log");
        let log_path = String::from(lp.to_str().unwrap());

        Self {
            accept_url,
            publisher_url,
            database_path,
            log_path,
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CashierdConfig {
    #[serde(default)]
    #[serde(rename = "accept_url")]
    pub accept_url: String,

    #[serde(default)]
    #[serde(rename = "database_path")]
    pub database_path: String,

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
    #[serde(rename = "password")]
    pub password: String,
}

impl Default for CashierdConfig {
    fn default() -> Self {
        let accept_url = String::from("127.0.0.1:7777");
        let gateway_url = String::from("127.0.0.1:3333");
        let database_path = String::from("cashierd.db");
        let btc_endpoint = String::from("tcp://electrum.blockstream.info:50001");
        let mut lp = PathBuf::new();
        lp.push(env::temp_dir());
        lp.push("cashierd.log");
        let log_path = String::from(lp.to_str().unwrap());

        let password = String::new();
        Self {
            accept_url,
            database_path,
            btc_endpoint,
            gateway_url,
            log_path,
            password,
        }
    }
}
