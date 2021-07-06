use crate::serial::{deserialize, serialize, Decodable, Encodable};
use crate::util::join_config_path;
use crate::Result;

use std::{fs::OpenOptions, io::prelude::*, path::PathBuf};

pub trait ClientCliConfig: Encodable + Decodable + Default {
    fn load(path: PathBuf) -> Result<Self> {
        let path = join_config_path(&path)?;
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(path)?;

        let mut buffer: Vec<u8> = vec![];
        file.read_to_end(&mut buffer)?;
        if !buffer.is_empty() {
            let config: Self = deserialize(&buffer)?;
            Ok(config)
        } else {
            Ok(Self::default())
        }
    }
    fn save(&self, path: PathBuf) -> Result<()> {
        let path = join_config_path(&path)?;
        let mut file = OpenOptions::new().write(true).create(true).open(&path)?;
        let serialized = serialize(self);
        file.write_all(&serialized)?;
        Ok(())
    }
}

impl ClientCliConfig for DarkfiCliConfig {} 
impl ClientCliConfig for DarkfidCliConfig {} 

pub struct DarkfiCliConfig {
    pub rpc_url: String,
    pub log_path: String,
}

pub struct DarkfidCliConfig {
    pub connect_url: String,
    pub subscriber_url: String,
    pub rpc_url: String,
    pub database_path: String,
    pub log_path: String,
}

impl Default for DarkfiCliConfig {
    fn default() -> Self {
        let rpc_url = String::from("127.0.0.1:8000");
        let log_path = String::from("/tmp/darkfi_cli.log");
        Self {
            rpc_url,
            log_path,
        }
    }
}

impl Encodable for DarkfidCliConfig {
    fn encode<S: std::io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += self.connect_url.encode(&mut s)?;
        len += self.subscriber_url.encode(&mut s)?;
        len += self.rpc_url.encode(&mut s)?;
        len += self.database_path.encode(&mut s)?;
        len += self.log_path.encode(&mut s)?;
        Ok(len)
    }
}

impl Decodable for DarkfidCliConfig {
    fn decode<D: std::io::Read>(mut d: D) -> Result<Self> {
        Ok(Self {
            connect_url: Decodable::decode(&mut d)?,
            subscriber_url: Decodable::decode(&mut d)?,
            rpc_url: Decodable::decode(&mut d)?,
            database_path: Decodable::decode(&mut d)?,
            log_path: Decodable::decode(&mut d)?,
        })
    }
}


impl Default for DarkfidCliConfig {
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
        Self {
            connect_url,
            subscriber_url,
            rpc_url,
            database_path,
            log_path,
        }
    }
}

impl Encodable for DarkfiCliConfig {
    fn encode<S: std::io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += self.rpc_url.encode(&mut s)?;
        len += self.log_path.encode(&mut s)?;
        Ok(len)
    }
}

impl Decodable for DarkfiCliConfig {
    fn decode<D: std::io::Read>(mut d: D) -> Result<Self> {
        Ok(Self {
            rpc_url: Decodable::decode(&mut d)?,
            log_path: Decodable::decode(&mut d)?,
        })
    }
}

