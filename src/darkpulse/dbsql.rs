use std::{collections::HashMap, convert::TryInto, fs::File, io::prelude::*};

use rusqlite::{params, Connection};

use super::{utility::default_config_dir, Channel, CiphertextHash, SlabMessage};
use crate::Result;

#[derive(Debug)]
pub struct Dbsql {
    connection: Connection,
    username: String,
}

impl Dbsql {
    pub fn new() -> Result<Dbsql> {
        let path = default_config_dir()?.join("data.db");
        let connection = Connection::open(path)?;
        let username = String::new();
        Ok(Dbsql { connection, username })
    }

    pub fn start(&mut self) -> Result<()> {
        let schemas = Self::read_schemas_from_file("../../sql/darkpulse_schema.sql")?;
        self.connection.execute_batch(schemas.as_str())?;

        Ok(())
    }

    pub fn add_slab(&self, slab: &SlabMessage, channel_id: &u32) -> Result<()> {
        self.connection.execute(
            "INSERT OR IGNORE INTO slab (nonce, cipher_text, cipher_text_hash, channel_id) VALUES (?1, ?2, ?3, ?4)",
            params![&slab.nonce[..], &slab.ciphertext[..], &slab.cipher_hash()[..], channel_id],
        )?;
        Ok(())
    }

    pub fn add_username(&self, username: &String) -> Result<()> {
        self.connection
            .execute("INSERT OR IGNORE INTO node (username) VALUES (?1)", params![username])?;
        Ok(())
    }

    pub fn get_channel_slabs(&self, id: u32) -> Result<HashMap<CiphertextHash, SlabMessage>> {
        let stmt = format!("SELECT * FROM slab WHERE channel_id={}", id);
        let mut stmt = self.connection.prepare(&stmt)?;

        let mut slabs: HashMap<CiphertextHash, SlabMessage> = HashMap::new();
        let slab_iter = stmt.query_map(params![], |row| {
            let nonce: Vec<u8> = row.get(1)?;
            let nonce = nonce
                .as_slice()
                .try_into()
                .expect("error when converting vector to slice with size [u8; 12]");

            let ciphertext = row.get(2)?;

            Ok(SlabMessage { nonce, ciphertext })
        })?;

        for slab in slab_iter {
            let slab = slab?;
            slabs.insert(slab.cipher_hash(), slab);
        }
        Ok(slabs)
    }

    pub fn add_channel(&self, channel: &Channel) -> Result<()> {
        self.connection.execute(
            "INSERT OR IGNORE INTO channel (channel_name, channel_secret, address) VALUES (?1, ?2, ?3)",
            params![
            &channel.get_channel_name(),
            &channel.get_channel_secret()[..],
            &channel.get_channel_address()
            ],
        )?;
        Ok(())
    }

    pub fn delete_channel(&self, channel_name: &String) -> Result<()> {
        self.connection
            .execute("DELETE FROM channel WHERE channel_name = (?1)", params![channel_name,])?;
        Ok(())
    }

    fn read_schemas_from_file(path: &str) -> Result<String> {
        let mut file = File::open(path)?;
        let mut schemas = String::new();
        file.read_to_string(&mut schemas)?;
        Ok(schemas)
    }

    pub fn get_slabs(&mut self) -> Result<HashMap<CiphertextHash, SlabMessage>> {
        let mut slabs = HashMap::new();
        let mut stmt = self.connection.prepare("SELECT * FROM slab")?;
        let slab_iter = stmt.query_map(params![], |row| {
            let nonce: Vec<u8> = row.get(1)?;

            let nonce: [u8; 12] = nonce
                .as_slice()
                .try_into()
                .expect("error when converting vector to slice with size [u8; 12]");

            let ciphertext = row.get(2)?;

            Ok(SlabMessage { nonce, ciphertext })
        })?;

        for slab in slab_iter {
            let slab = slab?;
            slabs.insert(slab.cipher_hash(), slab);
        }

        Ok(slabs)
    }

    pub fn get_channels(&self) -> Result<Vec<Channel>> {
        let mut channels = Vec::new();
        let mut stmt = self.connection.prepare("SELECT * FROM channel")?;
        let channel_iter = stmt.query_map(params![], |row| {
            let channel_id = row.get(0)?;
            let channel_name = row.get(1)?;
            let channel_secret: Vec<u8> = row.get(2)?;

            let channel_secret: [u8; 32] = channel_secret
                .as_slice()
                .try_into()
                .expect("error when converting vector to slice with size [u8; 32]");

            let address = row.get(3)?;
            Ok(Channel::new(channel_name, channel_secret, address, channel_id))
        })?;

        for channel in channel_iter {
            let channel = channel?;
            channels.push(channel);
        }

        Ok(channels.clone())
    }

    pub fn get_username(&self) -> Result<String> {
        let mut username = String::new();
        let mut stmt3 = self.connection.prepare("SELECT * FROM node")?;
        let mut uname_iter = stmt3.query_map(params![], |row| {
            let username: String = row.get(1)?;
            Ok(username)
        })?;

        for name in uname_iter.next() {
            username = name?;
        }

        Ok(username)
    }
}
