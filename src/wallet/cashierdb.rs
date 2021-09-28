use super::{Keypair, WalletApi};
use crate::client::ClientFailed;
use crate::{Error, Result};

use async_std::sync::{Arc, Mutex};
use log::*;
use rusqlite::{named_params, params, Connection};

use std::path::{Path, PathBuf};

pub type CashierDbPtr = Arc<CashierDb>;

pub struct CashierDb {
    pub path: PathBuf,
    pub password: String,
    pub initialized: Mutex<bool>,
}

impl WalletApi for CashierDb {
    fn get_password(&self) -> String {
        self.password.to_owned()
    }
    fn get_path(&self) -> PathBuf {
        self.path.to_owned()
    }
}

impl CashierDb {
    pub fn new(path: &Path, password: String) -> Result<CashierDbPtr> {
        debug!(target: "CASHIERDB", "new() Constructor called");
        Ok(Arc::new(Self {
            path: path.to_owned(),
            password,
            initialized: Mutex::new(false),
        }))
    }

    pub async fn init_db(&self) -> Result<()> {
        if *self.initialized.lock().await == false {
            if !self.password.trim().is_empty() {
                let contents = include_str!("../../sql/cashier.sql");
                let conn = Connection::open(&self.path)?;
                debug!(target: "CASHIERDB", "Opened connection at path {:?}", self.path);
                conn.pragma_update(None, "key", &self.password)?;
                conn.execute_batch(&contents)?;
                *self.initialized.lock().await = true;
            } else {
                debug!(
                    target: "CASHIERDB",
                    "Password is empty. You must set a password to use the wallet."
                );
                return Err(Error::from(ClientFailed::EmptyPassword));
            }
        } else {
            debug!(target: "WALLETDB", "Wallet already initialized.");
            return Err(Error::from(ClientFailed::WalletInitialized));
        }
        Ok(())
    }

    pub fn put_main_keys(
        &self,
        token_key_private: &Vec<u8>,
        token_key_public: &Vec<u8>,
        network: &String,
        token_id: &jubjub::Fr,
    ) -> Result<()> {
        debug!(target: "CASHIERDB", "Put main keys");

        // open connection
        let conn = Connection::open(&self.path)?;
        // unlock database
        conn.pragma_update(None, "key", &self.password)?;

        let network = self.get_value_serialized(network)?;
        let token_id = self.get_value_serialized(token_id)?;

        conn.execute(
            "INSERT INTO main_keypairs
            (token_key_private, token_key_public, network, token_id)
            VALUES
            (:token_key_private, :token_key_public, :network, :token_id)",
            named_params! {
                ":token_key_private": token_key_private,
                ":token_key_public": token_key_public,
                ":network": &network,
                ":token_id": &token_id,
            },
        )?;
        Ok(())
    }

    pub fn get_main_keys(
        &self,
        network: &String,
        token_id: &jubjub::Fr,
    ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        debug!(target: "CASHIERDB", "Get main keys");
        // open connection
        let conn = Connection::open(&self.path)?;
        // unlock database
        conn.pragma_update(None, "key", &self.password)?;

        let network = self.get_value_serialized(network)?;
        let token_id = self.get_value_serialized(token_id)?;

        let mut stmt = conn.prepare(
            "SELECT token_key_private, token_key_public
            FROM main_keypairs
            WHERE network = :network
            AND token_id = :token_id ;",
        )?;
        let keys_iter = stmt.query_map::<(Vec<u8>, Vec<u8>), _, _>(
            &[(":network", &network), (":token_id", &token_id)],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;

        let mut keys = vec![];

        for k in keys_iter {
            keys.push(k?);
        }

        Ok(keys)
    }

    pub fn put_withdraw_keys(
        &self,
        token_key_public: &Vec<u8>,
        d_key_public: &jubjub::SubgroupPoint,
        d_key_private: &jubjub::Fr,
        network: &String,
        token_id: &jubjub::Fr,
    ) -> Result<()> {
        debug!(target: "CASHIERDB", "Put withdraw keys");

        let d_key_public = self.get_value_serialized(d_key_public)?;
        let d_key_private = self.get_value_serialized(d_key_private)?;
        let network = self.get_value_serialized(network)?;
        let token_id = self.get_value_serialized(token_id)?;
        let confirm = self.get_value_serialized(&false)?;

        // open connection
        let conn = Connection::open(&self.path)?;
        // unlock database
        conn.pragma_update(None, "key", &self.password)?;

        conn.execute(
            "INSERT INTO withdraw_keypairs
            (token_key_public, d_key_private, d_key_public, network,  token_id, confirm)
            VALUES
            (:token_key_public, :d_key_private, :d_key_public,:network, :token_id, :confirm);",
            named_params! {
                ":token_key_public": token_key_public,
                ":d_key_private": d_key_private,
                ":d_key_public": d_key_public,
                ":network": network,
                ":token_id": token_id,
                ":confirm": confirm,
            },
        )?;
        Ok(())
    }

    pub fn put_deposit_keys(
        &self,
        d_key_public: &jubjub::SubgroupPoint,
        token_key_private: &Vec<u8>,
        token_key_public: &Vec<u8>,
        network: &String,
        token_id: &jubjub::Fr,
    ) -> Result<()> {
        debug!(target: "CASHIERDB", "Put exchange keys");

        // open connection
        let conn = Connection::open(&self.path)?;
        // unlock database
        conn.pragma_update(None, "key", &self.password)?;

        let d_key_public = self.get_value_serialized(d_key_public)?;
        let token_id = self.get_value_serialized(token_id)?;
        let network = self.get_value_serialized(network)?;
        let confirm = self.get_value_serialized(&false)?;

        conn.execute(
            "INSERT INTO deposit_keypairs
            (d_key_public, token_key_private, token_key_public, network, token_id, confirm)
            VALUES
            (:d_key_public, :token_key_private, :token_key_public, :network, :token_id, :confirm)",
            named_params! {
                ":d_key_public": &d_key_public,
                ":token_key_private": token_key_private,
                ":token_key_public": token_key_public,
                ":network": &network,
                ":token_id": &token_id,
                ":confirm": &confirm,
            },
        )?;
        Ok(())
    }

    pub fn get_withdraw_private_keys(&self) -> Result<Vec<jubjub::Fr>> {
        debug!(target: "CASHIERDB", "Get withdraw private keys");
        // open connection
        let conn = Connection::open(&self.path)?;
        // unlock database
        conn.pragma_update(None, "key", &self.password)?;

        let confirm = self.get_value_serialized(&false)?;

        let mut stmt = conn.prepare(
            "SELECT d_key_private
                FROM withdraw_keypairs
                WHERE confirm = :confirm",
        )?;

        let keys = stmt.query_map(&[(":confirm", &confirm)], |row| Ok(row.get(0)?))?;

        let mut private_keys: Vec<jubjub::Fr> = vec![];

        for k in keys {
            let private_key: jubjub::Fr = self.get_value_deserialized(k?)?;
            private_keys.push(private_key);
        }

        Ok(private_keys)
    }

    // return token public key, network name, and token_id as tuple
    pub fn get_withdraw_token_public_key_by_dkey_public(
        &self,
        pub_key: &jubjub::SubgroupPoint,
    ) -> Result<Option<(Vec<u8>, String, jubjub::Fr)>> {
        debug!(target: "CASHIERDB", "Get token address by pub_key");
        // open connection
        let conn = Connection::open(&self.path)?;
        // unlock database
        conn.pragma_update(None, "key", &self.password)?;

        let d_key_public = self.get_value_serialized(pub_key)?;

        let confirm = self.get_value_serialized(&false)?;

        let mut stmt = conn.prepare(
            "SELECT token_key_public, network, token_id
            FROM withdraw_keypairs
            WHERE d_key_public = :d_key_public AND confirm = :confirm;",
        )?;
        let addr_iter = stmt.query_map(
            &[(":d_key_public", &d_key_public), (":confirm", &&confirm)],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;

        let mut token_addresses = vec![];

        for addr in addr_iter {
            let addr = addr?;
            let token_key_public = addr.0;
            let network: String = self.get_value_deserialized(addr.1)?;
            let token_id: jubjub::Fr = self.get_value_deserialized(addr.2)?;
            token_addresses.push((token_key_public, network, token_id));
        }

        Ok(token_addresses.pop())
    }

    // return private and public keys as a tuple
    pub fn get_deposit_token_keys_by_dkey_public(
        &self,
        d_key_public: &jubjub::SubgroupPoint,
        network: &String,
    ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        debug!(target: "CASHIERDB", "Check for existing dkey");
        let d_key_public = self.get_value_serialized(d_key_public)?;
        // open connection
        let conn = Connection::open(&self.path)?;
        // unlock database
        conn.pragma_update(None, "key", &self.password)?;

        let network = self.get_value_serialized(network)?;
        let confirm = self.get_value_serialized(&false)?;

        let mut stmt = conn.prepare(
            "SELECT token_key_private, token_key_public
            FROM deposit_keypairs
            WHERE d_key_public = :d_key_public
            AND network = :network
            AND confirm = :confirm ;",
        )?;
        let keys_iter = stmt.query_map::<(Vec<u8>, Vec<u8>), _, _>(
            &[
                (":d_key_public", &d_key_public),
                (":network", &network),
                (":confirm", &confirm),
            ],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;

        let mut keys = vec![];

        for k in keys_iter {
            keys.push(k?);
        }

        Ok(keys)
    }

    // return private and public keys as a tuple
    pub fn get_deposit_token_keys_by_network(
        &self,
        network: &String,
    ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        debug!(target: "CASHIERDB", "Check for existing dkey");
        // open connection
        let conn = Connection::open(&self.path)?;
        // unlock database
        conn.pragma_update(None, "key", &self.password)?;

        let network = self.get_value_serialized(network)?;
        let confirm = self.get_value_serialized(&false)?;

        let mut stmt = conn.prepare(
            "SELECT token_key_private, token_key_public
            FROM deposit_keypairs
            WHERE network = :network
            AND confirm = :confirm ;",
        )?;
        let keys_iter = stmt.query_map::<(Vec<u8>, Vec<u8>), _, _>(
            &[(":network", &network), (":confirm", &confirm)],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;

        let mut keys = vec![];

        for k in keys_iter {
            keys.push(k?);
        }

        Ok(keys)
    }

    pub fn get_withdraw_keys_by_token_public_key(
        &self,
        token_key_public: &Vec<u8>,
        network: &String,
    ) -> Result<Option<Keypair>> {
        debug!(target: "CASHIERDB", "Check for existing token address");
        // open connection
        let conn = Connection::open(&self.path)?;
        // unlock database
        conn.pragma_update(None, "key", &self.password)?;

        let confirm = self.get_value_serialized(&false)?;

        let network = self.get_value_serialized(network)?;

        let mut stmt = conn.prepare(
            "SELECT d_key_private, d_key_public FROM withdraw_keypairs
                WHERE token_key_public = :token_key_public
                AND network = :network
                AND confirm = :confirm;",
        )?;

        let keypair_iter = stmt.query_map(
            &[
                (":token_key_public", &token_key_public),
                (":network", &&network),
                (":confirm", &&confirm),
            ],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;

        let mut keypairs: Vec<Keypair> = vec![];

        for kp in keypair_iter {
            let kp = kp?;
            let public: jubjub::SubgroupPoint = self.get_value_deserialized(kp.1)?;
            let private: jubjub::Fr = self.get_value_deserialized(kp.0)?;
            let keypair = Keypair { public, private };
            keypairs.push(keypair);
        }

        Ok(keypairs.pop())
    }

    pub fn confirm_withdraw_key_record(
        &self,
        token_address: &Vec<u8>,
        network: &String,
    ) -> Result<()> {
        debug!(target: "CASHIERDB", "Confirm withdraw keys");

        // open connection
        let conn = Connection::open(&self.path)?;
        // unlock database
        conn.pragma_update(None, "key", &self.password)?;

        let network = self.get_value_serialized(network)?;
        let confirm = self.get_value_serialized(&true)?;

        conn.execute(
            "UPDATE withdraw_keypairs
            SET confirm = ?1
            WHERE token_key_public = ?2
            AND network = ?3;",
            params![confirm, token_address, network],
        )?;

        Ok(())
    }

    pub fn confirm_deposit_key_record(
        &self,
        d_key_public: &jubjub::SubgroupPoint,
        network: &String,
    ) -> Result<()> {
        debug!(target: "CASHIERDB", "Confirm withdraw keys");

        // open connection
        let conn = Connection::open(&self.path)?;
        // unlock database
        conn.pragma_update(None, "key", &self.password)?;

        let network = self.get_value_serialized(network)?;
        let confirm = self.get_value_serialized(&true)?;
        let d_key_public = self.get_value_serialized(d_key_public)?;

        conn.execute(
            "UPDATE deposit_keypairs
            SET confirm = ?1
            WHERE d_key_public = ?2
            AND network = ?3;",
            params![confirm, d_key_public, network],
        )?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::serial::serialize;
    use crate::util::join_config_path;

    use ff::Field;
    use rand::rngs::OsRng;

    // TODO add more tests

    pub fn init_db(path: &PathBuf, password: String) -> Result<()> {
        if !password.trim().is_empty() {
            let contents = include_str!("../../sql/cashier.sql");
            let conn = Connection::open(&path)?;
            debug!(target: "CASHIERDB", "OPENED CONNECTION AT PATH {:?}", path);
            conn.pragma_update(None, "key", &password)?;
            conn.execute_batch(&contents)?;
        } else {
            debug!(target: "CASHIERDB", "Password is empty. You must set a password to use the wallet.");
            return Err(Error::from(ClientFailed::EmptyPassword));
        }
        Ok(())
    }

    #[test]
    pub fn test_put_main_keys_and_load_them_with_network_name() -> Result<()> {
        let walletdb_path = join_config_path(&PathBuf::from("cashier_wallet_test2.db"))?;
        let password: String = "darkfi".into();
        let wallet = CashierDb::new(&walletdb_path, password.clone())?;
        init_db(&walletdb_path, password)?;

        // btc addr testnet
        let token_addr = serialize(&String::from("mxVFsFW5N4mu1HPkxPttorvocvzeZ7KZyk"));
        let token_addr_private = serialize(&String::from("2222222222222222222222222222222222"));

        let network = String::from("btc");
        let token_id: jubjub::Fr = jubjub::Fr::random(&mut OsRng);

        wallet.put_main_keys(&token_addr_private, &token_addr, &network, &token_id)?;

        let keys = wallet.get_main_keys(&network, &token_id)?;

        assert_eq!(keys.len(), 1);

        assert_eq!(keys[0].0, token_addr_private);
        assert_eq!(keys[0].1, token_addr);

        std::fs::remove_file(walletdb_path)?;

        Ok(())
    }

    #[test]
    pub fn test_put_deposit_keys_and_load_them_with_() -> Result<()> {
        let walletdb_path = join_config_path(&PathBuf::from("cashier_wallet_test3.db"))?;
        let password: String = "darkfi".into();
        let wallet = CashierDb::new(&walletdb_path, password.clone())?;
        init_db(&walletdb_path, password)?;

        // btc addr testnet
        let token_addr = serialize(&String::from("mxVFsFW5N4mu1HPkxPttorvocvzeZ7KZyk"));
        let token_addr_private = serialize(&String::from("2222222222222222222222222222222222"));

        let network = String::from("btc");

        let secret2: jubjub::Fr = jubjub::Fr::random(&mut OsRng);
        let public2 = zcash_primitives::constants::SPENDING_KEY_GENERATOR * secret2;
        let token_id: jubjub::Fr = jubjub::Fr::random(&mut OsRng);

        wallet.put_deposit_keys(
            &public2,
            &token_addr_private,
            &token_addr,
            &network,
            &token_id,
        )?;

        let keys = wallet.get_deposit_token_keys_by_dkey_public(&public2, &network)?;

        assert_eq!(keys.len(), 1);

        assert_eq!(keys[0].0, token_addr_private);
        assert_eq!(keys[0].1, token_addr);

        wallet.confirm_deposit_key_record(&public2, &network)?;

        let keys = wallet.get_deposit_token_keys_by_dkey_public(&public2, &network)?;

        assert_eq!(keys.len(), 0);

        std::fs::remove_file(walletdb_path)?;

        Ok(())
    }

    #[test]
    pub fn test_put_withdraw_keys_and_load_them_with_token_key() -> Result<()> {
        let walletdb_path = join_config_path(&PathBuf::from("cashier_wallet_test.db"))?;
        let password: String = "darkfi".into();
        let wallet = CashierDb::new(&walletdb_path, password.clone())?;
        init_db(&walletdb_path, password)?;

        let secret2: jubjub::Fr = jubjub::Fr::random(&mut OsRng);
        let public2 = zcash_primitives::constants::SPENDING_KEY_GENERATOR * secret2;
        let token_id: jubjub::Fr = jubjub::Fr::random(&mut OsRng);

        // btc addr testnet
        let token_addr = serialize(&String::from("mxVFsFW5N4mu1HPkxPttorvocvzeZ7KZyk"));

        let network = String::from("btc");

        wallet.put_withdraw_keys(&token_addr, &public2, &secret2, &network, &token_id)?;

        let addr = wallet.get_withdraw_keys_by_token_public_key(&token_addr, &network)?;

        assert_eq!(addr.is_some(), true);

        wallet.confirm_withdraw_key_record(&token_addr, &network)?;

        let addr = wallet.get_withdraw_keys_by_token_public_key(&token_addr, &network)?;

        assert_eq!(addr.is_none(), true);

        std::fs::remove_file(walletdb_path)?;

        Ok(())
    }
}
