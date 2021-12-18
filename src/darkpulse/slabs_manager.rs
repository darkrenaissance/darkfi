use std::collections::HashMap;
use std::sync::Arc;

use log::*;
use sha2::{Digest, Sha256};

use crate:: Result;
use super::{dbsql, net::messages::SlabMessage, channel::Channel, CiphertextHash, aes::Ciphertext};

pub fn cipher_hash(ciphertext: &Ciphertext) -> CiphertextHash {
    let mut cipher_hash = [0u8; 32];
    let mut hasher = Sha256::new();
    for chunk in ciphertext.chunks(32) {
        hasher.update(chunk);
    }
    cipher_hash.copy_from_slice(&hasher.finalize());
    cipher_hash
}

impl SlabMessage {
    pub fn cipher_hash(&self) -> CiphertextHash {
        cipher_hash(&self.ciphertext)
    }
}

pub type SlabsManagerSafe = Arc<async_std::sync::Mutex<SlabsManager>>;

pub struct SlabsManager {
    slabs: HashMap<CiphertextHash, SlabMessage>,
    notify_update: async_channel::Sender<CiphertextHash>,
    main_channel: Channel,
    db: dbsql::Dbsql,
}

impl SlabsManager {
    pub async fn new(
        db: dbsql::Dbsql,
        notify_update: async_channel::Sender<CiphertextHash>,
        main_channel: Channel,
    ) -> SlabsManagerSafe {
        let mut slabs: HashMap<CiphertextHash, SlabMessage> = HashMap::new();

        if let Some(channel_id) = main_channel.get_channel_id() {
            slabs = db.get_channel_slabs(channel_id.clone()).unwrap_or(slabs);
        }

        Arc::new(async_std::sync::Mutex::new(SlabsManager {
            slabs,
            notify_update,
            main_channel,
            db,
        }))
    }

    pub fn get_slabs_hash(&self) -> Vec<CiphertextHash> {
        self.slabs.keys().cloned().collect()
    }
    pub fn height(&self) -> u32 {
        self.slabs.len() as u32
    }
    pub fn has_cipher_hash(&self, cipher_hash: &CiphertextHash) -> bool {
        self.slabs.contains_key(cipher_hash)
    }

    pub async fn add_new_slab(&mut self, slab: SlabMessage) -> Result<()> {
        info!("received Slab message.");
        self.slabs.insert(slab.cipher_hash(), slab.clone());

        if let Some(channel_id) = self.main_channel.get_channel_id() {
            self.db.add_slab(&slab, channel_id)?;
        }

        self.notify_update.send(slab.cipher_hash()).await?;
        Ok(())
    }

    pub fn set_main_channel(&mut self, main_channel: Channel) {
        self.main_channel = main_channel;
        self.switch_main_channel();
    }

    pub fn switch_main_channel(&mut self) {
        let default_slabs: HashMap<CiphertextHash, SlabMessage> = HashMap::new();

        if let Some(channel_id) = self.main_channel.get_channel_id() {
            self.slabs = self
                .db
                .get_channel_slabs(channel_id.clone())
                .unwrap_or(default_slabs);
        }
    }

    pub fn get_channels(&mut self) -> Result<Vec<Channel>> {
        self.db.get_channels()
    }

    pub fn add_new_channel(&mut self, new_channel: &Channel) -> Result<()> {
        self.db.add_channel(new_channel)?;
        Ok(())
    }

    pub fn delete_channel(&mut self, channel_id: &String) -> Result<()> {
        self.db.delete_channel(channel_id)?;
        Ok(())
    }

    pub fn add_username(&mut self, username: &String) -> Result<()> {
        self.db.add_username(username)?;
        Ok(())
    }

    pub fn get_main_channel(&self) -> Channel {
        self.main_channel.clone()
    }

    pub fn get_slab(&self, cipher_hash: &CiphertextHash) -> Option<&SlabMessage> {
        self.slabs.get(cipher_hash)
    }

    pub fn get_slabs(&self) -> &HashMap<CiphertextHash, SlabMessage> {
        &self.slabs
    }
}
