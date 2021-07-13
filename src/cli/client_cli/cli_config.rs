//use crate::serial::{deserialize, serialize, Decodable, Encodable};
//use toml::{map::Map, Value};
//use crate::util::join_config_path;
//use crate::Result;
use serde::{Deserialize, Serialize};
//use log::*;

//use std::{fs::OpenOptions, io::prelude::*, path::PathBuf};

//pub trait ClientCliConfig<'a>: Default + Deserialize<'a> {
//    fn load(path: PathBuf) -> Result<Self> {
//        let path = join_config_path(&path)?;
//        let mut file = OpenOptions::new()
//            .read(true)
//            .write(true)
//            .create(true)
//            .open(path)?;
//
//        //let mut toml_map = Map::new();
//        let mut buffer = String::new();
//        file.read_to_string(&mut buffer)?;
//        //let buffer: &'static str = buffer;
//
//        //let tomlstring = toml::to_string(&file).expect("Could not encode TTOML value");
//        if !buffer.is_empty() {
//            let config: Self = toml::from_str(&buffer)?;
//            //let config = toml::to_string(&buffer).unwrap();
//            //let config: Self = deserialize(&buffer)?;
//            //Ok(config)
//            Ok(config)
//        } else {
//            Ok(Self::default())
//        }
//    }
//    fn save(&self, path: PathBuf) -> Result<()> {
//        let path = join_config_path(&path)?;
//        let mut file = OpenOptions::new().write(true).create(true).open(&path)?;
//        //let serialized = serialize(self);
//        //file.write_all(&serialized)?;
//        Ok(())
//    }
//}

//impl ClientCliConfig<'_> for DrkCliConfig {}
//impl ClientCliConfig<'_> for DarkfidCliConfig {}

#[derive(Serialize, Default, Deserialize, Debug)]
pub struct DrkConfig {
    #[serde(rename = "rpc_url")]
    pub rpc_url: String,

    #[serde(rename = "log_path")]
    pub log_path: String,
}

#[derive(Serialize, Default, Deserialize, Debug)]
pub struct DarkfidConfig {
    #[serde(rename = "connect_url")]
    pub connect_url: String,

    #[serde(rename = "subscriber_url")]
    pub subscriber_url: String,

    #[serde(rename = "rpc_url")]
    pub rpc_url: String,

    #[serde(rename = "database_path")]
    pub database_path: String,

    #[serde(rename = "log_path")]
    pub log_path: String,

    #[serde(rename = "password")]
    pub password: String,
}

//impl Default for DrkCliConfig {
//    // default toml file
//    fn default() -> Self {
//        let rpc_url = String::from("http://127.0.0.1:8000");
//        let log_path = String::from("/tmp/drk_cli.log");
//        Self {
//            rpc_url,
//            log_path,
//        }
//    }
//}

//impl Default for DarkfidCliConfig {
//    // create default config file
//    fn default() -> Self {
//        //toml::toml! {
//        //    connect-url = "127.0.0.1:3333"
//        //};
//        let connect_url = String::from("127.0.0.1:3333");
//        let subscriber_url = String::from("127.0.0.1:4444");
//        let rpc_url = String::from("127.0.0.1:8000");
//
//        let database_path = String::from("database_client.db");
//        let database_path = join_config_path(&PathBuf::from(database_path))
//            .expect("error during join database_path to config path");
//        let database_path = String::from(
//            database_path
//                .to_str()
//                .expect("error convert Path to String"),
//        );
//        let log_path = String::from("/tmp/darkfid_service_daemon.log");
//
//        let password = String::new();
//        Self {
//            connect_url,
//            subscriber_url,
//            rpc_url,
//            database_path,
//            log_path,
//            password,
//        }
//    }
//}

