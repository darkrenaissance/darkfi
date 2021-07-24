use crate::util::join_config_path;
use crate::Result;
use serde::{Deserialize, Serialize};
use std::str;

use std::fs;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Debug)]
pub struct DrkConfig {
    #[serde(default)]
    pub rpc_url: String,

    #[serde(default)]
    pub log_path: String,
}

impl DrkConfig {
    pub fn load(path: PathBuf) -> Result<Self> {
        let toml = fs::read(&path)?;
        let str_buff = str::from_utf8(&toml)?;
        let config: Self = toml::from_str(str_buff)?;
        Ok(config)
    }

    pub fn load_default(path: PathBuf) -> Result<Self> {
        let toml = Self::default();
        let config_file = toml::to_string(&toml)?;
        fs::write(&path, &config_file)?;
        let config = Self::load(path)?;
        Ok(config)
    }
}

impl Default for DrkConfig {
    fn default() -> Self {
        let rpc_url = String::from("http://127.0.0.1:8000");
        let log_path = String::from("/tmp/drk_cli.log");
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
    #[serde(rename = "rpc_url")]
    pub rpc_url: String,

    #[serde(default)]
    #[serde(rename = "database_path")]
    pub database_path: String,

    #[serde(default)]
    #[serde(rename = "log_path")]
    pub log_path: String,

    #[serde(default)]
    #[serde(rename = "password")]
    pub password: String,
}

impl DarkfidConfig {
    pub fn load(path: PathBuf) -> Result<Self> {
        let toml = fs::read(&path)?;
        let str_buff = str::from_utf8(&toml)?;
        let config: Self = toml::from_str(str_buff)?;
        Ok(config)
    }
    pub fn load_default(path: PathBuf) -> Result<Self> {
        let toml = Self::default();
        let config_file = toml::to_string(&toml)?;
        fs::write(&path, &config_file)?;
        let config = Self::load(path)?;
        Ok(config)
    }
}

impl Default for DarkfidConfig {
    fn default() -> Self {
        let connect_url = String::from("127.0.0.1:3333");
        let subscriber_url = String::from("127.0.0.1:4444");
        let rpc_url = String::from("127.0.0.1:8000");

        let database_path = String::from("database_client.db");
        let database_path = join_config_path(&PathBuf::from(database_path))
            .expect("error during join database_path to config path");
        let database_path = String::from(
            database_path
                .to_str()
                .expect("error convert Path to String"),
        );
        let log_path = String::from("/tmp/darkfid_service_daemon.log");

        let password = String::new();
        Self {
            connect_url,
            subscriber_url,
            rpc_url,
            database_path,
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

impl GatewaydConfig {
    pub fn load(path: PathBuf) -> Result<Self> {
        let toml = fs::read(&path)?;
        let str_buff = str::from_utf8(&toml)?;
        let config: Self = toml::from_str(str_buff)?;
        Ok(config)
    }
    pub fn load_default(path: PathBuf) -> Result<Self> {
        let toml = Self::default();
        let config_file = toml::to_string(&toml)?;
        fs::write(&path, &config_file)?;
        let config = Self::load(path)?;
        Ok(config)
    }
}

impl Default for GatewaydConfig {
    fn default() -> Self {
        let accept_url = String::from("127.0.0.1:3333");
        let publisher_url = String::from("127.0.0.1:4444");
        let database_path = String::from("gatewayd.db");
        let log_path = String::from("/tmp/gatewayd.log");
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
    #[serde(rename = "connect_url")]
    pub accept_url: String,

    #[serde(default)]
    #[serde(rename = "database_path")]
    pub database_path: String,

    #[serde(default)]
    #[serde(rename = "log_path")]
    pub log_path: String,

    #[serde(default)]
    #[serde(rename = "password")]
    pub password: String,
}

impl CashierdConfig {
    pub fn load(path: PathBuf) -> Result<Self> {
        let toml = fs::read(&path)?;
        let str_buff = str::from_utf8(&toml)?;
        let config: Self = toml::from_str(str_buff)?;
        Ok(config)
    }
    pub fn load_default(path: PathBuf) -> Result<Self> {
        let toml = Self::default();
        let config_file = toml::to_string(&toml)?;
        fs::write(&path, &config_file)?;
        let config = Self::load(path)?;
        Ok(config)
    }
}

impl Default for CashierdConfig {
    fn default() -> Self {
        let accept_url = String::from("127.0.0.1:7777");
        let database_path = String::from("cashierd.db");
        let log_path = String::from("/tmp/cashierd.log");
        let password = String::new();
        Self {
            accept_url,
            database_path,
            log_path,
            password,
        }
    }
}
