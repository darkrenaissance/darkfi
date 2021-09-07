use super::WalletApi;
use crate::client::ClientFailed;
use crate::serial;
use crate::serial::{deserialize, serialize, Decodable, Encodable};
use crate::service::btc::{PrivKey, PubKey};
use crate::{Error, Result};

use async_std::sync::Arc;
use ff::Field;
use log::*;
use rand::rngs::OsRng;
use rusqlite::{named_params, params, Connection};

use std::path::PathBuf;

pub type CashierDbPtr = Arc<CashierDb>;

pub struct CashierDb {
    pub path: PathBuf,
    pub password: String,
}

impl WalletApi for CashierDb {
    fn init_db(&self) -> Result<()> {
        if !self.password.trim().is_empty() {
            let contents = include_str!("../../res/cashier.sql");
            let conn = Connection::open(&self.path)?;
            debug!(target: "CASHIERDB", "Opened connection at path {:?}", self.path);
            conn.pragma_update(None, "key", &self.password)?;
            conn.execute_batch(&contents)?;
        } else {
            debug!(target: "CASHIERDB", "Password is empty. You must set a password to use the wallet.");
            return Err(Error::from(ClientFailed::EmptyPassword));
        }
        Ok(())
    }

    fn key_gen(&self) -> Result<(Vec<u8>, Vec<u8>)> {
        debug!(target: "CASHIERDB", "Generating cashier keys...");
        let secret: jubjub::Fr = jubjub::Fr::random(&mut OsRng);
        let public = zcash_primitives::constants::SPENDING_KEY_GENERATOR * secret;
        let pubkey = serial::serialize(&public);
        let privkey = serial::serialize(&secret);
        Ok((pubkey, privkey))
    }

    fn put_keypair(&self, key_public: Vec<u8>, key_private: Vec<u8>) -> Result<()> {
        let conn = Connection::open(&self.path)?;
        conn.pragma_update(None, "key", &self.password)?;
        conn.execute(
            "INSERT INTO keys(key_public, key_private) VALUES (?1, ?2)",
            params![key_public, key_private],
        )?;
        Ok(())
    }

    fn get_public_keys(&self) -> Result<Vec<jubjub::SubgroupPoint>> {
        debug!(target: "CASHIERDB", "Returning keys...");
        let conn = Connection::open(&self.path)?;
        conn.pragma_update(None, "key", &self.password)?;
        let mut stmt = conn.prepare("SELECT key_public FROM keys")?;
        let key_iter = stmt.query_map::<Vec<u8>, _, _>([], |row| row.get(0))?;
        let mut pub_keys = Vec::new();
        for key in key_iter {
            let public: jubjub::SubgroupPoint =
                self.get_value_deserialized::<jubjub::SubgroupPoint>(key?)?;
            pub_keys.push(public);
        }
        Ok(pub_keys)
    }

    fn get_private_keys(&self) -> Result<Vec<jubjub::Fr>> {
        debug!(target: "CASHIERDB", "Returning keys...");
        let conn = Connection::open(&self.path)?;
        conn.pragma_update(None, "key", &self.password)?;
        let mut stmt = conn.prepare("SELECT key_private FROM keys")?;
        let key_iter = stmt.query_map::<Vec<u8>, _, _>([], |row| row.get(0))?;
        let mut keys = Vec::new();
        for key in key_iter {
            let private: jubjub::Fr = self.get_value_deserialized(key?)?;
            keys.push(private);
        }
        Ok(keys)
    }
}

impl CashierDb {
    pub fn new(path: &PathBuf, password: String) -> Result<CashierDbPtr> {
        debug!(target: "CASHIERDB", "new() Constructor called");
        Ok(Arc::new(Self {
            path: path.to_owned(),
            password,
        }))
    }

    pub fn get_keys_by_dkey(&self, dkey_pub: &Vec<u8>) -> Result<()> {
        debug!(target: "CASHIERDB", "Check for existing dkey");
        //let dkey_id = self.get_value_deserialized(dkey_pub)?;
        // open connection
        let conn = Connection::open(&self.path)?;
        // unlock database
        conn.pragma_update(None, "key", &self.password)?;

        // let mut keypairs = conn.prepare("SELECT dkey_id FROM keypairs WHERE dkey_id = :dkey_id")?;
        // let rows = keypairs.query_map::<Vec<u8>, _, _>(&[(":dkey_id", &secret)], |row| row.get(0))?;

        let mut stmt = conn.prepare("SELECT * FROM keypairs where dkey_id = ?")?;
        let mut rows = stmt.query([dkey_pub])?;
        if let Some(_row) = rows.next()? {
            println!("Got something");
        } else {
            println!("Did not get something");
        }

        Ok(())
    }

    // Update to take BitcoinKeys instance instead
    pub fn put_exchange_keys(
        &self,
        dkey_pub: Vec<u8>,
        btc_private: PrivKey,
        btc_public: PubKey,
        //txid will be updated when exists
    ) -> Result<()> {
        debug!(target: "CASHIERDB", "Put exchange keys");
        // prepare the values
        //let dkey_pub = self.get_value_serialized(&dkey_pub)?;
        let btc_private = btc_private.to_bytes();
        let btc_public = btc_public.to_bytes();

        // open connection
        let conn = Connection::open(&self.path)?;
        // unlock database
        conn.pragma_update(None, "key", &self.password)?;

        conn.execute(
            "INSERT INTO keypairs(dkey_id, btc_key_private, btc_key_public)
            VALUES (:dkey_id, :btc_key_private, :btc_key_public)",
            named_params! {
                ":dkey_id": dkey_pub,
                ":btc_key_private": btc_private,
                ":btc_key_private": btc_public,
            },
        )?;
        Ok(())
    }

    // return (public key, private key)
    pub fn get_address_by_btc_key(
        &self,
        btc_address: &Vec<u8>,
    ) -> Result<Option<(Vec<u8>, Vec<u8>)>> {
        debug!(target: "CASHIERDB", "Check for existing btc address");
        // open connection
        let conn = Connection::open(&self.path)?;
        // unlock database
        conn.pragma_update(None, "key", &self.password)?;

        let mut stmt =
            conn.prepare("SELECT * FROM withdraw_keypairs where btc_key_id = :btc_key_id")?;
        let addr_iter = stmt
            .query_map::<(Vec<u8>, Vec<u8>), _, _>(&[(":btc_key_id", btc_address)], |row| {
                Ok((row.get(2)?, row.get(1)?))
            })?;

        let mut btc_addresses = vec![];

        for addr in addr_iter {
            btc_addresses.push(addr);
        }

        if let Some(addr) = btc_addresses.pop() {
            return Ok(Some(addr?));
        }

        return Ok(None);
    }

    pub fn put_withdraw_keys(
        &self,
        btc_key_id: Vec<u8>,
        d_key_public: Vec<u8>,
        d_key_private: Vec<u8>,
    ) -> Result<()> {
        debug!(target: "CASHIERDB", "Put withdraw keys");

        // open connection
        let conn = Connection::open(&self.path)?;
        // unlock database
        conn.pragma_update(None, "key", &self.password)?;

        conn.execute(
            "INSERT INTO withdraw_keypairs(btc_key_id, d_key_private, d_key_public)
            VALUES (:btc_key_id, :d_key_private, :d_key_public)",
            named_params! {
                ":btc_key_id": btc_key_id,
                ":d_key_private": d_key_private,
                ":d_key_public": d_key_public,
            },
        )?;
        Ok(())
    }

    pub fn test_wallet(&self) -> Result<()> {
        let conn = Connection::open(&self.path)?;
        conn.pragma_update(None, "key", &self.password)?;
        let mut stmt = conn.prepare("SELECT * FROM keys")?;
        let _rows = stmt.query([])?;
        Ok(())
    }

    pub fn get_value_serialized<T: Encodable>(&self, data: &T) -> Result<Vec<u8>> {
        let v = serialize(data);
        Ok(v)
    }

    pub fn get_value_deserialized<D: Decodable>(&self, key: Vec<u8>) -> Result<D> {
        let v: D = deserialize(&key)?;
        Ok(v)
    }

    fn get_tables_name(&self) -> Result<Vec<String>> {
        let conn = Connection::open(&self.path)?;
        conn.pragma_update(None, "key", &self.password)?;
        let mut stmt = conn.prepare("SELECT name FROM sqlite_master WHERE type='table'")?;
        let table_iter = stmt.query_map::<String, _, _>([], |row| row.get(0))?;

        let mut tables = Vec::new();

        for table in table_iter {
            tables.push(table?);
        }

        Ok(tables)
    }

    pub fn destroy(&self) -> Result<()> {
        let conn = Connection::open(&self.path)?;
        conn.pragma_update(None, "key", &self.password)?;

        for table in self.get_tables_name()?.iter() {
            let drop_stmt = format!("DROP TABLE IF EXISTS {}", table);
            let drop_stmt = drop_stmt.as_str();
            conn.execute(drop_stmt, [])?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::util::join_config_path;

    #[test]
    pub fn test_save_and_load_keypair() -> Result<()> {
        let walletdb_path = join_config_path(&PathBuf::from("cashier_wallet_test.db"))?;
        let wallet = CashierDb::new(&walletdb_path, "darkfi".into())?;
        wallet.init_db()?;

        let secret: jubjub::Fr = jubjub::Fr::random(&mut OsRng);
        let public = zcash_primitives::constants::SPENDING_KEY_GENERATOR * secret;
        let key_public = serial::serialize(&public);
        let key_private = serial::serialize(&secret);

        wallet.put_keypair(key_public, key_private)?;

        let public2 = wallet.get_public_keys()?[0];
        let secret2 = wallet.get_private_keys()?[0];

        assert_eq!(public, public2);
        assert_eq!(secret, secret2);

        wallet.destroy()?;

        Ok(())
    }

    #[test]
    pub fn test_put_withdraw_keys_and_load_them_with_btc_key() -> Result<()> {
        let walletdb_path = join_config_path(&PathBuf::from("cashier_wallet_test2.db"))?;
        let wallet = CashierDb::new(&walletdb_path, "darkfi".into())?;
        wallet.init_db()?;

        let (public, secret) = wallet.key_gen()?;
        wallet.put_keypair(public, secret)?;

        let secret2: jubjub::Fr = jubjub::Fr::random(&mut OsRng);
        let public2 = zcash_primitives::constants::SPENDING_KEY_GENERATOR * secret2;
        let key_public2 = serial::serialize(&public2);
        let key_private2 = serial::serialize(&secret2);

        let btc_addr = serialize(&String::from("bc10000000000000000000000000000000000000000"));

        wallet.put_withdraw_keys(btc_addr.clone(), key_public2.clone(), key_private2.clone())?;

        let addr = wallet.get_address_by_btc_key(&btc_addr)?;

        assert_eq!(addr, Some((key_public2, key_private2)));

        wallet.destroy()?;

        Ok(())
    }
}
