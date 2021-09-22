use super::{Keypair, WalletApi};
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
    pub fn get_deposit_token_keys_by_dkey_public(
        &self,
        d_key_public: &jubjub::SubgroupPoint,
        asset_id: &jubjub::Fr,
    ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        debug!(target: "CASHIERDB", "Check for existing dkey");
        let d_key_public = self.get_value_serialized(d_key_public)?;
        // open connection
        let conn = Connection::open(&self.path)?;
        // unlock database
        conn.pragma_update(None, "key", &self.password)?;

        let asset_id = self.get_value_serialized(asset_id)?;

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

    pub fn put_exchange_keys(
        &self,
        d_key_public: &jubjub::SubgroupPoint,
        token_private: &Vec<u8>,
        token_public: &Vec<u8>,
        asset_id: &jubjub::Fr,
    ) -> Result<()> {
        debug!(target: "CASHIERDB", "Put exchange keys");

        let d_key_public = self.get_value_serialized(d_key_public)?;

        // open connection
        let conn = Connection::open(&self.path)?;
        // unlock database
        conn.pragma_update(None, "key", &self.password)?;

        let asset_id = self.get_value_serialized(asset_id)?;

        conn.execute(
            "INSERT INTO deposit_keypairs(d_key_public, token_key_private, token_public_key_public, asset_id)
            VALUES (:d_key_public, :token_key_private, :token_key_public, :asset_id)",
            named_params! {
                ":d_key_public": d_key_public,
                ":token_key_private": token_private,
                ":token_key_public": token_public,
                ":asset_id": asset_id,
            },
        )?;
        Ok(())
    }

    // TODO convert this to generic function work with different tokens
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

        let confirm = self.get_value_serialized(&false)?;

        let mut stmt =
            conn.prepare("SELECT d_key_private FROM withdraw_keypairs WHERE confirm = :confirm")?;
        let keys = stmt.query_map(&[(":confirm", &confirm)], |row| {
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

    pub fn get_withdraw_keys_by_token_public_key(
        &self,
        token_public_key: &Vec<u8>,
        asset_id: &jubjub::Fr,
    ) -> Result<Option<Keypair>> {
        debug!(target: "CASHIERDB", "Check for existing token address");
        // open connection
        let conn = Connection::open(&self.path)?;
        // unlock database
        conn.pragma_update(None, "key", &self.password)?;

        let confirm = self.get_value_serialized(&false)?;

        let asset_id = self.get_value_serialized(asset_id)?;

        let mut stmt =
            conn.prepare(
                "SELECT * FROM withdraw_keypairs WHERE token_key_id = :token_key_id AND asset_id = :asset_id AND confirm = :confirm;")?;

        let addr_iter = stmt.query_map::<Keypair, _, _>(
            &[
                (":token_key_id", &token_public_key),
                (":asset_id", &&asset_id),
                (":confirm", &&confirm),
            ],
            |row| {
                let public: jubjub::SubgroupPoint = self
                    .get_value_deserialized(row.get(2)?)
                    .expect("get public key deserialize");
                let private: jubjub::Fr = self
                    .get_value_deserialized(row.get(1)?)
                    .expect("get  private key deserialize");
                Ok(Keypair { public, private })
            },
        )?;

        let mut addresses: Vec<Keypair> = vec![];

        for addr in addr_iter {
            addresses.push(addr?);
        }

        Ok(addresses.pop())
    }

    pub fn get_withdraw_token_public_key_by_dkey_public(
        &self,
        pub_key: &jubjub::SubgroupPoint,
    ) -> Result<Option<(Vec<u8>, jubjub::Fr)>> {
        debug!(target: "CASHIERDB", "Get token address by pub_key");
        // open connection
        let conn = Connection::open(&self.path)?;
        // unlock database
        conn.pragma_update(None, "key", &self.password)?;

        let d_key_public = self.get_value_serialized(pub_key)?;

        let confirm = self.get_value_serialized(&false)?;

        let mut stmt = conn.prepare(
            "SELECT token_key_id, asset_id FROM withdraw_keypairs WHERE d_key_public = :d_key_public AND confirm = :confirm;",
        )?;
        let addr_iter = stmt.query_map::<(Vec<u8>, jubjub::Fr), _, _>(
            &[(":d_key_public", &d_key_public), (":confirm", &&confirm)],
            |row| {
                let token_public_key = row.get(0)?;
                let asset_id = row.get(1)?;
                let asset_id: jubjub::Fr = self
                    .get_value_deserialized(asset_id)
                    .expect("deserialize asset_id");
                Ok((token_public_key, asset_id))
            },
        )?;

        let mut token_addresses = vec![];

        for addr in addr_iter {
            token_addresses.push(addr?);
        }

        Ok(token_addresses.pop())
    }

    pub fn confirm_withdraw_key_record(
        &self,
        token_address: &Vec<u8>,
        asset_id: &jubjub::Fr,
    ) -> Result<()> {
        debug!(target: "CASHIERDB", "Confirm withdraw keys");

        // open connection
        let conn = Connection::open(&self.path)?;
        // unlock database
        conn.pragma_update(None, "key", &self.password)?;

        let asset_id = self.get_value_serialized(asset_id)?;

        let confirm = self.get_value_serialized(&true)?;

        conn.execute(
            "UPDATE withdraw_keypairs SET confirm = ?1  WHERE token_key_id = ?2 AND asset_id = ?3;",
            params![confirm, token_address, asset_id],
        )?;

        Ok(())
    }

    pub fn put_withdraw_keys(
        &self,
        token_key_id: &Vec<u8>,
        d_key_public: &jubjub::SubgroupPoint,
        d_key_private: &jubjub::Fr,
        asset_id: &jubjub::Fr,
    ) -> Result<()> {
        debug!(target: "CASHIERDB", "Put withdraw keys");

        let d_key_public = self.get_value_serialized(d_key_public)?;
        let d_key_private = self.get_value_serialized(d_key_private)?;
        let asset_id = self.get_value_serialized(asset_id)?;

        // open connection
        let conn = Connection::open(&self.path)?;
        // unlock database
        conn.pragma_update(None, "key", &self.password)?;

        let confirm = self.get_value_serialized(&false)?;

        conn.execute(
            "INSERT INTO withdraw_keypairs(token_key_id, d_key_private, d_key_public, asset_id, confirm)
            VALUES (:token_key_id, :d_key_private, :d_key_public, :asset_id, :confirm)",
            named_params! {
                ":token_key_id": token_key_id,
                ":d_key_private": d_key_private,
                ":d_key_public": d_key_public,
                ":asset_id": asset_id,
                ":confirm": confirm,
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
    pub fn test_put_withdraw_keys_and_load_them_with_token_key() -> Result<()> {
        let walletdb_path = join_config_path(&PathBuf::from("cashier_wallet_test.db"))?;
        let wallet = CashierDb::new(&walletdb_path, "darkfi".into())?;
        wallet.init_db()?;

        let secret2: jubjub::Fr = jubjub::Fr::random(&mut OsRng);
        let public2 = zcash_primitives::constants::SPENDING_KEY_GENERATOR * secret2;

        // btc addr testnet
        let token_addr = serialize(&String::from("mxVFsFW5N4mu1HPkxPttorvocvzeZ7KZyk"));

        let asset_id: jubjub::Fr = jubjub::Fr::random(&mut OsRng);

        wallet.put_withdraw_keys(&token_addr, &public2, &secret2, &asset_id)?;

        let addr = wallet.get_withdraw_keys_by_token_public_key(&token_addr, &asset_id)?;

        assert_eq!(addr.is_some(), true);

        wallet.confirm_withdraw_key_record(&token_addr, &asset_id)?;

        let addr = wallet.get_withdraw_keys_by_token_public_key(&token_addr, &asset_id)?;

        assert_eq!(addr.is_none(), true);

        wallet.destroy()?;

        Ok(())
    }
}
