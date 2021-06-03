use crate::Result;
use crate::serial;
use ff::Field;
use log::*;
use rand::rngs::OsRng;
use rusqlite::{Connection, named_params};
use std::path::PathBuf;

// TODO: make this more generic to remove boiler plate. e.g. create_wallet(cashier) instead of
// create_cashier_wallet
pub struct DBInterface {}

impl DBInterface {
    pub fn wallet_path() -> PathBuf {
        debug!(target: "wallet_path", "Finding wallet path...");
        let path = dirs::home_dir()
            .expect("cannot find home directory.")
            .as_path()
            .join(".config/darkfi/wallet.db");
        path
    }

    pub fn cashier_path() -> PathBuf {
        debug!(target: "cashier_path", "Finding cashier path...");
        let path = dirs::home_dir()
            .expect("Cannot find home directory.")
            .as_path()
            .join(".config/darkfi/cashier.db");
        path
    }

    pub async fn new_wallet() -> Result<()> {
        debug!(target: "new_wallet", "Creating new wallet...");
        let path = Self::wallet_path();
        debug!(target: "new_wallet", "Found path {:?}", path);
        let connect = Connection::open(&path).expect("Failed to connect to database.");
        debug!(target: "new_wallet", "Connection established");
        debug!(target: "new_wallet", "Attempting to load schema...");
        let contents = include_str!("../../res/schema.sql");
        debug!(target: "new_wallet", "Schema loaded");
        debug!(target: "new_wallet", "Executing schema");
        Ok(connect.execute_batch(&contents)?)
    }

    pub async fn new_cashier_wallet() -> Result<()> {
        debug!(target: "new_cashier_wallet", "Creating new cashier wallet...");
        let path = Self::cashier_path();
        let connect = Connection::open(&path).expect("Failed to connect to database.");
        let contents = include_str!("../../res/schema.sql");
        Ok(connect.execute_batch(&contents)?)
    }

    pub async fn own_key_gen() -> Result<()> {
        debug!(target: "own_key_gen", "Generating keys...");
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
        debug!(target: "own_key_gen", "Generating cashier keys...");
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
        debug!(target: "get_cash_public", "Returning cashier keys...");
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
        debug!(target: "save_cash_key", "Save cashier keys...");
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
        debug!(target: "save_key", "Save keys...");
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
