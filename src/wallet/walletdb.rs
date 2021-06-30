use crate::crypto::{coin::Coin, merkle::IncrementalWitness, merkle_node::MerkleNode, note::Note};
use crate::serial;
use crate::serial::{deserialize, serialize, Decodable, Encodable};
use crate::Error;
use crate::Result;
use async_std::sync::{Arc, Mutex};
use ff::Field;
use log::*;
use rand::rngs::OsRng;
use rusqlite::{named_params, Connection, OpenFlags};
use std::path::{Path, PathBuf};

pub type WalletPtr = Arc<WalletDB>;

// function: given keyID, get corresponding secret
pub struct WalletDB {
    pub path: PathBuf,
    pub secrets: Vec<jubjub::Fr>,
    pub cashier_secrets: Vec<jubjub::Fr>,
    pub coins: Mutex<Vec<Coin>>,
    pub notes: Mutex<Vec<Note>>,
    pub witnesses: Mutex<Vec<IncrementalWitness<MerkleNode>>>,
    // wrap in mutex or read/write lock
    //pub own_coins: Vec<(Coin, Note, jubjub::Fr, IncrementalWitness<MerkleNode>)>,
    pub cashier_public: jubjub::SubgroupPoint,
    pub public: jubjub::SubgroupPoint,
    //conn: Arc<Connection>,
}

impl WalletDB {
    pub fn new(wallet: &str) -> Result<Self> {
        debug!(target: "walletdb", "new() Constructor called");
        let path = Self::create_path(wallet)?;
        //let conn = Connection::open(&path)?;
        //debug!(target: "walletdb", "OPENED CONNECTION AT PATH {:?}", path);
        //let contents = include_str!("../../res/schema.sql");
        let cashier_secret = jubjub::Fr::random(&mut OsRng);
        let secret = jubjub::Fr::random(&mut OsRng);
        let public = zcash_primitives::constants::SPENDING_KEY_GENERATOR * secret;
        let cashier_public = zcash_primitives::constants::SPENDING_KEY_GENERATOR * cashier_secret;
        let coins = Mutex::new(Vec::new());
        let notes = Mutex::new(Vec::new());
        let witnesses = Mutex::new(Vec::new());
        //match conn.execute_batch(&contents) {
        //    Ok(v) => println!("Database initalized successfully {:?}", v),
        //    Err(err) => println!("Error: {}", err),
        //};
        debug!(target: "walletdb", "new(): wallet constructor called");
        Ok(Self {
            path,
            //own_coins: vec![],
            cashier_secrets: vec![cashier_secret.clone()],
            secrets: vec![secret.clone()],
            cashier_public,
            public,
            coins,
            notes,
            witnesses,
            //conn,
        })
    }

    //coin, serial, value, asset_id, coin_blind, valcom_blind, witness, key_id
    pub async fn put_own_coins(
        &self,
        coin: Coin,
        note: Note,
        witness: IncrementalWitness<MerkleNode>,
    ) -> Result<()> {
        let coin = self.get_value_serialized(&coin.repr).await?;
        //let coin = self.get_value_serialized(&self.own_coins[0].0.repr).await?;
        //let note = &self.own_coins[0].1;
        let serial = self.get_value_serialized(&note.serial).await?;
        let coin_blind = self.get_value_serialized(&note.coin_blind).await?;
        let valcom_blind = self.get_value_serialized(&note.valcom_blind).await?;
        let value = self.get_value_serialized(&note.value).await?;
        let asset_id = self.get_value_serialized(&note.asset_id).await?;
        let conn = Connection::open(&self.path)?;
        let witness = self.get_value_serialized(&witness).await?;
        //let witness = self.get_value_serialized(&self.own_coins[0].3).await?;
        conn.execute(
            "INSERT INTO coins(coin, serial, value, asset_id, coin_blind, valcom_blind, witness, key_id)
            VALUES (NULL, :coin, :serial, :value, :asset_id, :coin_blind, :valcom_blind, :witness, :key_id)",
            named_params! {
            ":coin": coin,
            ":serial": serial,
            ":value": value,
            ":asset_id": asset_id,
            ":coin_blind": coin_blind,
            ":valcom_blind": valcom_blind,
            ":witness": witness,
            },
        )?;
        Ok(())
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

    pub async fn key_gen(&self) -> (Vec<u8>, Vec<u8>) {
        debug!(target: "key_gen", "Generating keys...");
        let secret: jubjub::Fr = jubjub::Fr::random(&mut OsRng);
        let public = zcash_primitives::constants::SPENDING_KEY_GENERATOR * secret;
        let pubkey = serial::serialize(&public);
        let privkey = serial::serialize(&secret);
        (pubkey, privkey)
    }

    pub async fn cash_key_gen(&self) -> (Vec<u8>, Vec<u8>) {
        debug!(target: "cash key_gen", "Generating cashier keys...");
        let secret: jubjub::Fr = jubjub::Fr::random(&mut OsRng);
        let public = zcash_primitives::constants::SPENDING_KEY_GENERATOR * secret;
        let pubkey = serial::serialize(&public);
        let privkey = serial::serialize(&secret);
        (pubkey, privkey)
    }

    pub async fn put_keypair(&self, pubkey: Vec<u8>, privkey: Vec<u8>) -> Result<()> {
        let conn = Connection::open(&self.path)?;
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

    pub async fn get_cashier_public(&self) -> Result<Vec<u8>> {
        debug!(target: "get_cashier_public", "Returning keys...");
        let conn = Connection::open(&self.path)?;
        let mut stmt = conn.prepare("SELECT key_public FROM cashier")?;
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

    pub async fn get_value_serialized<T: Encodable>(&self, data: &T) -> Result<Vec<u8>> {
        let v = serialize(data);
        Ok(v)
    }
    pub async fn get_value_deserialized<D: Decodable>(&self, key: Vec<u8>) -> Result<D> {
        let v: D = deserialize(&key)?;
        Ok(v)
    }
}
