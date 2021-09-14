use super::WalletApi;
use crate::client::ClientFailed;
use crate::{Error, Result};

use async_std::sync::Arc;
use log::*;
use rusqlite::{named_params, params, Connection};

use std::path::PathBuf;

pub type CashierDbPtr = Arc<CashierDb>;

pub struct CashierDb {
    pub path: PathBuf,
    pub password: String,
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
    pub fn new(path: &PathBuf, password: String) -> Result<CashierDbPtr> {
        debug!(target: "CASHIERDB", "new() Constructor called");
        Ok(Arc::new(Self {
            path: path.to_owned(),
            password,
        }))
    }

    pub fn init_db(&self) -> Result<()> {
        if !self.password.trim().is_empty() {
            let contents = include_str!("../../sql/cashier.sql");
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

    // return private and public keys as a tuple
    pub fn get_deposit_coin_keys_by_dkey_public(
        &self,
        d_key_public: &jubjub::SubgroupPoint,
        asset_id: &Vec<u8>,
    ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        debug!(target: "CASHIERDB", "Check for existing dkey");
        let d_key_public = self.get_value_serialized(d_key_public)?;
        // open connection
        let conn = Connection::open(&self.path)?;
        // unlock database
        conn.pragma_update(None, "key", &self.password)?;

        let mut stmt =
            conn.prepare("SELECT * FROM deposit_keypairs where d_key_public = :d_key_public AND asset_id = :asset_id")?;
        let keys_iter = stmt.query_map::<(Vec<u8>, Vec<u8>), _, _>(
            &[(":d_key_public", &d_key_public), (":asset_id", &asset_id)],
            |row| Ok((row.get(1)?, row.get(2)?)),
        )?;

        let mut keys = vec![];

        for k in keys_iter {
            keys.push(k?);
        }

        Ok(keys)
    }

    // Update to take BitcoinKeys instance instead
    pub fn put_exchange_keys(
        &self,
        d_key_public: &jubjub::SubgroupPoint,
        coin_private: &Vec<u8>,
        coin_public: &Vec<u8>,
        asset_id: &Vec<u8>,
    ) -> Result<()> {
        debug!(target: "CASHIERDB", "Put exchange keys");

        let d_key_public = self.get_value_serialized(d_key_public)?;

        // open connection
        let conn = Connection::open(&self.path)?;
        // unlock database
        conn.pragma_update(None, "key", &self.password)?;

        conn.execute(
            "INSERT INTO deposit_keypairs(d_key_public, coin_key_private, coin_key_public, asset_id)
            VALUES (:d_key_public, :coin_key_private, :coin_key_public, :asset_id)",
            named_params! {
                ":d_key_public": d_key_public,
                ":coin_key_private": coin_private,
                ":coin_key_public": coin_public,
                ":asset_id": asset_id,
            },
        )?;
        Ok(())
    }
    pub fn put_btc_utxo(
        &self,
        tx_id: &Vec<u8>,
        btc_key_public: &Vec<u8>,
        balance: i64,
    ) -> Result<()> {
        debug!(target: "CASHIERDB", "Put BTC Utxo");

        let tx_id = self.get_value_serialized(tx_id)?;

        // open connection
        let conn = Connection::open(&self.path)?;
        // unlock database
        conn.pragma_update(None, "key", &self.password)?;

        conn.execute(
            "INSERT INTO btc_utxo(tx_id, btc_key_public, balance)
            VALUES (:tx_id, :btc_key_public, :balance)",
            named_params! {
                ":tx_id": tx_id,
                ":btc_key_public": btc_key_public,
                ":balance": balance,
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

        let mut stmt = conn.prepare("SELECT d_key_private FROM withdraw_keypairs")?;
        let keys = stmt.query_map([], |row| {
            let private_key: jubjub::Fr = self
                .get_value_deserialized(row.get(0)?)
                .expect("deserialize private key");
            Ok(private_key)
        })?;

        let mut private_keys: Vec<jubjub::Fr> = vec![];

        for k in keys {
            private_keys.push(k?);
        }

        Ok(private_keys)
    }

    // return (public key, private key)
    pub fn get_withdraw_keys_by_coin_public_key(
        &self,
        coin_public_key: &Vec<u8>,
        asset_id: &Vec<u8>,
    ) -> Result<Option<(jubjub::SubgroupPoint, jubjub::Fr)>> {
        debug!(target: "CASHIERDB", "Check for existing coin address");
        // open connection
        let conn = Connection::open(&self.path)?;
        // unlock database
        conn.pragma_update(None, "key", &self.password)?;

        let mut stmt =
            conn.prepare("SELECT * FROM withdraw_keypairs WHERE coin_key_id = :coin_key_id AND asset_id = :asset_id")?;
        let addr_iter = stmt.query_map::<(jubjub::SubgroupPoint, jubjub::Fr), _, _>(
            &[(":coin_key_id", &coin_public_key), (":asset_id", &asset_id)],
            |row| {
                let public: jubjub::SubgroupPoint = self
                    .get_value_deserialized(row.get(2)?)
                    .expect("get public key deserialize");
                let private: jubjub::Fr = self
                    .get_value_deserialized(row.get(1)?)
                    .expect("get  private key deserialize");
                Ok((public, private))
            },
        )?;

        let mut addresses: Vec<(jubjub::SubgroupPoint, jubjub::Fr)> = vec![];

        for addr in addr_iter {
            addresses.push(addr?);
        }

        Ok(addresses.pop())
    }

    pub fn get_withdraw_coin_public_key_by_dkey_public(
        &self,
        pub_key: &jubjub::SubgroupPoint,
        asset_id: &Vec<u8>,
    ) -> Result<Option<Vec<u8>>> {
        debug!(target: "CASHIERDB", "Get coin address by pub_key");
        // open connection
        let conn = Connection::open(&self.path)?;
        // unlock database
        conn.pragma_update(None, "key", &self.password)?;

        let d_key_public = self.get_value_serialized(pub_key)?;

        let mut stmt = conn.prepare(
            "SELECT coin_key_id FROM withdraw_keypairs WHERE d_key_public = :d_key_public AND asset_id = :asset_id",
        )?;
        let addr_iter = stmt.query_map::<Vec<u8>, _, _>(
            &[(":d_key_public", &d_key_public), (":asset_id", &asset_id)],
            |row| Ok(row.get(0)?),
        )?;

        let mut coin_addresses = vec![];

        for addr in addr_iter {
            coin_addresses.push(addr?);
        }

        Ok(coin_addresses.pop())
    }

    pub fn delete_withdraw_key_record(
        &self,
        coin_address: &Vec<u8>,
        asset_id: &Vec<u8>,
    ) -> Result<()> {
        debug!(target: "CASHIERDB", "Delete withdraw keys");

        // open connection
        let conn = Connection::open(&self.path)?;
        // unlock database
        conn.pragma_update(None, "key", &self.password)?;

        conn.execute(
            "DELETE FROM withdraw_keypairs WHERE coin_key_id = ?1 AND asset_id = ?2;",
            params![coin_address, asset_id],
        )?;

        Ok(())
    }

    pub fn put_withdraw_keys(
        &self,
        coin_key_id: &Vec<u8>,
        d_key_public: &jubjub::SubgroupPoint,
        d_key_private: &jubjub::Fr,
        asset_id: &Vec<u8>,
    ) -> Result<()> {
        debug!(target: "CASHIERDB", "Put withdraw keys");

        let d_key_public = self.get_value_serialized(d_key_public)?;
        let d_key_private = self.get_value_serialized(d_key_private)?;

        // open connection
        let conn = Connection::open(&self.path)?;
        // unlock database
        conn.pragma_update(None, "key", &self.password)?;

        conn.execute(
            "INSERT INTO withdraw_keypairs(coin_key_id, d_key_private, d_key_public, asset_id)
            VALUES (:coin_key_id, :d_key_private, :d_key_public, :asset_id)",
            named_params! {
                ":coin_key_id": coin_key_id,
                ":d_key_private": d_key_private,
                ":d_key_public": d_key_public,
                ":asset_id": asset_id,
            },
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

    #[test]
    pub fn test_put_withdraw_keys_and_load_them_with_coin_key() -> Result<()> {
        let walletdb_path = join_config_path(&PathBuf::from("cashier_wallet_test.db"))?;
        let wallet = CashierDb::new(&walletdb_path, "darkfi".into())?;
        wallet.init_db()?;

        let secret2: jubjub::Fr = jubjub::Fr::random(&mut OsRng);
        let public2 = zcash_primitives::constants::SPENDING_KEY_GENERATOR * secret2;

        // btc addr testnet
        let coin_addr = serialize(&String::from("mxVFsFW5N4mu1HPkxPttorvocvzeZ7KZyk"));

        let asset_id = serialize(&1);

        wallet.put_withdraw_keys(&coin_addr, &public2, &secret2, &asset_id)?;

        let addr = wallet.get_withdraw_keys_by_coin_public_key(&coin_addr, &asset_id)?;

        assert_eq!(addr, Some((public2, secret2)));

        wallet.delete_withdraw_key_record(&coin_addr, &asset_id)?;

        let addr = wallet.get_withdraw_keys_by_coin_public_key(&coin_addr, &asset_id)?;

        assert_eq!(addr, None);

        wallet.destroy()?;

        Ok(())
    }
}
