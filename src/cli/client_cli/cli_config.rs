use crate::serial::{Encodable, Decodable, serialize, deserialize};
use crate::Result;
use crate::util::join_config_path;

use std::{
    fs::{create_dir_all, File},
    io::prelude::*,
    path::PathBuf,
};

pub struct Config {
    pub connect_url: String,
    pub subscriber_url: String,
    pub rpc_url: String,
    pub database_path: String,
    pub log_path: String,
}

impl Default for Config {
    fn default() -> Self {
        let connect_url= String::from("127.0.0.1:3333");
        let subscriber_url= String::from("127.0.0.1:4444");
        let rpc_url= String::from("127.0.0.1:8000");
        let database_path = String::from("database_client.db"); 
        let log_path = String::from("/tmp/darkfid_service_daemon.log");
        Self {
            connect_url,
            subscriber_url,
            rpc_url,
            database_path,
            log_path
        }
    }
}

impl Config {
    pub fn load(path: PathBuf) -> Result<Config> {
        let path = join_config_path(&path)?; 
        load_config_file(path)
    }
    pub fn save(&self, path: PathBuf) -> Result <()> {
        let path = join_config_path(&path)?; 
        save_config_file(self, path)
    }
}



pub fn load_config_file(config_file: PathBuf) -> Result<Config>
{
    
    let mut file = File::open(config_file)?;
    let mut buffer: Vec<u8> = vec![];
    file.read_to_end(&mut buffer)?;
    let config: Config = deserialize(&buffer)?;

    Ok(config)
}

pub fn save_config_file(config: &Config, config_file: PathBuf) -> Result<()>
{
    let serialized = serialize(config);

    if let Some(outdir) = config_file.parent() {
        create_dir_all(outdir)?;
    }

    let mut file = File::create(config_file)?;
    file.write_all(&serialized)?;

    Ok(())
}



impl Encodable for Config {
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

impl Decodable for Config {
    fn decode<D: std::io::Read>(mut d: D) -> Result<Self> {
        Ok(Self {
            connect_url: Decodable::decode(&mut d)?,
            subscriber_url: Decodable::decode(&mut d)?,
            rpc_url: Decodable::decode(&mut d)?,
            database_path: Decodable::decode(&mut d)?,
            log_path: Decodable::decode(&mut d)?
        })
    }
}
