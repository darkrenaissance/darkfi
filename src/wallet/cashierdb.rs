use async_std::sync::{Arc, Mutex};
use std::path::{Path, PathBuf};

use log::*;
use rusqlite::{named_params, params, Connection};

use super::{Keypair, WalletApi};
use crate::client::ClientFailed;
use crate::util::NetworkName;
use crate::{Error, Result};

pub type CashierDbPtr = Arc<CashierDb>;

pub struct CashierDb {
    pub path: PathBuf,
    pub password: String,
    pub initialized: Mutex<bool>,
}

#[derive(Debug, Clone)]
pub struct TokenKey {
    pub public_key: Vec<u8>,
    pub private_key: Vec<u8>,
}

pub struct WithdrawToken {
    pub token_public_key: Vec<u8>,
    pub network: NetworkName,
    pub token_id: jubjub::Fr,
    pub mint_address: String,
}

pub struct DepositToken {
    pub drk_public_key: jubjub::SubgroupPoint,
    pub token_key: TokenKey,
    pub token_id: jubjub::Fr,
    pub mint_address: String,
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
        if !*self.initialized.lock().await {
            if !self.password.trim().is_empty() {
                let contents = include_str!("../../sql/cashier.sql");
                let conn = Connection::open(&self.path)?;
                debug!(target: "CASHIERDB", "Opened connection at path {:?}", self.path);
                conn.pragma_update(None, "key", &self.password)?;
                conn.execute_batch(contents)?;
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

    pub fn put_main_keys(&self, token_key: &TokenKey, network: &NetworkName) -> Result<()> {
        debug!(target: "CASHIERDB", "Put main keys");

        // open connection
        let conn = Connection::open(&self.path)?;
        // unlock database
        conn.pragma_update(None, "key", &self.password)?;

        let network = self.get_value_serialized(network)?;

        conn.execute(
            "INSERT INTO main_keypairs
            (token_key_private, token_key_public, network)
            VALUES
            (:token_key_private, :token_key_public, :network)",
            named_params! {
                ":token_key_private": token_key.private_key,
                ":token_key_public": token_key.public_key,
                ":network": &network,
            },
        )?;
        Ok(())
    }

    pub fn get_main_keys(&self, network: &NetworkName) -> Result<Vec<TokenKey>> {
        debug!(target: "CASHIERDB", "Get main keys");
        // open connection
        let conn = Connection::open(&self.path)?;
        // unlock database
        conn.pragma_update(None, "key", &self.password)?;

        let network = self.get_value_serialized(network)?;

        let mut stmt = conn.prepare(
            "SELECT token_key_private, token_key_public
            FROM main_keypairs
            WHERE network = :network ;",
        )?;
        let keys_iter = stmt
            .query_map::<(Vec<u8>, Vec<u8>), _, _>(&[(":network", &network)], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })?;

        let mut keys = vec![];

        for k in keys_iter {
            let k = k?;
            keys.push(TokenKey {
                private_key: k.0,
                public_key: k.1,
            });
        }

        Ok(keys)
    }

    pub fn put_withdraw_keys(
        &self,
        token_key_public: &[u8],
        d_key_public: &jubjub::SubgroupPoint,
        d_key_private: &jubjub::Fr,
        network: &NetworkName,
        token_id: &jubjub::Fr,
        mint_address: String,
    ) -> Result<()> {
        debug!(target: "CASHIERDB", "Put withdraw keys");

        let d_key_public = self.get_value_serialized(d_key_public)?;
        let d_key_private = self.get_value_serialized(d_key_private)?;
        let network = self.get_value_serialized(network)?;
        let token_id = self.get_value_serialized(token_id)?;
        let confirm = self.get_value_serialized(&false)?;
        let mint_address = self.get_value_serialized(&mint_address)?;

        // open connection
        let conn = Connection::open(&self.path)?;
        // unlock database
        conn.pragma_update(None, "key", &self.password)?;

        conn.execute(
            "INSERT INTO withdraw_keypairs
            (token_key_public, d_key_private, d_key_public, network,  token_id, mint_address, confirm)
            VALUES
            (:token_key_public, :d_key_private, :d_key_public,:network, :token_id, :mint_address, :confirm);",
            named_params! {
                ":token_key_public": token_key_public,
                ":d_key_private": d_key_private,
                ":d_key_public": d_key_public,
                ":network": network,
                ":token_id": token_id,
                ":mint_address": mint_address,
                ":confirm": confirm,
            },
        )?;
        Ok(())
    }

    pub fn put_deposit_keys(
        &self,
        d_key_public: &jubjub::SubgroupPoint,
        token_key_private: &[u8],
        token_key_public: &[u8],
        network: &NetworkName,
        token_id: &jubjub::Fr,
        mint_address: String,
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

        let mint_address = self.get_value_serialized(&mint_address)?;

        conn.execute(
            "INSERT INTO deposit_keypairs
            (d_key_public, token_key_private, token_key_public, network, token_id, mint_address, confirm)
            VALUES
            (:d_key_public, :token_key_private, :token_key_public, :network, :token_id, :mint_address, :confirm)",
            named_params! {
                ":d_key_public": &d_key_public,
                ":token_key_private": token_key_private,
                ":token_key_public": token_key_public,
                ":network": &network,
                ":token_id": &token_id,
                ":mint_address": &mint_address,
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

        let keys = stmt.query_map(&[(":confirm", &confirm)], |row| Ok(row.get(0)))?;

        let mut private_keys: Vec<jubjub::Fr> = vec![];

        for k in keys {
            let private_key: jubjub::Fr = self.get_value_deserialized(&k??)?;
            private_keys.push(private_key);
        }

        Ok(private_keys)
    }

    pub fn get_withdraw_token_public_key_by_dkey_public(
        &self,
        pub_key: &jubjub::SubgroupPoint,
    ) -> Result<Option<WithdrawToken>> {
        debug!(target: "CASHIERDB", "Get token address by pub_key");
        // open connection
        let conn = Connection::open(&self.path)?;
        // unlock database
        conn.pragma_update(None, "key", &self.password)?;

        let d_key_public = self.get_value_serialized(pub_key)?;

        let confirm = self.get_value_serialized(&false)?;

        let mut stmt = conn.prepare(
            "SELECT token_key_public, network, token_id, mint_address
            FROM withdraw_keypairs
            WHERE d_key_public = :d_key_public AND confirm = :confirm;",
        )?;
        let addr_iter = stmt.query_map(
            &[(":d_key_public", &d_key_public), (":confirm", &confirm)],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?;

        let mut token_addresses = vec![];

        for addr in addr_iter {
            let addr = addr?;
            let token_public_key = addr.0;
            let network: NetworkName = self.get_value_deserialized(&addr.1)?;
            let token_id: jubjub::Fr = self.get_value_deserialized(&addr.2)?;
            let mint_address: String = self.get_value_deserialized(&addr.3)?;
            token_addresses.push(WithdrawToken {
                token_public_key,
                network,
                token_id,
                mint_address,
            });
        }

        Ok(token_addresses.pop())
    }

    pub fn get_deposit_token_keys_by_dkey_public(
        &self,
        d_key_public: &jubjub::SubgroupPoint,
        network: &NetworkName,
    ) -> Result<Vec<TokenKey>> {
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
            let k = k?;
            keys.push(TokenKey {
                private_key: k.0,
                public_key: k.1,
            });
        }

        Ok(keys)
    }

    pub fn get_deposit_token_keys_by_network(
        &self,
        network: &NetworkName,
    ) -> Result<Vec<DepositToken>> {
        debug!(target: "CASHIERDB", "Check for existing dkey");
        // open connection
        let conn = Connection::open(&self.path)?;
        // unlock database
        conn.pragma_update(None, "key", &self.password)?;

        let network = self.get_value_serialized(network)?;
        let confirm = self.get_value_serialized(&false)?;

        let mut stmt = conn.prepare(
            "SELECT d_key_public, token_key_private, token_key_public, token_id, mint_address
            FROM deposit_keypairs
            WHERE network = :network
            AND confirm = :confirm ;",
        )?;
        let keys_iter =
            stmt.query_map(&[(":network", &network), (":confirm", &confirm)], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            })?;

        let mut keys = vec![];

        for key in keys_iter {
            let key = key?;
            let drk_public_key: jubjub::SubgroupPoint = self.get_value_deserialized(&key.0)?;
            let private_key = key.1;
            let public_key = key.2;
            let token_id: jubjub::Fr = self.get_value_deserialized(&key.3)?;
            let mint_address: String = self.get_value_deserialized(&key.4)?;
            keys.push(DepositToken {
                drk_public_key,
                token_key: TokenKey {
                    private_key,
                    public_key,
                },
                token_id,
                mint_address,
            });
        }

        Ok(keys)
    }

    pub fn get_withdraw_keys_by_token_public_key(
        &self,
        token_key_public: &[u8],
        network: &NetworkName,
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
                (":network", &network.as_ref()),
                (":confirm", &confirm.as_ref()),
            ],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;

        let mut keypairs: Vec<Keypair> = vec![];

        for kp in keypair_iter {
            let kp = kp?;
            let public: jubjub::SubgroupPoint = self.get_value_deserialized(&kp.1)?;
            let private: jubjub::Fr = self.get_value_deserialized(&kp.0)?;
            let keypair = Keypair { public, private };
            keypairs.push(keypair);
        }

        Ok(keypairs.pop())
    }

    pub fn confirm_withdraw_key_record(
        &self,
        token_address: &[u8],
        network: &NetworkName,
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
        network: &NetworkName,
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

    pub fn init_db(path: &Path, password: String) -> Result<()> {
        if !password.trim().is_empty() {
            let contents = include_str!("../../sql/cashier.sql");
            let conn = Connection::open(path)?;
            debug!(target: "CASHIERDB", "OPENED CONNECTION AT PATH {:?}", path);
            conn.pragma_update(None, "key", &password)?;
            conn.execute_batch(contents)?;
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

        let network = NetworkName::Bitcoin;

        wallet.put_main_keys(
            &TokenKey {
                private_key: token_addr_private.clone(),
                public_key: token_addr.clone(),
            },
            &network,
        )?;

        let keys = wallet.get_main_keys(&network)?;

        assert_eq!(keys.len(), 1);

        assert_eq!(keys[0].private_key, token_addr_private);
        assert_eq!(keys[0].public_key, token_addr);

        std::fs::remove_file(walletdb_path)?;

        Ok(())
    }

    #[test]
    pub fn test_put_deposit_keys_and_load_them() -> Result<()> {
        let walletdb_path = join_config_path(&PathBuf::from("cashier_wallet_test3.db"))?;
        let password: String = "darkfi".into();
        let wallet = CashierDb::new(&walletdb_path, password.clone())?;
        init_db(&walletdb_path, password)?;

        // btc addr testnet
        let token_addr = serialize(&String::from("mxVFsFW5N4mu1HPkxPttorvocvzeZ7KZyk"));
        let token_addr_private = serialize(&String::from("2222222222222222222222222222222222"));

        let network = NetworkName::Bitcoin;

        let secret2: jubjub::Fr = jubjub::Fr::random(&mut OsRng);
        let public2 = zcash_primitives::constants::SPENDING_KEY_GENERATOR * secret2;
        let token_id: jubjub::Fr = jubjub::Fr::random(&mut OsRng);

        wallet.put_deposit_keys(
            &public2,
            &token_addr_private,
            &token_addr,
            &network,
            &token_id,
            String::new(),
        )?;

        let keys = wallet.get_deposit_token_keys_by_dkey_public(&public2, &network)?;

        assert_eq!(keys.len(), 1);

        assert_eq!(keys[0].private_key, token_addr_private);
        assert_eq!(keys[0].public_key, token_addr);

        let resumed_keys = wallet.get_deposit_token_keys_by_network(&network)?;

        assert_eq!(resumed_keys[0].drk_public_key, public2);
        assert_eq!(resumed_keys[0].token_key.private_key, token_addr_private);
        assert_eq!(resumed_keys[0].token_key.public_key, token_addr);
        assert_eq!(resumed_keys[0].token_id, token_id);

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

        let network = NetworkName::Bitcoin;

        wallet.put_withdraw_keys(
            &token_addr,
            &public2,
            &secret2,
            &network,
            &token_id,
            String::new(),
        )?;

        let addr = wallet.get_withdraw_keys_by_token_public_key(&token_addr, &network)?;

        assert!(addr.is_some());

        wallet.confirm_withdraw_key_record(&token_addr, &network)?;

        let addr = wallet.get_withdraw_keys_by_token_public_key(&token_addr, &network)?;

        assert!(addr.is_none());

        std::fs::remove_file(walletdb_path)?;

        Ok(())
    }
}
