use crate::serial;
use crate::Result;
use ff::Field;
use log::*;
use rand::rngs::OsRng;
use rusqlite::{named_params, Connection};
use std::path::PathBuf;

// TODO: make this more generic to remove boiler plate. e.g. create_wallet(cashier) instead of
// create_cashier_wallet
pub struct WalletDB {}

impl WalletDB {
    pub async fn new(path: PathBuf) -> Result<()> {
        let connect = Connection::open(&path).expect("Failed to connect to database.");
        let contents = include_str!("../../res/schema.sql");
        Ok(connect.execute_batch(&contents)?)
    }

    //    pub async fn create_keypair() -> Result<String, String> {
    //        let public = zcash_primitives::constants::SPENDING_KEY_GENERATOR * secret;
    //        let pubkey = serial::serialize(&public);
    //        let secret: jubjub::Fr = jubjub::Fr::random(&mut OsRng);
    //        let privkey = serial::serialize(&secret);
    //        Ok(pubkey, privkey)
    //    }
    pub async fn path() -> Result<PathBuf> {
        let path = dirs::home_dir()
            .expect("cannot find home directory.")
            .as_path()
            .join(".config/darkfi/");
        debug!(target: "walletdb", "CREATE PATH {:?}", path);
        Ok(path)
    }

    pub async fn key_gen(path: PathBuf, id: i32, pubkey: Vec<u8>, privkey: Vec<u8>) -> Result<()> {
        debug!(target: "key_gen", "Generating keys...");
        let connect = Connection::open(&path).expect("Failed to connect to database.");
        // TODO: ID should not be fixed
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

    pub async fn get(path: PathBuf) -> Result<()> {
        debug!(target: "get_cash_public", "Returning cashier keys...");
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

    pub async fn save(path: PathBuf, pubkey: Vec<u8>) -> Result<()> {
        debug!(target: "save_cash_key", "Save cashier keys...");
        //let path = Self::wallet_path();
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
}

fn main() {}
