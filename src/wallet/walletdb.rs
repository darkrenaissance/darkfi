use crate::serial;
use crate::Result;
use ff::Field;
use log::*;
use rand::rngs::OsRng;
use rusqlite::{named_params, Connection};
use std::path::PathBuf;

// TODO: make this more generic to remove boiler plate. e.g. create_wallet(cashier) instead of
// create_cashier_wallet
pub struct WalletDB {
}

impl WalletDB {
    pub async fn new(path: PathBuf) -> Result<()> {
        let connect = Connection::open(&path).expect("Failed to connect to database.");
        let contents = include_str!("../../res/schema.sql");
        Ok(connect.execute_batch(&contents)?)
    }

    pub fn path(wallet: &str) -> Result<PathBuf> {
        let mut path = dirs::home_dir()
            .expect("cannot find home directory.")
            .as_path()
            .join(".config/darkfi/");
        // add wallet specifier
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

    pub async fn get(path: PathBuf) -> Result<()> {
        debug!(target: "get_cash_public", "Returning cashier keys...");
        let connect = Connection::open(&path).expect("Failed to connect to database.");
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
