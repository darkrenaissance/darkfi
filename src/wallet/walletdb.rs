use async_std::sync;
use bellman::groth16;
use bls12_381::Bls12;
use crate::{Error, Result};
use crate::serial;
use ff::{Field, PrimeField};
use log::*;
use rand::rngs::OsRng;
use rocksdb::DB;
use rusqlite::{named_params, Connection};
use std::fs::File;
use std::path::{Path, PathBuf};

pub struct DBInterface {}

impl DBInterface {
    pub fn wallet_path() -> PathBuf {
        let path = dirs::home_dir()
            .expect("cannot find home directory.")
            .as_path()
            .join(".config/darkfi/wallet.db");
        path
    }

    pub fn cashier_path() -> PathBuf {
        let path = dirs::home_dir()
            .expect("Cannot find home directory.")
            .as_path()
            .join(".config/darkfi/cashier.db");
        path
    }

    pub async fn new_wallet() -> Result<()> {
        let path = Self::wallet_path();
        let connect = Connection::open(&path).expect("Failed to connect to database.");
        let contents = include_str!("../../res/schema.sql");
        Ok(connect.execute_batch(&contents)?)
    }

    pub async fn new_cashier_wallet() -> Result<()> {
        let path = Self::cashier_path();
        let connect = Connection::open(&path).expect("Failed to connect to database.");
        let contents = include_str!("../../res/schema.sql");
        Ok(connect.execute_batch(&contents)?)
    }

    pub async fn own_key_gen() -> Result<()> {
        let path = Self::wallet_path();
        let connect = Connection::open(&path).expect("Failed to connect to database.");
        let id = 0;
        // Create keys
        let secret: jubjub::Fr = jubjub::Fr::random(&mut OsRng);
        debug!(target: "adapter", "key_gen() [Generating public key...]");
        let public = zcash_primitives::constants::SPENDING_KEY_GENERATOR * secret;
        let pubkey = serial::serialize(&public);
        let privkey = serial::serialize(&secret);
        // Write keys to database
        connect.execute(
            "INSERT INTO keys(key_id, key_private, key_public)
            VALUES (:id, :privkey, :pubkey)",
            named_params! {":id": id,
             ":privkey": privkey,
             ":pubkey": pubkey
            },
        )?;
        Ok(())
    }

    pub async fn cash_key_gen() -> Result<()> {
        let path = Self::cashier_path();
        let connect = Connection::open(&path).expect("Failed to connect to database.");
        let id = 0;
        // Create keys
        let secret: jubjub::Fr = jubjub::Fr::random(&mut OsRng);
        let public = zcash_primitives::constants::SPENDING_KEY_GENERATOR * secret;
        let pubkey = serial::serialize(&public);
        let privkey = serial::serialize(&secret);
        // Write keys to database
        connect.execute(
            "INSERT INTO keys(key_id, key_private, key_public)
            VALUES (:id, :privkey, :pubkey)",
            named_params! {":id": id,
             ":privkey": privkey,
             ":pubkey": pubkey
            },
        )?;
        Ok(())
    }

    pub async fn get_cash_public() -> Result<()> {
        let path = Self::cashier_path();
        let connect = Connection::open(&path).expect("Failed to connect to database.");
        let id = 0;
        let mut stmt = connect.prepare("SELECT key_public FROM keys").unwrap();
        let key_iter = stmt
            .query_map::<Vec<u8>, _, _>([], |row| row.get(0))
            .unwrap();
        let mut pub_keys = Vec::new();
        for key in key_iter {
            pub_keys.push(key.unwrap());
        }
        let key = match pub_keys.pop() {
            Some(key_found) => println!("{:?}", key_found),
            None => println!("No cashier public key found"),
        };
        Ok(key)
    }

    pub async fn save_cash_key(pubkey: Vec<u8>) -> Result<()> {
        let path = Self::wallet_path();
        let connect = Connection::open(&path).expect("Failed to connect to database.");
        let id = 0;
        // Write keys to database
        connect.execute(
            "INSERT INTO cashier(key_id, key_public)
            VALUES (:id, :pubkey)",
            named_params! {":id": id,
             ":pubkey": pubkey
            },
        )?;
        Ok(())
    }

    pub async fn save_key(pubkey: Vec<u8>) -> Result<()> {
        let path = Self::wallet_path();
        let connect = Connection::open(&path).expect("Failed to connect to database.");
        let id = 0;
        // Write keys to database
        connect.execute(
            "INSERT INTO keys(key_id, key_public)
            VALUES (:id, :pubkey)",
            named_params! {":id": id,
             ":pubkey": pubkey
            },
        )?;
        Ok(())
    }
}

fn main() {}
