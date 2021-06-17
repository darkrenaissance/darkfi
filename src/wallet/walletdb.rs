use crate::serial;
use async_std::sync::Arc;
use crate::serial::{deserialize, serialize, Decodable, Encodable};
use crate::Error;
use crate::Result;
use ff::Field;
use log::*;
use rand::rngs::OsRng;
use rusqlite::{named_params, Connection};
use std::path::PathBuf;

pub struct WalletDB {
    path: PathBuf,
}

impl WalletDB {
    pub fn new(wallet: &str) -> Result<Arc<Self>> {
        let path = Self::create_path(wallet)?;
        let connect = Connection::open(&path)?;
        let contents = include_str!("../../res/schema.sql");
        connect.execute_batch(&contents)?;
        Ok(Arc::new(Self {
            path
        }))
    }

    pub fn create_path(wallet: &str) -> Result<PathBuf> {
        let mut path = dirs::home_dir()
            .ok_or(Error::PathNotFound)?
            .as_path()
            .join(".config/darkfi/");
        path.push(wallet);
        debug!(target: "walletdb", "CREATE PATH {:?}", path);
        Ok(path)
    }

    pub async fn save_key(&self, path: PathBuf, pubkey: Vec<u8>, privkey: Vec<u8>) -> Result<()> {
        debug!(target: "key_gen", "Generating keys...");
        let connect = Connection::open(&path)?;
        debug!(target: "adapter", "key_gen() [Saving public key...]");
        connect.execute(
            "INSERT INTO keys(key_id, key_private, key_public)
            VALUES (NULL, :privkey, :pubkey)",
            named_params! {
            ":privkey": privkey,
             ":pubkey": pubkey
            },
        )?;
        Ok(())
    }

    pub async fn create_key(&self) -> (Vec<u8>, Vec<u8>) {
        debug!(target: "key_gen", "Generating keys...");
        let secret: jubjub::Fr = jubjub::Fr::random(&mut OsRng);
        let public = zcash_primitives::constants::SPENDING_KEY_GENERATOR * secret;
        let pubkey = serial::serialize(&public);
        let privkey = serial::serialize(&secret);
        // Write keys to database
        (pubkey, privkey)
    }

    pub async fn get_public(&self, path: PathBuf) -> Result<Vec<u8>> {
        debug!(target: "get", "Returning keys...");
        let connect = Connection::open(&path)?;
        let mut stmt = connect.prepare("SELECT key_public FROM keys")?;
        let key_iter = stmt.query_map::<u8, _, _>([], |row| row.get(0))?;
        let mut pub_keys = Vec::new();
        for key in key_iter {
            pub_keys.push(key?);
        }
        Ok(pub_keys)
    }

    pub fn get_private(&self, path: PathBuf) -> Result<Vec<u8>> {
        debug!(target: "get", "Returning keys...");
        let connect = Connection::open(&path)?;
        let mut stmt = connect.prepare("SELECT key_private FROM keys")?;
        let key_iter = stmt.query_map::<u8, _, _>([], |row| row.get(0))?;
        let mut keys = Vec::new();
        for key in key_iter {
            keys.push(key?);
        }
        Ok(keys)
    }

    pub async fn get_value_deserialized<D: Decodable>(&self, key: Vec<u8>) -> Result<D> {
        let v: D = deserialize(&key)?;
        Ok(v)
    }

    pub async fn save(&self, path: PathBuf, pubkey: Vec<u8>) -> Result<()> {
        debug!(target: "save_cash_key", "Save cashier keys...");
        //let path = Self::wallet_path();
        let connect = Connection::open(&path)?;
        // Write keys to database
        connect.execute(
            "INSERT INTO cashier(key_id, key_public)
            VALUES (NULL, :pubkey)",
            named_params! {":pubkey": pubkey},
        )?;
        Ok(())
    }
}
