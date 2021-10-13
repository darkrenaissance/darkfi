use super::WalletApi;
use crate::client::ClientFailed;
use crate::crypto::{
    merkle::IncrementalWitness, merkle_node::MerkleNode, note::Note, OwnCoin, OwnCoins,
};
use crate::serial;
use crate::{Error, Result};

use async_std::sync::{Arc, Mutex};
use ff::Field;
use log::*;
use rand::rngs::OsRng;
use rusqlite::{named_params, params, Connection};

use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub type WalletPtr = Arc<WalletDb>;

#[derive(Debug, Clone)]
pub struct Keypair {
    pub public: jubjub::SubgroupPoint,
    pub private: jubjub::Fr,
}

//#[derive(Clone)]
pub struct WalletDb {
    pub path: PathBuf,
    pub password: String,
    pub initialized: Mutex<bool>,
}

impl WalletApi for WalletDb {
    fn get_password(&self) -> String {
        self.password.to_owned()
    }
    fn get_path(&self) -> PathBuf {
        self.path.to_owned()
    }
}

impl WalletDb {
    pub fn new(path: &Path, password: String) -> Result<WalletPtr> {
        debug!(target: "WALLETDB", "new() Constructor called");
        Ok(Arc::new(Self {
            path: path.to_owned(),
            password,
            initialized: Mutex::new(false),
        }))
    }

    pub async fn init_db(&self) -> Result<()> {
        if !*self.initialized.lock().await {
            if !self.password.trim().is_empty() {
                let contents = include_str!("../../sql/schema.sql");
                let conn = Connection::open(&self.path)?;
                debug!(target: "WALLETDB", "OPENED CONNECTION AT PATH {:?}", self.path);
                conn.pragma_update(None, "key", &self.password)?;
                conn.execute_batch(&contents)?;
                *self.initialized.lock().await = true;
            } else {
                debug!(
                    target: "WALLETDB",
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

    pub fn key_gen(&self) -> Result<()> {
        debug!(target: "WALLETDB", "Attempting to generate keys...");
        let conn = Connection::open(&self.path)?;
        conn.pragma_update(None, "key", &self.password)?;
        let mut stmt = conn.prepare("SELECT * FROM keys WHERE key_id > ?")?;
        let key_check = stmt.exists(params!["0"])?;
        if !key_check {
            let secret: jubjub::Fr = jubjub::Fr::random(&mut OsRng);
            let public = zcash_primitives::constants::SPENDING_KEY_GENERATOR * secret;
            let pubkey = serial::serialize(&public);
            let privkey = serial::serialize(&secret);
            self.put_keypair(pubkey, privkey)?;
        } else {
            debug!(target: "WALLETDB", "Keys already exist.");
            return Err(Error::from(ClientFailed::KeyExists));
        }
        Ok(())
    }

    pub fn put_keypair(&self, key_public: Vec<u8>, key_private: Vec<u8>) -> Result<()> {
        let conn = Connection::open(&self.path)?;
        conn.pragma_update(None, "key", &self.password)?;
        conn.execute(
            "INSERT INTO keys(key_public, key_private) VALUES (?1, ?2)",
            params![key_public, key_private],
        )?;
        Ok(())
    }

    pub fn get_keypairs(&self) -> Result<Vec<Keypair>> {
        debug!(target: "WALLETDB", "Returning keypairs...");
        let conn = Connection::open(&self.path)?;
        conn.pragma_update(None, "key", &self.password)?;
        let mut stmt = conn.prepare("SELECT * FROM keys")?;
        // this just gets the first key. maybe we should randomize this
        let key_iter = stmt.query_map([], |row| Ok((row.get(1)?, row.get(2)?)))?;
        let mut keypairs = Vec::new();

        for key in key_iter {
            let key = key?;
            let public = key.0;
            let private = key.1;
            let public: jubjub::SubgroupPoint = self.get_value_deserialized(&public)?;
            let private: jubjub::Fr = self.get_value_deserialized(&private)?;
            keypairs.push(Keypair { public, private });
        }

        Ok(keypairs)
    }

    pub fn get_own_coins(&self) -> Result<OwnCoins> {
        debug!(target: "WALLETDB", "Get own coins");

        let is_spent = self.get_value_serialized(&false)?;

        let conn = Connection::open(&self.path)?;
        // unlock database
        conn.pragma_update(None, "key", &self.password)?;

        let mut coins = conn.prepare("SELECT * FROM coins WHERE is_spent = :is_spent ;")?;
        let rows = coins.query_map(&[(":is_spent", &is_spent)], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
                row.get(7)?,
                row.get(8)?,
            ))
        })?;

        let mut own_coins = Vec::new();

        for row in rows {
            let row = row?;
            let coin_id: u64 = row.0;
            let coin = self.get_value_deserialized(&row.1)?;

            // note
            let serial = self.get_value_deserialized(&row.2)?;
            let coin_blind = self.get_value_deserialized(&row.3)?;
            let valcom_blind = self.get_value_deserialized(&row.4)?;
            let value: u64 = row.5;
            let token_id = self.get_value_deserialized(&row.6)?;

            let note = Note {
                serial,
                value,
                token_id,
                coin_blind,
                valcom_blind,
            };

            let witness = self.get_value_deserialized(&row.7)?;
            let secret: jubjub::Fr = self.get_value_deserialized(&row.8)?;

            let oc = OwnCoin {
                coin,
                note,
                secret,
                witness,
            };

            own_coins.push((coin_id, oc))
        }

        Ok(own_coins)
    }

    pub fn put_own_coins(&self, own_coin: OwnCoin) -> Result<()> {
        // prepare the values

        debug!(target: "WALLETDB", "Put own coins");

        let coin = self.get_value_serialized(&own_coin.coin.repr)?;
        let serial = self.get_value_serialized(&own_coin.note.serial)?;
        let coin_blind = self.get_value_serialized(&own_coin.note.coin_blind)?;
        let valcom_blind = self.get_value_serialized(&own_coin.note.valcom_blind)?;
        let value: u64 = own_coin.note.value;
        let token_id = self.get_value_serialized(&own_coin.note.token_id)?;
        let witness = self.get_value_serialized(&own_coin.witness)?;
        let secret = self.get_value_serialized(&own_coin.secret)?;
        let is_spent = self.get_value_serialized(&false)?;

        // open connection
        let conn = Connection::open(&self.path)?;
        // unlock database
        conn.pragma_update(None, "key", &self.password)?;

        conn.execute(
            "INSERT INTO coins
            (coin, serial, value, token_id, coin_blind, valcom_blind, witness, secret, is_spent)
            VALUES
            (:coin, :serial, :value, :token_id, :coin_blind, :valcom_blind, :witness, :secret, :is_spent);",
            named_params! {
                ":coin": coin,
                ":serial": serial,
                ":value": value,
                ":token_id": token_id,
                ":coin_blind": coin_blind,
                ":valcom_blind": valcom_blind,
                ":witness": witness,
                ":secret": secret,
                ":is_spent": is_spent,
            },
        )?;
        Ok(())
    }

    pub fn confirm_spend_coin(&self, coin_id: &u64) -> Result<()> {
        debug!(target: "WALLETDB", "Confirm spend coin");

        // open connection
        let conn = Connection::open(&self.path)?;
        // unlock database
        conn.pragma_update(None, "key", &self.password)?;

        let is_spent = self.get_value_serialized(&true)?;

        conn.execute(
            "UPDATE coins 
            SET is_spent = ?1
            WHERE coin_id = ?2 ;",
            params![is_spent, coin_id],
        )?;

        Ok(())
    }

    pub fn get_witnesses(&self) -> Result<Vec<(u64, IncrementalWitness<MerkleNode>)>> {
        let conn = Connection::open(&self.path)?;
        conn.pragma_update(None, "key", &self.password)?;

        let is_spent = self.get_value_serialized(&false)?;

        let mut witnesses =
            conn.prepare("SELECT coin_id, witness FROM coins WHERE is_spent = :is_spent;")?;

        let rows = witnesses.query_map(&[(":is_spent", &is_spent)], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?;

        let mut witnesses = Vec::new();
        for i in rows {
            let i = i?;
            let coin_id: u64 = i.0;
            let witness: IncrementalWitness<MerkleNode> = self.get_value_deserialized(&i.1)?;
            witnesses.push((coin_id, witness))
        }

        Ok(witnesses)
    }

    pub fn update_witness(
        &self,
        coin_id: u64,
        witness: IncrementalWitness<MerkleNode>,
    ) -> Result<()> {
        debug!(target: "WALLETDB", "Updating witness");

        let conn = Connection::open(&self.path)?;
        conn.pragma_update(None, "key", &self.password)?;

        let witness = self.get_value_serialized(&witness)?;
        let is_spent = self.get_value_serialized(&false)?;

        conn.execute(
            "UPDATE coins SET witness = ?1  WHERE coin_id = ?2 AND is_spent = ?3",
            params![witness, coin_id, is_spent],
        )?;

        Ok(())
    }

    pub fn put_cashier_pub(&self, key_public: &jubjub::SubgroupPoint) -> Result<()> {
        debug!(target: "WALLETDB", "Save cashier keys...");
        let conn = Connection::open(&self.path)?;
        conn.pragma_update(None, "key", &self.password)?;

        let key_public = self.get_value_serialized(key_public)?;

        conn.execute(
            "INSERT INTO cashier(key_public) VALUES (?1)",
            params![key_public],
        )?;
        Ok(())
    }

    //pub fn get_cashier_public_keys(&self) -> Result<Vec<jubjub::SubgroupPoint>> {
    //    debug!(target: "WALLETDB", "Returning Cashier Public key...");
    //    let conn = Connection::open(&self.path)?;
    //    conn.pragma_update(None, "key", &self.password)?;

    //    let mut stmt = conn.prepare("SELECT key_public FROM cashier")?;

    //    let key_iter = stmt.query_map([], |row| row.get(0))?;

    //    let mut pub_keys = Vec::new();

    //    for key in key_iter {
    //        let public: jubjub::SubgroupPoint = self.get_value_deserialized(&key?)?;
    //        pub_keys.push(public);
    //    }

    //    Ok(pub_keys)
    //}

    pub fn get_balances(&self) -> Result<HashMap<Vec<u8>, u64>> {
        debug!(target: "WALLETDB", "Get token and balances...");
        let conn = Connection::open(&self.path)?;
        conn.pragma_update(None, "key", &self.password)?;

        let is_spent = self.get_value_serialized(&false)?;

        let mut stmt =
            conn.prepare("SELECT value, token_id FROM coins  WHERE is_spent = :is_spent ;")?;
        let rows = stmt.query_map(&[(":is_spent", &is_spent)], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?;

        let mut balances = HashMap::new();

        for row in rows {
            let row = row?;
            let value: u64 = row.0;
            let token_id: Vec<u8> = row.1;

            if let Some(val) = balances.get_mut(&token_id) {
                *val += value;
            } else {
                balances.insert(token_id, value);
            }
        }
        Ok(balances)
    }

    pub fn get_token_id(&self) -> Result<Vec<jubjub::Fr>> {
        debug!(target: "WALLETDB", "Get token ID...");
        let conn = Connection::open(&self.path)?;
        conn.pragma_update(None, "key", &self.password)?;

        let is_spent = self.get_value_serialized(&false)?;

        let mut stmt = conn.prepare("SELECT token_id FROM coins WHERE is_spent = :is_spent ;")?;
        let rows = stmt.query_map(&[(":is_spent", &is_spent)], |row| row.get(0))?;

        let mut token_ids = Vec::new();
        for row in rows {
            let row = row?;
            let token_id = self.get_value_deserialized(&row).unwrap();

            token_ids.push(token_id);
        }

        Ok(token_ids)
    }

    pub fn token_id_exists(&self, token_id: &jubjub::Fr) -> Result<bool> {
        debug!(target: "WALLETDB", "Check tokenID exists");
        let conn = Connection::open(&self.path)?;
        conn.pragma_update(None, "key", &self.password)?;

        let id = self.get_value_serialized(token_id)?;
        let is_spent = self.get_value_serialized(&false)?;

        let mut stmt = conn.prepare("SELECT * FROM coins WHERE token_id = ? AND is_spent = ? ;")?;
        let id_check = stmt.exists(params![id, is_spent])?;
        Ok(id_check)
    }

    pub fn test_wallet(&self) -> Result<()> {
        let conn = Connection::open(&self.path)?;
        conn.pragma_update(None, "key", &self.password)?;
        let mut stmt = conn.prepare("SELECT * FROM keys")?;
        let _rows = stmt.query([])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::crypto::{coin::Coin, OwnCoin};
    use crate::serial::serialize;
    use crate::util::join_config_path;
    use ff::PrimeField;

    pub fn init_db(path: &PathBuf, password: String) -> Result<()> {
        if !password.trim().is_empty() {
            let contents = include_str!("../../sql/schema.sql");
            let conn = Connection::open(&path)?;
            debug!(target: "WALLETDB", "OPENED CONNECTION AT PATH {:?}", path);
            conn.pragma_update(None, "key", &password)?;
            conn.execute_batch(&contents)?;
        } else {
            debug!(
                target: "WALLETDB", "Password is empty. You must set a password to use the wallet."
            );
            return Err(Error::from(ClientFailed::EmptyPassword));
        }
        Ok(())
    }

    #[test]
    pub fn test_get_token_id() -> Result<()> {
        let walletdb_path = join_config_path(&PathBuf::from("test_wallet.db"))?;
        let password: String = "darkfi".into();
        let wallet = WalletDb::new(&walletdb_path, password.clone())?;
        init_db(&walletdb_path, password)?;

        let secret: jubjub::Fr = jubjub::Fr::random(&mut OsRng);
        let public = zcash_primitives::constants::SPENDING_KEY_GENERATOR * secret;
        let key_public = serial::serialize(&public);
        let key_private = serial::serialize(&secret);

        wallet.put_keypair(key_public, key_private)?;

        let token_id = jubjub::Fr::random(&mut OsRng);

        let note = Note {
            serial: jubjub::Fr::random(&mut OsRng),
            value: 110,
            token_id,
            coin_blind: jubjub::Fr::random(&mut OsRng),
            valcom_blind: jubjub::Fr::random(&mut OsRng),
        };

        let coin = Coin::new(bls12_381::Scalar::random(&mut OsRng).to_repr());

        let mut tree = crate::crypto::merkle::CommitmentTree::empty();
        tree.append(MerkleNode::from_coin(&coin))?;

        let witness = IncrementalWitness::from_tree(&tree);

        let own_coin = OwnCoin {
            coin,
            note: note.clone(),
            secret,
            witness: witness.clone(),
        };

        wallet.put_own_coins(own_coin.clone())?;
        wallet.put_own_coins(own_coin.clone())?;
        wallet.put_own_coins(own_coin.clone())?;
        wallet.put_own_coins(own_coin.clone())?;

        let id = wallet.get_token_id()?;

        assert_eq!(id.len(), 4);

        for i in id {
            assert_eq!(i, token_id);
            assert!(wallet.token_id_exists(&i)?);
        }

        std::fs::remove_file(walletdb_path)?;

        Ok(())
    }

    #[test]
    pub fn test_get_balances() -> Result<()> {
        let walletdb_path = join_config_path(&PathBuf::from("test2_wallet.db"))?;
        let password: String = "darkfi".into();
        let wallet = WalletDb::new(&walletdb_path, password.clone())?;
        init_db(&walletdb_path, password)?;

        let secret: jubjub::Fr = jubjub::Fr::random(&mut OsRng);
        let public = zcash_primitives::constants::SPENDING_KEY_GENERATOR * secret;
        let key_public = serial::serialize(&public);
        let key_private = serial::serialize(&secret);

        wallet.put_keypair(key_public, key_private)?;

        let token_id = jubjub::Fr::random(&mut OsRng);

        let note = Note {
            serial: jubjub::Fr::random(&mut OsRng),
            value: 110,
            token_id,
            coin_blind: jubjub::Fr::random(&mut OsRng),
            valcom_blind: jubjub::Fr::random(&mut OsRng),
        };

        let coin = Coin::new(bls12_381::Scalar::random(&mut OsRng).to_repr());

        let mut tree = crate::crypto::merkle::CommitmentTree::empty();
        tree.append(MerkleNode::from_coin(&coin))?;

        let witness = IncrementalWitness::from_tree(&tree);

        let own_coin = OwnCoin {
            coin,
            note: note.clone(),
            secret,
            witness: witness.clone(),
        };

        wallet.put_own_coins(own_coin.clone())?;
        wallet.put_own_coins(own_coin.clone())?;
        wallet.put_own_coins(own_coin.clone())?;
        wallet.put_own_coins(own_coin.clone())?;

        let balances = wallet.get_balances()?;

        let token_id = serialize(&token_id);

        assert_eq!(balances[&token_id], 440);

        std::fs::remove_file(walletdb_path)?;

        Ok(())
    }

    #[test]
    pub fn test_save_and_load_keypair() -> Result<()> {
        let walletdb_path = join_config_path(&PathBuf::from("test3_wallet.db"))?;
        let password: String = "darkfi".into();
        let wallet = WalletDb::new(&walletdb_path, password.clone())?;
        init_db(&walletdb_path, password)?;

        let secret: jubjub::Fr = jubjub::Fr::random(&mut OsRng);
        let public = zcash_primitives::constants::SPENDING_KEY_GENERATOR * secret;
        let key_public = serial::serialize(&public);
        let key_private = serial::serialize(&secret);

        wallet.put_keypair(key_public, key_private)?;

        let keypair = wallet.get_keypairs()?[0].clone();

        assert_eq!(public, keypair.public);
        assert_eq!(secret, keypair.private);

        std::fs::remove_file(walletdb_path)?;

        Ok(())
    }

    #[test]
    pub fn test_put_and_get_own_coins() -> Result<()> {
        let walletdb_path = join_config_path(&PathBuf::from("test4_wallet.db"))?;
        let password: String = "darkfi".into();
        let wallet = WalletDb::new(&walletdb_path, password.clone())?;
        init_db(&walletdb_path, password)?;

        let secret: jubjub::Fr = jubjub::Fr::random(&mut OsRng);
        let public = zcash_primitives::constants::SPENDING_KEY_GENERATOR * secret;
        let key_public = serial::serialize(&public);
        let key_private = serial::serialize(&secret);

        wallet.put_keypair(key_public, key_private)?;

        let note = Note {
            serial: jubjub::Fr::random(&mut OsRng),
            value: 110,
            token_id: jubjub::Fr::random(&mut OsRng),
            coin_blind: jubjub::Fr::random(&mut OsRng),
            valcom_blind: jubjub::Fr::random(&mut OsRng),
        };

        let coin = Coin::new(bls12_381::Scalar::random(&mut OsRng).to_repr());

        let mut tree = crate::crypto::merkle::CommitmentTree::empty();
        tree.append(MerkleNode::from_coin(&coin))?;

        let witness = IncrementalWitness::from_tree(&tree);

        let own_coin = OwnCoin {
            coin,
            note: note.clone(),
            secret,
            witness: witness.clone(),
        };
        wallet.put_own_coins(own_coin.clone())?;

        let own_coin = wallet.get_own_coins()?[0].clone();

        assert_eq!(&own_coin.1.note.valcom_blind, &note.valcom_blind);
        assert_eq!(&own_coin.1.note.coin_blind, &note.coin_blind);
        assert_eq!(own_coin.1.secret, secret);
        assert_eq!(own_coin.1.witness.root(), witness.root());
        assert_eq!(own_coin.1.witness.path(), witness.path());

        wallet.confirm_spend_coin(&own_coin.0)?;

        let own_coins = wallet.get_own_coins()?.clone();

        assert_eq!(own_coins.len(), 0);

        std::fs::remove_file(walletdb_path)?;

        Ok(())
    }

    #[test]
    pub fn test_get_witnesses_and_update_them() -> Result<()> {
        let walletdb_path = join_config_path(&PathBuf::from("test5_wallet.db"))?;
        let password: String = "darkfi".into();
        let wallet = WalletDb::new(&walletdb_path, password.clone())?;
        init_db(&walletdb_path, password)?;

        let secret: jubjub::Fr = jubjub::Fr::random(&mut OsRng);
        let public = zcash_primitives::constants::SPENDING_KEY_GENERATOR * secret;
        let key_public = serial::serialize(&public);
        let key_private = serial::serialize(&secret);

        wallet.put_keypair(key_public, key_private)?;

        let mut tree = crate::crypto::merkle::CommitmentTree::empty();

        let note = Note {
            serial: jubjub::Fr::random(&mut OsRng),
            value: 110,
            token_id: jubjub::Fr::random(&mut OsRng),
            coin_blind: jubjub::Fr::random(&mut OsRng),
            valcom_blind: jubjub::Fr::random(&mut OsRng),
        };

        let coin = Coin::new(bls12_381::Scalar::random(&mut OsRng).to_repr());

        let node = MerkleNode::from_coin(&coin);
        tree.append(node)?;
        tree.append(node)?;
        tree.append(node)?;
        tree.append(node)?;

        let witness = IncrementalWitness::from_tree(&tree);

        let own_coin = OwnCoin {
            coin,
            note,
            secret,
            witness,
        };

        wallet.put_own_coins(own_coin.clone())?;
        wallet.put_own_coins(own_coin.clone())?;
        wallet.put_own_coins(own_coin.clone())?;
        wallet.put_own_coins(own_coin.clone())?;

        let coin2 = Coin::new(bls12_381::Scalar::random(&mut OsRng).to_repr());

        let node2 = MerkleNode::from_coin(&coin2);
        tree.append(node2)?;

        for (coin_id, witness) in wallet.get_witnesses()?.iter_mut() {
            witness.append(node2).expect("Append to witness");
            wallet.update_witness(coin_id.clone(), witness.clone())?;
        }

        for (_, witness) in wallet.get_witnesses()?.iter() {
            assert_eq!(tree.root(), witness.root());
        }

        std::fs::remove_file(walletdb_path)?;

        Ok(())
    }

}
