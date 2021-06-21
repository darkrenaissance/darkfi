use crate::crypto::{coin::Coin, merkle::IncrementalWitness, merkle_node::MerkleNode, note::Note};
use crate::serial;
use crate::serial::{deserialize, Decodable};
use crate::Error;
use crate::Result;
use async_std::sync::Arc;
use ff::Field;
use log::*;
use rand::rngs::OsRng;
use rusqlite::{named_params, Connection, OpenFlags};
use std::path::PathBuf;

pub struct WalletDB {
    pub path: PathBuf,
    pub secrets: Vec<jubjub::Fr>,
    pub cashier_secrets: Vec<jubjub::Fr>,
    pub own_coins: Vec<(Coin, Note, jubjub::Fr, IncrementalWitness<MerkleNode>)>,
    pub cashier_public: jubjub::SubgroupPoint,
    //conn: Arc<Connection>,
}

impl WalletDB {
    pub fn new(wallet: &str) -> Result<Self> {
        let path = Self::create_path(wallet)?;
        let conn = Connection::open(&path)?;
        //let conn = Arc::new(Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_NO_MUTEX)?);
        let contents = include_str!("../../res/schema.sql");
        let cashier_secret = jubjub::Fr::random(&mut OsRng);
        let secret = jubjub::Fr::random(&mut OsRng);
        let _public = zcash_primitives::constants::SPENDING_KEY_GENERATOR * secret;
        let cashier_public = zcash_primitives::constants::SPENDING_KEY_GENERATOR * cashier_secret;
        conn.execute_batch(&contents)?;
        Ok(Self {
            path,
            own_coins: vec![],
            cashier_secrets: vec![cashier_secret.clone()],
            secrets: vec![secret.clone()],
            cashier_public,
            //conn,
        })
    }

    fn create_path(wallet: &str) -> Result<PathBuf> {
        let mut path = dirs::home_dir()
            .ok_or(Error::PathNotFound)?
            .as_path()
            .join(".config/darkfi/");
        path.push(wallet);
        debug!(target: "walletdb", "CREATE PATH {:?}", path);
        Ok(path)
    }

    //fn get_path() -> Result<PathBuf> {
    //    Ok(self.path)
    //}

    pub async fn put_key(&self, pubkey: Vec<u8>, privkey: Vec<u8>) -> Result<()> {
        //debug!(target: "key_gen", "Generating keys...");
        let conn = Connection::open(&self.path)?;
        //debug!(target: "adapter", "key_gen() [Saving public key...]");
        conn.execute(
            "INSERT INTO keys(key_id, key_private, key_public)
            VALUES (NULL, :privkey, :pubkey)",
            named_params! {
            ":privkey": privkey,
             ":pubkey": pubkey
            },
        )?;
        Ok(())
    }

    pub async fn key_gen(&self) -> (Vec<u8>, Vec<u8>) {
        debug!(target: "key_gen", "Generating keys...");
        let secret: jubjub::Fr = jubjub::Fr::random(&mut OsRng);
        let public = zcash_primitives::constants::SPENDING_KEY_GENERATOR * secret;
        let pubkey = serial::serialize(&public);
        let privkey = serial::serialize(&secret);
        (pubkey, privkey)
    }

    pub async fn get_public(&self) -> Result<Vec<u8>> {
        debug!(target: "get", "Returning keys...");
        let conn = Connection::open(&self.path)?;
        let mut stmt = conn.prepare("SELECT key_public FROM keys")?;
        let key_iter = stmt.query_map::<u8, _, _>([], |row| row.get(0))?;
        let mut pub_keys = Vec::new();
        for key in key_iter {
            pub_keys.push(key?);
        }
        Ok(pub_keys)
    }

    pub fn get_private(&self) -> Result<Vec<u8>> {
        debug!(target: "get", "Returning keys...");
        let conn = Connection::open(&self.path)?;
        let mut stmt = conn.prepare("SELECT key_private FROM keys")?;
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

    pub async fn put_cashier_pub(&self, pubkey: Vec<u8>) -> Result<()> {
        debug!(target: "save_cash_key", "Save cashier keys...");
        let conn = Connection::open(&self.path)?;
        // Write keys to database
        conn.execute(
            "INSERT INTO cashier(key_id, key_public)
            VALUES (NULL, :pubkey)",
            named_params! {":pubkey": pubkey},
        )?;
        Ok(())
    }

    pub async fn is_valid_cashier_pub(&self, public: &jubjub::SubgroupPoint) -> Result<bool> {
        let conn = Connection::open(&self.path)?;
        let mut stmt = conn
            .prepare("SELECT key_public FROM cashier WHERE key_public IN (SELECT key_public)")
            .expect("Cannot generate statement.");
        Ok(stmt.exists([1i32])?)
    }
}
