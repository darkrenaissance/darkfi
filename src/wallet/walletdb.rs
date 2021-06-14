use crate::serial;
use crate::serial::{deserialize, serialize, Decodable, Encodable};
use crate::Error;
use crate::Result;
use ff::Field;
use log::*;
use rand::rngs::OsRng;
use rusqlite::{named_params, Connection};
use std::path::PathBuf;

pub struct WalletDB {}

impl WalletDB {
    pub async fn new(path: PathBuf) -> Result<()> {
        let connect = Connection::open(&path).expect("Failed to connect to database.");
        let contents = include_str!("../../res/schema.sql");
        Ok(connect.execute_batch(&contents)?)
    }

    pub fn path(wallet: &str) -> Result<PathBuf> {
        let mut path = dirs::home_dir()
            .ok_or(Error::PathNotFound)?
            .as_path()
            .join(".config/darkfi/");
        path.push(wallet);
        debug!(target: "walletdb", "CREATE PATH {:?}", path);
        Ok(path)
    }

    pub async fn save_key(path: PathBuf, pubkey: Vec<u8>, privkey: Vec<u8>) -> Result<()> {
        debug!(target: "key_gen", "Generating keys...");
        let connect = Connection::open(&path).expect("Failed to connect to database.");
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

    pub async fn create_key() -> (Vec<u8>, Vec<u8>) {
        debug!(target: "key_gen", "Generating keys...");
        let secret: jubjub::Fr = jubjub::Fr::random(&mut OsRng);
        let public = zcash_primitives::constants::SPENDING_KEY_GENERATOR * secret;
        let pubkey = serial::serialize(&public);
        let privkey = serial::serialize(&secret);
        // Write keys to database
        (pubkey, privkey)
    }

    // match statement here
    pub async fn get_public(path: PathBuf) -> Result<Vec<u8>> {
        debug!(target: "get", "Returning keys...");
        let connect = Connection::open(&path).expect("Failed to connect to database.");
        // does use unwrap here
        let mut stmt = connect.prepare("SELECT key_public FROM keys").unwrap();
        let key_iter = stmt.query_map::<u8, _, _>([], |row| row.get(0)).unwrap();
        let mut pub_keys = Vec::new();
        for key in key_iter {
            pub_keys.push(key.unwrap());
        }
        Ok(pub_keys)
    }

    // match statement here
    pub fn get_private(path: PathBuf) -> Result<Vec<u8>> {
        debug!(target: "get", "Returning keys...");
        let connect = Connection::open(&path).expect("Failed to connect to database.");
        let mut stmt = connect.prepare("SELECT key_private FROM keys").unwrap();
        let key_iter = stmt.query_map::<u8, _, _>([], |row| row.get(0)).unwrap();
        let mut keys = Vec::new();
        for key in key_iter {
            keys.push(key.unwrap());
        }
        Ok(keys)
    }

    pub async fn get_value_deserialized<D: Decodable>(key: Vec<u8>) -> Result<D> {
        let v: D = deserialize(&key)?;
        Ok(v)
    }

    pub async fn save(path: PathBuf, pubkey: Vec<u8>) -> Result<()> {
        debug!(target: "save_cash_key", "Save cashier keys...");
        //let path = Self::wallet_path();
        let connect = Connection::open(&path).expect("Failed to connect to database.");
        // Write keys to database
        connect.execute(
            "INSERT INTO cashier(key_id, key_public)
            VALUES (NULL, :pubkey)",
            named_params! {":pubkey": pubkey},
        )?;
        Ok(())
    }
}
